//! 代码运行接口（Code Runner）。
//!
//! 三层架构的最上层 API 模块：
//! - [`mod@code_runner`] 本文件：双目标（server / wasm）共享的可序列化数据结构。
//! - `progress`：内存任务缓冲表（DashMap + GC），server-only。
//! - `languages`：语言注册表与围栏代码块 info 解析，server-only。
//! - `execute`：Leptos server function（StartExec / GetExecResult），server-only。
//!
//! 为避免 WASM 构建引入 server 依赖，数据结构在模块顶层无 cfg 门禁定义，
//! 而引用的 `ResourceLimits` 同样位于无门禁的 [`crate::infra::runner_config`]。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::infra::runner_config::ResourceLimits;

/// 一次代码执行请求。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ExecRequest {
    /// 语言标识（与 [`crate::api::code_runner::languages::LANGUAGES`] 的 key 对应）。
    pub language: String,
    /// 源代码文本。
    pub source: String,
    /// 作者/读者可覆盖的资源限制；最终仍会被 [`crate::infra::runner_config::clamp_limits`] 钳制。
    pub overrides: Option<ResourceLimits>,
}

/// 执行状态枚举。
///
/// 既用于任务当前状态（[`ExecTask::status`]），也用于最终结果（[`ExecResult::status`]）。
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum ExecStatus {
    /// 已入队，等待并发槽位。
    Queued,
    /// 已获得槽位，容器运行中。
    Running,
    /// 正常结束且退出码为 0。
    Success,
    /// 超过 timeout_secs 被强制终止。
    Timeout,
    /// 触发内核 OOM killer。
    OomKilled,
    /// 容器执行出错（非 0 退出码）。
    Error,
    /// 系统层失败（拉起容器异常、排队超时等）。
    Failed,
    /// 触发速率限制。
    ///
    /// D8：服务端从不构造此变体（限流在 insert 前直接拒绝，返回 429）。
    /// 保留供前端 ExecStatus label 反序列化兼容；勿在服务端 match 依赖它。
    RateLimited,
}

/// 一次执行的最终结果。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ExecResult {
    pub status: ExecStatus,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i64>,
    pub duration_ms: u64,
    pub language: String,
}

/// 任务条目，供前端轮询查询执行进度。
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ExecTask {
    pub id: String,
    pub status: ExecStatus,
    /// 人类可读的当前阶段描述（如「启动容器」「执行完毕」）。
    pub stage: String,
    pub created_at: DateTime<Utc>,
    /// 终态时填充。
    pub result: Option<ExecResult>,
}

// execute.rs 含 server function，需对双目标可见（不能 cfg-gate）；其 server-only
// 依赖在文件内单独 gate。languages / progress 是纯 server 辅助，整体 gate。
pub mod execute;
#[cfg(feature = "server")]
pub mod languages;
#[cfg(feature = "server")]
pub mod progress;
#[cfg(feature = "server")]
pub mod readiness;
#[cfg(feature = "server")]
pub mod sse;
