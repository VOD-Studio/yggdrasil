//! Owned SSE callbacks are detached before being dropped, including partial registration failure.
use super::{execution::Execution, state::RunState};
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

#[derive(serde::Deserialize)]
struct DonePayload {
    exit_code: Option<i64>,
    oom_killed: bool,
    timed_out: bool,
    duration_ms: u64,
    #[serde(default)]
    error: Option<String>,
}
impl DonePayload {
    fn outcome(self) -> RunState {
        let (label, error) = if let Some(error) = self.error {
            ("系统错误".into(), error)
        } else if self.timed_out {
            ("超时".into(), "超时".into())
        } else if self.oom_killed {
            ("内存超限".into(), "内存超限".into())
        } else if self.exit_code == Some(0) {
            ("成功".into(), String::new())
        } else {
            (
                format!("退出码: {} · 运行错误", self.exit_code.unwrap_or(-1)),
                "运行错误".into(),
            )
        };
        let summary = format!("耗时: {}ms · 状态: {label}", self.duration_ms);
        if error.is_empty() {
            RunState::Succeeded(summary)
        } else {
            RunState::Failed {
                summary,
                detail: error,
            }
        }
    }
}

pub(super) fn start(id: &str, mut execution: Execution) -> Result<StreamHandle, JsValue> {
    let mut handle = StreamHandle {
        source: EventSource::new(&format!("/api/exec/stream?task_id={id}"))?,
        listeners: Vec::new(),
    };
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
            .unwrap_or_else(|| RunState::failed("运行结果格式异常，请重试"));
        execution.has_output.set(true);
        execution.state.set(outcome);
        source.close();
    })?;
    let source = handle.source.clone();
    handle.listen("error", move |_| {
        if execution.state.peek().is_running() {
            execution.state.set(RunState::failed("连接异常，请重试"));
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
                execution.state.set(RunState::Running(task.stage));
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
                        .set(RunState::failed("运行结果缺失，请重试"));
                }
                return;
            }
            Err(error) => {
                execution
                    .state
                    .set(RunState::failed(format!("结果获取异常：{error}")));
                return;
            }
        }
    }
    execution.state.set(RunState::failed("轮询超时，请重试"));
}
