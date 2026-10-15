use super::{state::RunState, RunnerExample};
use crate::api::code_runner::ExecRequest;
use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub(super) struct Execution {
    pub state: Signal<RunState>,
    pub show_output: Signal<bool>,
    pub has_output: Signal<bool>,
    #[cfg(target_arch = "wasm32")]
    pub terminal: Signal<Option<crate::bridges::xterm::TerminalHandle>>,
    #[cfg(target_arch = "wasm32")]
    pending: Signal<Option<ExecRequest>>,
    #[cfg(target_arch = "wasm32")]
    stream: Signal<Option<super::stream::StreamHandle>>,
}

impl Execution {
    pub fn show_example(mut self, example: &RunnerExample) {
        self.show_output.set(true);
        self.has_output.set(true);
        self.state.set(if example.error.is_empty() {
            RunState::Succeeded(example.status.clone())
        } else {
            RunState::Failed {
                summary: example.status.clone(),
                detail: example.error.clone(),
            }
        });
        #[cfg(target_arch = "wasm32")]
        if let Some(handle) = self.terminal.read().as_ref() {
            handle
                .instance()
                .write_all(&example.stdout, &example.stderr);
        }
    }
    pub fn clear(mut self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(handle) = self.terminal.read().as_ref() {
            handle.instance().clear();
        }
        self.state.set(RunState::Idle);
        self.has_output.set(false);
    }
    pub fn start(mut self, request: ExecRequest) {
        if self.state.peek().is_running() {
            return;
        }
        self.clear();
        self.state.set(RunState::Running("准备中...".into()));
        self.show_output.set(true);
        #[cfg(target_arch = "wasm32")]
        {
            self.stream.set(None);
            self.pending.set(Some(request));
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = request;
    }
}

pub(super) fn use_execution(example: Option<RunnerExample>) -> Execution {
    let state = use_signal(|| match example.as_ref() {
        Some(example) if example.error.is_empty() => RunState::Succeeded(example.status.clone()),
        Some(example) => RunState::Failed {
            summary: example.status.clone(),
            detail: example.error.clone(),
        },
        None => RunState::Idle,
    });
    let execution = Execution {
        state,
        show_output: use_signal(|| example.is_some()),
        has_output: use_signal(|| example.is_some()),
        #[cfg(target_arch = "wasm32")]
        terminal: use_signal(|| None),
        #[cfg(target_arch = "wasm32")]
        pending: use_signal(|| None),
        #[cfg(target_arch = "wasm32")]
        stream: use_signal(|| None),
    };
    #[cfg(target_arch = "wasm32")]
    {
        let mut execution = execution;
        use_drop(move || execution.stream.set(None));
        use_effect(move || {
            if !execution.state.read().is_running() {
                execution.stream.set(None);
            }
        });
        use_effect(move || {
            if execution.terminal.read().is_none() || execution.pending.read().is_none() {
                return;
            }
            let Some(request) = execution.pending.write().take() else {
                return;
            };
            execution.state.set(RunState::Running("提交中...".into()));
            // Dioxus cancels this task on unmount. The stream handle separately owns callbacks.
            spawn(async move {
                match crate::api::code_runner::execute::start_exec_stream(request).await {
                    Ok(id) => {
                        execution.state.set(RunState::Running("运行中".into()));
                        match super::stream::start(&id, execution) {
                            Ok(stream) => execution.stream.set(Some(stream)),
                            Err(_) => super::stream::poll_result(&id, execution).await,
                        }
                    }
                    Err(error) => execution.state.set(RunState::failed(error)),
                }
            });
        });
    }
    execution
}
