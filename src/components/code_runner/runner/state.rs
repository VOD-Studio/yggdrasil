#[cfg(any(target_arch = "wasm32", test))]
use crate::api::code_runner::{ExecResult, ExecStatus};

/// Execution state has one outcome; a previous run's error cannot survive a new run.
#[derive(Clone, PartialEq)]
pub(super) enum RunState {
    Idle,
    Running(String),
    Succeeded(String),
    Failed { summary: String, detail: String },
}
impl RunState {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running(_))
    }
    pub fn stage(&self) -> &str {
        if let Self::Running(stage) = self {
            stage
        } else {
            ""
        }
    }
    pub fn summary(&self) -> &str {
        match self {
            Self::Succeeded(text) | Self::Failed { summary: text, .. } => text,
            _ => "",
        }
    }
    pub fn error(&self) -> &str {
        if let Self::Failed { detail, .. } = self {
            detail
        } else {
            ""
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub fn failed(message: impl ToString) -> Self {
        Self::Failed {
            summary: String::new(),
            detail: message.to_string(),
        }
    }
    #[cfg(any(target_arch = "wasm32", test))]
    pub fn result(result: &ExecResult) -> Self {
        let label = match result.status {
            ExecStatus::Success => "成功",
            ExecStatus::Timeout => "超时",
            ExecStatus::OomKilled => "内存超限",
            ExecStatus::Error => "运行错误",
            ExecStatus::Failed => "系统失败",
            ExecStatus::RateLimited => "请求过频",
            ExecStatus::Queued => "排队中",
            ExecStatus::Running => "运行中",
        };
        let summary = format!("耗时: {}ms · 状态: {label}", result.duration_ms);
        if result.status == ExecStatus::Success {
            Self::Succeeded(summary)
        } else {
            Self::Failed {
                summary,
                detail: label.into(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_results_never_retain_a_running_state_or_stale_error() {
        for status in [
            ExecStatus::Success,
            ExecStatus::Error,
            ExecStatus::Timeout,
            ExecStatus::OomKilled,
        ] {
            let result = ExecResult {
                status: status.clone(),
                stdout: String::new(),
                stderr: String::new(),
                exit_code: None,
                duration_ms: 12,
                language: "python".into(),
            };
            let state = RunState::result(&result);
            assert!(!state.is_running());
            assert!(state.summary().contains("12ms"));
            assert_eq!(state.error().is_empty(), status == ExecStatus::Success);
        }
    }
}
