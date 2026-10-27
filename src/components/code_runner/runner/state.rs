//! 运行状态：进行中三个阶段，结束后一种结果。每种状态对应标题栏的「圆点 + 文字」、
//! 色调、输出区的说明，以及是否在输出区给出重试按钮。
#[cfg(any(target_arch = "wasm32", test))]
use crate::api::code_runner::{ExecResult, ExecStatus};
#[cfg(any(target_arch = "wasm32", test))]
use crate::api::error::ApiError;

/// 进行中的阶段。排队与运行只在浏览器端由流事件构造。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Phase {
    /// 并发槽位已满，等待。
    Queued,
    /// 提交中或正在拉起容器。
    Preparing,
    /// 容器已启动。
    Running,
}

/// 一次运行的结果。上一次的结果在新一次开始时清空。真实结果只在浏览器端构造。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Outcome {
    Success {
        duration_ms: u64,
    },
    RunError {
        exit_code: Option<i64>,
        duration_ms: u64,
    },
    Timeout {
        duration_ms: u64,
    },
    OutOfMemory {
        duration_ms: u64,
    },
    /// 服务端限流消息（每分钟或每天的上限）。
    RateLimited(String),
    /// 运行环境的问题，附服务端给出的可读消息。
    SystemFailed(String),
    /// 输出流断开，结果可能不完整。
    Disconnected,
    /// 图鉴播放作者保存的输出。
    Example {
        meta: String,
        error: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum RunState {
    Idle,
    Busy(Phase),
    Finished(Outcome),
}

/// 状态色调：成功与进行中用中性色，资源受限用琥珀色，出错用红色。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Tone {
    Neutral,
    Warn,
    Danger,
}

impl Tone {
    pub fn as_str(self) -> &'static str {
        match self {
            Tone::Neutral => "neutral",
            Tone::Warn => "warn",
            Tone::Danger => "danger",
        }
    }
}

impl RunState {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Busy(_))
    }

    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Finished(_))
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "未运行",
            Self::Busy(Phase::Queued) => "排队中",
            Self::Busy(Phase::Preparing) => "准备中",
            Self::Busy(Phase::Running) => "运行中",
            Self::Finished(outcome) => match outcome {
                Outcome::Success { .. } => "成功",
                Outcome::RunError { .. } => "运行错误",
                Outcome::Timeout { .. } => "超时",
                Outcome::OutOfMemory { .. } => "内存超限",
                Outcome::RateLimited(_) => "请求过频",
                Outcome::SystemFailed(_) => "系统失败",
                Outcome::Disconnected => "连接中断",
                Outcome::Example { .. } => "示例",
            },
        }
    }

    pub fn tone(&self) -> Tone {
        match self {
            Self::Finished(Outcome::Timeout { .. } | Outcome::OutOfMemory { .. })
            | Self::Finished(Outcome::RateLimited(_)) => Tone::Warn,
            Self::Finished(
                Outcome::RunError { .. } | Outcome::SystemFailed(_) | Outcome::Disconnected,
            ) => Tone::Danger,
            Self::Finished(Outcome::Example { error, .. }) if !error.is_empty() => Tone::Danger,
            _ => Tone::Neutral,
        }
    }

    /// 输出区标题栏右侧的耗时与退出码。
    pub fn meta(&self) -> String {
        match self {
            Self::Finished(Outcome::Success { duration_ms }) => {
                format!("退出码 0 · {}", seconds(*duration_ms))
            }
            Self::Finished(Outcome::RunError {
                exit_code,
                duration_ms,
            }) => match exit_code {
                Some(code) => format!("退出码 {code} · {}", seconds(*duration_ms)),
                None => seconds(*duration_ms),
            },
            Self::Finished(
                Outcome::Timeout { duration_ms } | Outcome::OutOfMemory { duration_ms },
            ) => seconds(*duration_ms),
            Self::Finished(Outcome::Example { meta, .. }) => meta.clone(),
            _ => String::new(),
        }
    }

    /// 输出区底部的说明；没有时不渲染。
    pub fn note(&self) -> Option<String> {
        let text = match self {
            Self::Busy(Phase::Queued) => "并发已满，空出槽位后自动开始",
            Self::Busy(Phase::Preparing) => "正在启动容器…",
            Self::Finished(Outcome::Timeout { .. }) => "超过时间上限被终止；输出到终止前为止",
            Self::Finished(Outcome::OutOfMemory { .. }) => "内存超过上限，进程被系统终止",
            Self::Finished(Outcome::RateLimited(message)) => return Some(message.clone()),
            Self::Finished(Outcome::SystemFailed(message)) if !message.is_empty() => {
                return Some(message.clone())
            }
            Self::Finished(Outcome::SystemFailed(_)) => "运行环境出了问题，不是代码的错",
            Self::Finished(Outcome::Disconnected) => "输出流断开，结果可能不完整",
            Self::Finished(Outcome::Example { error, .. }) if !error.is_empty() => {
                return Some(error.clone())
            }
            _ => return None,
        };
        Some(text.to_string())
    }

    /// 与代码无关、再试一次可能成功的结果。
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Finished(
                Outcome::RateLimited(_) | Outcome::SystemFailed(_) | Outcome::Disconnected
            )
        )
    }

    /// 提交被服务端拒绝：限流消息单独归类，其余视为系统失败。
    #[cfg(any(target_arch = "wasm32", test))]
    pub fn rejected(error: &ApiError) -> Self {
        let message = crate::utils::error::error_reason(error)
            .unwrap_or_else(|| "运行服务暂时不可用，请稍后再试".to_string());
        if message.contains("频繁") || message.contains("次数已达上限") {
            Self::Finished(Outcome::RateLimited(message))
        } else {
            Self::Finished(Outcome::SystemFailed(message))
        }
    }

    /// 轮询兜底路径的终态结果。
    #[cfg(any(target_arch = "wasm32", test))]
    pub fn result(result: &ExecResult) -> Self {
        let duration_ms = result.duration_ms;
        Self::Finished(match result.status {
            ExecStatus::Success => Outcome::Success { duration_ms },
            ExecStatus::Timeout => Outcome::Timeout { duration_ms },
            ExecStatus::OomKilled => Outcome::OutOfMemory { duration_ms },
            ExecStatus::Error => Outcome::RunError {
                exit_code: result.exit_code,
                duration_ms,
            },
            ExecStatus::RateLimited => Outcome::RateLimited("运行太频繁，稍后再试".into()),
            ExecStatus::Failed | ExecStatus::Queued | ExecStatus::Running => {
                Outcome::SystemFailed(result.stderr.trim().to_string())
            }
        })
    }
}

