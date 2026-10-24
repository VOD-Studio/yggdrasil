//! 一次运行的共享状态与「命令」信号。
//!
//! 外壳（调用方所在包）持有状态信号并发出命令；激活后加载的分包函数
//! `code_runner_body` 负责消费命令：启动流、写终端。JS 句柄只存在于分包内，
//! 外壳与分包之间只经由这些 `Send + Sync` 的信号通信。

use super::{
    state::{Outcome, Phase, RunState},
    RunnerExample,
};
use crate::api::code_runner::ExecRequest;
use leptos::prelude::*;

/// 外壳发给终端的命令；终端未挂载时直接丢弃（挂载时按 `example_output` 自行写入）。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq)]
pub(super) enum TerminalCommand {
    Clear,
    WriteAll { stdout: String, stderr: String },
}

#[derive(Clone, Copy)]
pub(super) struct Execution {
    pub state: RwSignal<RunState>,
    pub show_output: RwSignal<bool>,
    pub has_output: RwSignal<bool>,
    /// 待提交的运行请求，由分包里的执行 Effect 取走（终端就绪后才提交）。
    pub pending: RwSignal<Option<ExecRequest>>,
    /// 待执行的终端命令，由分包里的终端 Effect 取走。
    pub terminal_command: RwSignal<Option<TerminalCommand>>,
}

impl Execution {
    pub fn new(example: Option<&RunnerExample>) -> Self {
        Self {
            state: RwSignal::new(example.map_or(RunState::Idle, example_state)),
            show_output: RwSignal::new(example.is_some()),
            has_output: RwSignal::new(example.is_some()),
            pending: RwSignal::new(None),
            terminal_command: RwSignal::new(None),
        }
    }

    pub fn show_example(self, example: &RunnerExample) {
        self.show_output.set(true);
        self.has_output.set(true);
        self.state.set(example_state(example));
        self.terminal_command.set(Some(TerminalCommand::WriteAll {
            stdout: example.stdout.clone(),
            stderr: example.stderr.clone(),
        }));
    }

    pub fn clear(self) {
        self.terminal_command.set(Some(TerminalCommand::Clear));
        self.state.set(RunState::Idle);
        self.has_output.set(false);
    }

    pub fn start(self, request: ExecRequest) {
        if self.state.get_untracked().is_running() {
            return;
        }
        self.clear();
        self.state.set(RunState::Busy(Phase::Preparing));
        self.show_output.set(true);
        self.pending.set(Some(request));
    }

    /// 当前是否在运行（非响应式；组件已卸载时视为未运行）。
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn is_running_untracked(self) -> bool {
        self.state
            .try_with_untracked(RunState::is_running)
            .unwrap_or(false)
    }
}

fn example_state(example: &RunnerExample) -> RunState {
    RunState::Finished(Outcome::Example {
        meta: example.status.clone(),
        error: example.error.clone(),
    })
}
