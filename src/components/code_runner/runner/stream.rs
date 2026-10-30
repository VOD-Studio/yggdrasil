//! Owned SSE callbacks are detached before being dropped, including partial registration failure.
use super::{
    execution::Execution,
    state::{DonePayload, Outcome, Phase, RunState},
};
use crate::bridges::xterm::TerminalHandle;
use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{Event, EventSource, MessageEvent};

type Listener = (&'static str, Closure<dyn FnMut(Event)>);

/// 终端句柄的本地存储（JS 句柄不是 `Send`，只能放 `StoredValue::new_local`）。
pub(super) type TerminalSlot = StoredValue<Option<TerminalHandle>, LocalStorage>;

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

/// 在终端（若已挂载）上执行一次操作；组件已卸载时静默跳过。
fn with_terminal(terminal: TerminalSlot, f: impl FnOnce(&TerminalHandle)) {
    let _ = terminal.try_with_value(|handle| {
        if let Some(handle) = handle.as_ref() {
            f(handle);
        }
    });
}

pub(super) fn start(
    id: &str,
    execution: Execution,
    terminal: TerminalSlot,
) -> Result<StreamHandle, JsValue> {
    let mut handle = StreamHandle {
        source: EventSource::new(&format!("/api/exec/stream?task_id={id}"))?,
        listeners: Vec::new(),
    };
    handle.listen("stage", move |event| {
        if !execution.is_running_untracked() {
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
        let _ = execution.state.try_set(RunState::Busy(phase));
    })?;
    for channel in ["stdout", "stderr"] {
        handle.listen(channel, move |event| {
            if !execution.is_running_untracked() {
                return;
            }
            if let Some(text) = event
                .dyn_ref::<MessageEvent>()
                .and_then(|e| e.data().as_string())
            {
                with_terminal(terminal, |terminal| {
                    if channel == "stdout" {
                        terminal.instance().write_stdout(&text);
                    } else {
                        terminal.instance().write_stderr(&text);
                    }
                });
                let _ = execution.has_output.try_set(true);
                // 输出先于 stage 事件到达时也算已在运行。
                if execution
                    .state
                    .try_with_untracked(|state| *state != RunState::Busy(Phase::Running))
                    .unwrap_or(false)
                {
                    let _ = execution.state.try_set(RunState::Busy(Phase::Running));
                }
            }
        })?;
    }
    let source = handle.source.clone();
    handle.listen("done", move |event| {
        if !execution.is_running_untracked() {
            return;
        }
        let outcome = event
            .dyn_ref::<MessageEvent>()
            .and_then(|e| e.data().as_string())
            .and_then(|data| serde_json::from_slice::<DonePayload>(data.as_bytes()).ok())
            .map(DonePayload::outcome)
            .unwrap_or_else(|| {
                RunState::Finished(Outcome::SystemFailed("运行结果格式异常，请重试".into()))
            });
        let _ = execution.has_output.try_set(true);
        let _ = execution.state.try_set(outcome);
        source.close();
    })?;
    let source = handle.source.clone();
    handle.listen("error", move |_| {
        if execution.is_running_untracked() {
            let _ = execution
                .state
                .try_set(RunState::Finished(Outcome::Disconnected));
        }
        source.close();
    })?;
    Ok(handle)
}

pub(super) async fn poll_result(id: &str, execution: Execution, terminal: TerminalSlot) {
    use crate::api::code_runner::{execute::get_exec_result, ExecStatus};
    for _ in 0..240 {
        crate::utils::time::sleep_ms(500).await;
        // 组件已卸载（信号已释放）就停止轮询。
        if execution.state.try_with_untracked(|_| ()).is_none() {
            return;
        }
        match get_exec_result(id.to_string()).await {
            Ok(task) if matches!(task.status, ExecStatus::Queued | ExecStatus::Running) => {
                let _ =
                    execution
                        .state
                        .try_set(RunState::Busy(if task.status == ExecStatus::Queued {
                            Phase::Queued
                        } else {
                            Phase::Running
                        }));
            }
            Ok(task) => {
                if let Some(result) = task.result {
                    with_terminal(terminal, |terminal| {
                        terminal
                            .instance()
                            .write_all(&result.stdout, &result.stderr);
                    });
                    let _ = execution.has_output.try_set(true);
                    let _ = execution.state.try_set(RunState::result(&result));
                } else {
                    let _ = execution
                        .state
                        .try_set(RunState::Finished(Outcome::SystemFailed(
                            "运行结果缺失，请重试".into(),
                        )));
                }
                return;
            }
            Err(_) => {
                let _ = execution
                    .state
                    .try_set(RunState::Finished(Outcome::Disconnected));
                return;
            }
        }
    }
    let _ = execution
        .state
        .try_set(RunState::Finished(Outcome::Disconnected));
}
