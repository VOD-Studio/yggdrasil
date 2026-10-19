//! Owned SSE callbacks are detached before being dropped, including partial registration failure.
use super::{
    execution::Execution,
    state::{DonePayload, Outcome, Phase, RunState},
};
use dioxus::prelude::{ReadableExt, WritableExt};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{Event, EventSource, MessageEvent};

type Listener = (&'static str, Closure<dyn FnMut(Event)>);
pub(super) struct StreamHandle {
    source: EventSource,
    listeners: Vec<Listener>,
}
impl StreamHandle {
    fn listen(
        &mut self,
        name: &'static str,
        callback: impl FnMut(Event) + 'static,
    ) -> Result<(), JsValue> {
        let closure = Closure::new(callback);
        self.source
            .add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())?;
        self.listeners.push((name, closure));
        Ok(())
    }
}
impl Drop for StreamHandle {
    fn drop(&mut self) {
        self.source.close();
        for (name, callback) in &self.listeners {
            let _ = self
                .source
                .remove_event_listener_with_callback(name, callback.as_ref().unchecked_ref());
        }
    }
}

pub(super) fn start(id: &str, mut execution: Execution) -> Result<StreamHandle, JsValue> {
    let mut handle = StreamHandle {
        source: EventSource::new(&format!("/api/exec/stream?task_id={id}"))?,
        listeners: Vec::new(),
    };
    handle.listen("stage", move |event| {
        if !execution.state.peek().is_running() {
            return;
        }
        let phase = match event
            .dyn_ref::<MessageEvent>()
            .and_then(|e| e.data().as_string())
            .as_deref()
        {
            Some("queued") => Phase::Queued,
            Some("preparing") => Phase::Preparing,
            _ => Phase::Running,
        };
        execution.state.set(RunState::Busy(phase));
    })?;
    for channel in ["stdout", "stderr"] {
        handle.listen(channel, move |event| {
            if !execution.state.peek().is_running() {
                return;
            }
            if let Some(text) = event
                .dyn_ref::<MessageEvent>()
                .and_then(|e| e.data().as_string())
            {
                if let Some(terminal) = execution.terminal.read().as_ref() {
                    if channel == "stdout" {
                        terminal.instance().write_stdout(&text);
                    } else {
                        terminal.instance().write_stderr(&text);
                    }
                }
                execution.has_output.set(true);
                // 输出先于 stage 事件到达时也算已在运行。
                if *execution.state.peek() != RunState::Busy(Phase::Running) {
                    execution.state.set(RunState::Busy(Phase::Running));
                }
            }
        })?;
    }
    let source = handle.source.clone();
    handle.listen("done", move |event| {
        if !execution.state.peek().is_running() {
            return;
        }
        let outcome = event
            .dyn_ref::<MessageEvent>()
            .and_then(|e| e.data().as_string())
            .and_then(|data| serde_json::from_str::<DonePayload>(&data).ok())
            .map(DonePayload::outcome)
            .unwrap_or_else(|| {
                RunState::Finished(Outcome::SystemFailed("运行结果格式异常，请重试".into()))
            });
        execution.has_output.set(true);
        execution.state.set(outcome);
        source.close();
    })?;
    let source = handle.source.clone();
    handle.listen("error", move |_| {
        if execution.state.peek().is_running() {
            execution
                .state
                .set(RunState::Finished(Outcome::Disconnected));
        }
        source.close();
    })?;
    Ok(handle)
}

pub(super) async fn poll_result(id: &str, mut execution: Execution) {
    use crate::api::code_runner::{execute::get_exec_result, ExecStatus};
    for _ in 0..240 {
        crate::utils::time::sleep_ms(500).await;
        match get_exec_result(id.to_string()).await {
            Ok(task) if matches!(task.status, ExecStatus::Queued | ExecStatus::Running) => {
                execution
                    .state
                    .set(RunState::Busy(if task.status == ExecStatus::Queued {
                        Phase::Queued
                    } else {
                        Phase::Running
                    }));
            }
            Ok(task) => {
                if let Some(result) = task.result {
                    if let Some(terminal) = execution.terminal.read().as_ref() {
                        terminal
                            .instance()
                            .write_all(&result.stdout, &result.stderr);
                    }
                    execution.has_output.set(true);
                    execution.state.set(RunState::result(&result));
                } else {
                    execution
                        .state
                        .set(RunState::Finished(Outcome::SystemFailed(
                            "运行结果缺失，请重试".into(),
                        )));
                }
                return;
            }
            Err(_) => {
                execution
                    .state
                    .set(RunState::Finished(Outcome::Disconnected));
                return;
            }
        }
    }
    execution
        .state
        .set(RunState::Finished(Outcome::Disconnected));
}