/// 流式路径 `done` 事件的载荷。
#[cfg(any(target_arch = "wasm32", test))]
#[derive(serde::Deserialize)]
pub(super) struct DonePayload {
    pub exit_code: Option<i64>,
    pub oom_killed: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
    #[serde(default)]
    pub error: Option<String>,
}

#[cfg(any(target_arch = "wasm32", test))]
impl DonePayload {
    /// 超时与内存超限优先于系统错误：容器被终止时服务端也会附带错误消息。
    pub fn outcome(self) -> RunState {
        let duration_ms = self.duration_ms;
        RunState::Finished(if self.timed_out {
            Outcome::Timeout { duration_ms }
        } else if self.oom_killed {
            Outcome::OutOfMemory { duration_ms }
        } else if let Some(error) = self.error {
            Outcome::SystemFailed(error)
        } else if self.exit_code == Some(0) {
            Outcome::Success { duration_ms }
        } else {
            Outcome::RunError {
                exit_code: self.exit_code,
                duration_ms,
            }
        })
    }
}

/// `0.04 s`、`0.8 s`、`12 s`：秒数随量级减少小数位。
fn seconds(ms: u64) -> String {
    let s = ms as f64 / 1000.0;
    if s < 1.0 {
        format!("{s:.2} s")
    } else if s < 10.0 {
        format!("{s:.1} s")
    } else {
        format!("{s:.0} s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(exit_code: Option<i64>, oom: bool, timed_out: bool, error: Option<&str>) -> RunState {
        DonePayload {
            exit_code,
            oom_killed: oom,
            timed_out,
            duration_ms: 40,
            error: error.map(str::to_string),
        }
        .outcome()
    }

    #[test]
    fn done_payload_maps_to_each_outcome() {
        assert_eq!(done(Some(0), false, false, None).label(), "成功");
        assert_eq!(done(Some(1), false, false, None).label(), "运行错误");
        assert_eq!(done(None, false, true, Some("执行超时")).label(), "超时");
        assert_eq!(done(Some(137), true, false, None).label(), "内存超限");
        assert_eq!(
            done(None, false, false, Some("镜像未构建")).label(),
            "系统失败"
        );
    }

    #[test]
    fn tones_follow_severity() {
        assert_eq!(done(Some(0), false, false, None).tone(), Tone::Neutral);
        assert_eq!(done(None, false, true, None).tone(), Tone::Warn);
        assert_eq!(done(Some(1), false, false, None).tone(), Tone::Danger);
        assert_eq!(RunState::Busy(Phase::Running).tone(), Tone::Neutral);
    }

    #[test]
    fn meta_shows_exit_code_and_duration() {
        assert_eq!(
            done(Some(0), false, false, None).meta(),
            "退出码 0 · 0.04 s"
        );
        assert_eq!(
            done(Some(1), false, false, None).meta(),
            "退出码 1 · 0.04 s"
        );
        assert_eq!(seconds(800), "0.80 s");
        assert_eq!(seconds(2_340), "2.3 s");
        assert_eq!(seconds(15_000), "15 s");
    }

    #[test]
    fn rejected_submission_separates_rate_limits() {
        let limited = RunState::rejected(&ApiError::BadRequest("请求过于频繁，请稍后再试".into()));
        assert_eq!(limited.label(), "请求过频");
        assert_eq!(limited.note().as_deref(), Some("请求过于频繁，请稍后再试"));
        assert!(limited.retryable());
        let oversized = RunState::rejected(&ApiError::BadRequest("源代码过大".into()));
        assert_eq!(oversized.label(), "系统失败");
        assert_eq!(oversized.note().as_deref(), Some("源代码过大"));
    }

    #[test]
    fn only_environment_failures_offer_retry() {
        assert!(RunState::Finished(Outcome::Disconnected).retryable());
        assert!(!done(Some(1), false, false, None).retryable());
        assert!(!done(None, false, true, None).retryable());
    }

    #[test]
    fn poll_results_never_stay_running() {
        for status in [
            ExecStatus::Success,
            ExecStatus::Error,
            ExecStatus::Timeout,
            ExecStatus::OomKilled,
            ExecStatus::Failed,
        ] {
            let result = ExecResult {
                status,
                stdout: String::new(),
                stderr: String::new(),
                exit_code: None,
                duration_ms: 12,
                language: "python".into(),
            };
            let state = RunState::result(&result);
            assert!(state.is_finished());
        }
    }
}
