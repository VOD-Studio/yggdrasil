//! 运行日志查看器接口。
//!
//! - 本文件：5 个 Dioxus server function（查询 / 导出 / target 列表 / 设置读写），
//!   双目标编译，DB 逻辑 gate 在 `#[cfg(feature = "server")]` 块内。
//! - [`capture`]：进程内 tracing Layer（日志捕获进 mpsc/broadcast），server-only。
//! - [`sse`]：`GET /api/logs/stream` 实时流 axum handler，server-only。
//!
//! 数据源：logs 表（迁移 025），由 [`crate::tasks::log_writer`] 批量写入、
//! [`crate::tasks::log_purge`] 按保留策略裁剪。

// 与 settings 模块一致：Dioxus `#[server]` 宏触发 deprecated/unit 提示，按项目惯例放行。
#![allow(clippy::unused_unit, deprecated)]

use dioxus::prelude::*;

#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;
// LogEntry 仅在 server 端的行映射里构造，wasm 端无人消费。
#[cfg(feature = "server")]
use crate::models::log::LogEntry;
use crate::models::log::{LogSettings, LogsPage};

#[cfg(feature = "server")]
pub mod capture;
#[cfg(feature = "server")]
pub mod sse;

/// 日志级别白名单（大写）。levels 空 vec = 全部级别。
#[cfg(feature = "server")]
pub(crate) const VALID_LEVELS: [&str; 5] = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

/// `get_logs` 单页条数上限。
#[cfg(feature = "server")]
const MAX_PAGE_LIMIT: i32 = 500;

/// `export_logs` 总行数上限。
#[cfg(feature = "server")]
const EXPORT_MAX_ROWS: i64 = 10_000;

/// 规范化后的日志筛选条件（levels 已白名单校验，target/query 已 trim）。
#[cfg(feature = "server")]
struct LogFilter {
    levels: Vec<String>,
    target: Option<String>,
    query: Option<String>,
}

#[cfg(feature = "server")]
impl LogFilter {
    /// 规范化 + 白名单校验：级别 trim 后大写，不在白名单内返回 BadRequest。
    fn new(
        levels: Vec<String>,
        target: Option<String>,
        query: Option<String>,
    ) -> Result<Self, AppError> {
        let levels: Vec<String> = levels
            .iter()
            .map(|l| l.trim().to_uppercase())
            .filter(|l| !l.is_empty())
            .collect();
        for l in &levels {
            if !VALID_LEVELS.contains(&l.as_str()) {
                return Err(AppError::BadRequest(format!("非法日志级别: {l}")));
            }
        }
        Ok(Self {
            levels,
            target: target
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty()),
            query: query
                .map(|q| q.trim().to_string())
                .filter(|q| !q.is_empty()),
        })
    }

    /// 生成 WHERE 片段与参数列表（参数按出现顺序编号，$1 起；全部参数化，无拼接值）。
    /// 返回 (conditions, params)，调用方追加自己的条件后 join 成 WHERE。
    fn conditions(&self) -> (Vec<String>, Vec<&(dyn tokio_postgres::types::ToSql + Sync)>) {
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::new();
        if !self.levels.is_empty() {
            params.push(&self.levels);
            conditions.push(format!("level = ANY(${})", params.len()));
        }
        if let Some(t) = &self.target {
            params.push(t);
            conditions.push(format!("target = ${}", params.len()));
        }
        if let Some(q) = &self.query {
            params.push(q);
            conditions.push(format!("message ILIKE '%' || ${} || '%'", params.len()));
        }
        (conditions, params)
    }
}

/// 把 conditions 拼成 WHERE 子句（无条件时为空串）。
#[cfg(feature = "server")]
fn where_clause(conditions: &[String]) -> String {
    if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    }
}

/// 分页查询运行日志（id DESC 游标分页）。
///
/// - `levels`：级别白名单过滤，空 vec = 全部；
/// - `target`：精确匹配 tracing target；
/// - `query`：message 子串匹配（参数化 ILIKE）；
/// - `before_id`：游标（`WHERE id < before_id`），None = 从最新开始；
/// - `limit`：clamp 到 1..=500。
#[server(GetLogs, "/api")]
pub async fn get_logs(
    levels: Vec<String>,
    target: Option<String>,
    query: Option<String>,
    before_id: Option<i64>,
    limit: i32,
) -> Result<LogsPage, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let filter = LogFilter::new(levels, target, query)?;
        let limit = limit.clamp(1, MAX_PAGE_LIMIT) as i64;
        let client = get_conn().await.map_err(AppError::db_conn)?;

        let (mut conditions, mut params) = filter.conditions();
        if let Some(before) = &before_id {
            params.push(before);
            conditions.push(format!("id < ${}", params.len()));
        }
        params.push(&limit);
        let limit_idx = params.len();

        let rows = client
            .query(
                &format!(
                    "SELECT id, ts, level, target, message FROM logs {} \
                     ORDER BY id DESC LIMIT ${limit_idx}",
                    where_clause(&conditions)
                ),
                &params,
            )
            .await
            .map_err(AppError::query)?;

        let entries: Vec<LogEntry> = rows
            .iter()
            .map(|r| LogEntry {
                id: r.get(0),
                ts: r.get(1),
                level: r.get(2),
                target: r.get(3),
                message: r.get(4),
            })
            .collect();

        // 满页才给游标：最后一行的 id 即下一页的 before_id。
        let next_cursor = if entries.len() as i64 == limit {
            entries.last().map(|e| e.id)
        } else {
            None
        };

        Ok(LogsPage {
            entries,
            next_cursor,
            dropped: capture::dropped_count(),
        })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(LogsPage {
            entries: Vec::new(),
            next_cursor: None,
            dropped: 0,
        })
    }
}

