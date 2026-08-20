//! 运行日志查看器的共享 DTO（server / wasm 双目标）。
//!
//! `LogEntry` 同时用于历史查询（`get_logs` 分页）与 SSE 实时事件
//! （`/api/logs/stream` 的 `log` 事件负载；实时事件尚未落库，`id` 恒为 0）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 默认日志保留天数（天）。
pub const DEFAULT_LOGS_RETENTION_DAYS: i32 = 7;
/// 默认日志最大行数：超出后按 id 从新到旧裁剪。
pub const DEFAULT_LOGS_MAX_ROWS: i32 = 100_000;
/// 保留天数下限（天）。
#[cfg(feature = "server")]
pub const MIN_LOGS_RETENTION_DAYS: i32 = 1;
/// 保留天数上限（天）。防止误填超大值导致永不清理。
#[cfg(feature = "server")]
pub const MAX_LOGS_RETENTION_DAYS: i32 = 90;
/// 最大行数下限。防止误填过小值把日志表裁空。
#[cfg(feature = "server")]
pub const MIN_LOGS_MAX_ROWS: i32 = 1_000;
/// 最大行数上限。防止误填超大值导致表无限增长。
#[cfg(feature = "server")]
pub const MAX_LOGS_MAX_ROWS: i32 = 1_000_000;

/// 单条日志记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// 数据库主键（按 id 游标分页）。SSE 实时事件尚未落库，恒为 0。
    pub id: i64,
    /// 事件捕获时刻（UTC）。
    pub ts: DateTime<Utc>,
    /// 级别大写：ERROR / WARN / INFO / DEBUG / TRACE。
    pub level: String,
    /// tracing target（模块路径）。
    pub target: String,
    /// 消息文本（含追加的结构化字段，截断至 4KB）。
    pub message: String,
}

/// `get_logs` 的一页结果（按 id DESC 游标分页）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogsPage {
    /// 本页条目（id 降序，最新在前）。
    pub entries: Vec<LogEntry>,
    /// 下一页游标（本页最后一条的 id）；None 表示没有更多。
    pub next_cursor: Option<i64>,
    /// 进程启动以来因管道满 / 写库失败被丢弃的日志条数。
    pub dropped: u64,
}

/// 日志查看器配置（settings 表 `logs_retention_days` / `logs_max_rows` 键）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogSettings {
    /// 日志保留天数，超过后被后台任务删除。
    pub retention_days: i32,
    /// 日志表最大行数，超出后从新到旧裁剪。
    pub max_rows: i32,
}

impl Default for LogSettings {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_LOGS_RETENTION_DAYS,
            max_rows: DEFAULT_LOGS_MAX_ROWS,
        }
    }
}

impl LogSettings {
    /// 将保留天数钳制到合法范围 [MIN, MAX]。
    #[cfg(feature = "server")]
    pub fn clamp_retention(days: i32) -> i32 {
        days.clamp(MIN_LOGS_RETENTION_DAYS, MAX_LOGS_RETENTION_DAYS)
    }

    /// 将最大行数钳制到合法范围 [MIN, MAX]。
    #[cfg(feature = "server")]
    pub fn clamp_max_rows(rows: i32) -> i32 {
        rows.clamp(MIN_LOGS_MAX_ROWS, MAX_LOGS_MAX_ROWS)
    }
}