/// 导出运行日志为纯文本（每行 `[RFC3339] LEVEL target: message`）。
///
/// 与 `get_logs` 同一筛选语义，按 id ASC（时间正序），上限 10000 行。
#[server(ExportLogs, "/api")]
pub async fn export_logs(
    levels: Vec<String>,
    target: Option<String>,
    query: Option<String>,
) -> Result<String, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        use std::fmt::Write as _;

        let filter = LogFilter::new(levels, target, query)?;
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let (conditions, params) = filter.conditions();

        let rows = client
            .query(
                &format!(
                    "SELECT id, ts, level, target, message FROM logs {} \
                     ORDER BY id ASC LIMIT {EXPORT_MAX_ROWS}",
                    where_clause(&conditions)
                ),
                &params,
            )
            .await
            .map_err(AppError::query)?;

        let mut out = String::new();
        for r in &rows {
            let ts: chrono::DateTime<chrono::Utc> = r.get(1);
            let level: String = r.get(2);
            let target: String = r.get(3);
            let message: String = r.get(4);
            // 写 String 不会失败，忽略 fmt::Result。
            let _ = writeln!(
                out,
                "[{}] {} {}: {}",
                ts.to_rfc3339(),
                level,
                target,
                message
            );
        }
        Ok(out)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(String::new())
    }
}

/// 日志 target 去重列表（前端筛选项）。
///
/// 走 moka 缓存（60s TTL）：DISTINCT 全表扫在日志表上不值得每开页面跑一次。
#[server(GetLogTargets, "/api")]
pub async fn get_log_targets() -> Result<Vec<String>, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        if let Some(cached) = crate::cache::get_log_targets().await {
            return Ok(cached);
        }
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let rows = client
            .query("SELECT DISTINCT target FROM logs ORDER BY target", &[])
            .await
            .map_err(AppError::query)?;
        let targets: Vec<String> = rows.iter().map(|r| r.get(0)).collect();
        crate::cache::set_log_targets(targets.clone()).await;
        Ok(targets)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(Vec::new())
    }
}

/// 读取日志查看器配置。
///
/// settings 表缺失键时回退到默认值，保证向后兼容（trash 模式，不做 env 播种）。
#[server(GetLogSettings, "/api")]
pub async fn get_log_settings() -> Result<LogSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        let retention_days: i32 = client
            .query_opt(
                "SELECT value FROM settings WHERE key = 'logs_retention_days'",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .and_then(|r| r.get::<_, String>("value").parse().ok())
            .unwrap_or(crate::models::log::DEFAULT_LOGS_RETENTION_DAYS);

        let max_rows: i32 = client
            .query_opt(
                "SELECT value FROM settings WHERE key = 'logs_max_rows'",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .and_then(|r| r.get::<_, String>("value").parse().ok())
            .unwrap_or(crate::models::log::DEFAULT_LOGS_MAX_ROWS);

        Ok(LogSettings {
            retention_days: LogSettings::clamp_retention(retention_days),
            max_rows: LogSettings::clamp_max_rows(max_rows),
        })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(LogSettings::default())
    }
}

/// 更新日志查看器配置。
///
/// 两个值都会被 clamp 到合法范围后写入。
#[server(UpdateLogSettings, "/api")]
pub async fn update_log_settings(
    retention_days: i32,
    max_rows: i32,
) -> Result<LogSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let retention_days = LogSettings::clamp_retention(retention_days);
    let max_rows = LogSettings::clamp_max_rows(max_rows);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        client
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('logs_retention_days', $1, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                &[&retention_days.to_string()],
            )
            .await
            .map_err(AppError::query)?;

        client
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('logs_max_rows', $1, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                &[&max_rows.to_string()],
            )
            .await
            .map_err(AppError::query)?;

        // target 在 capture 排除名单内：此日志只到控制台，不进查看器管道。
        tracing::info!(
            "Log settings updated: retention_days={}, max_rows={}",
            retention_days,
            max_rows
        );
    }

    Ok(LogSettings {
        retention_days,
        max_rows,
    })
}
