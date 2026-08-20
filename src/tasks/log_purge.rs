//! 运行日志保留策略后台任务（server-only）。
//!
//! 每小时执行一次（含启动时立即执行），读取 settings 表两键
//! （缺键回退默认值，trash 模式）：
//! - `logs_retention_days`（默认 7）：删除 `ts < now() - 保留天数` 的日志；
//! - `logs_max_rows`（默认 100000）：按 id 从新到旧保留前 N 行，其余删除。
//!
//! 任何错误只记日志，不中断循环（与 post_purge 同一模式）。

use std::time::Duration;

use tokio::time::interval;

use crate::db::pool::get_conn;
use crate::models::log::{LogSettings, DEFAULT_LOGS_MAX_ROWS, DEFAULT_LOGS_RETENTION_DAYS};

/// 裁剪间隔：每小时。
const PURGE_INTERVAL: Duration = Duration::from_secs(3600);

/// 启动日志保留策略循环（serve() 内 spawn 一次）。
pub async fn run_purge() {
    let mut ticker = interval(PURGE_INTERVAL);
    loop {
        match get_conn().await {
            Ok(client) => {
                if let Err(e) = purge_once(&client).await {
                    tracing::error!("Log purge error: {:?}", e);
                }
            }
            Err(e) => tracing::error!("Failed to get DB connection for log purge: {:?}", e),
        }
        ticker.tick().await;
    }
}

/// 读取配置并执行一次龄期 + 行数裁剪。
async fn purge_once(client: &tokio_postgres::Client) -> Result<(), tokio_postgres::Error> {
    // 读取配置，缺键/非法值时回退默认值。
    let retention_days: i32 = client
        .query_opt(
            "SELECT value FROM settings WHERE key = 'logs_retention_days'",
            &[],
        )
        .await?
        .and_then(|r| r.get::<_, String>("value").parse().ok())
        .unwrap_or(DEFAULT_LOGS_RETENTION_DAYS);

    let max_rows: i32 = client
        .query_opt(
            "SELECT value FROM settings WHERE key = 'logs_max_rows'",
            &[],
        )
        .await?
        .and_then(|r| r.get::<_, String>("value").parse().ok())
        .unwrap_or(DEFAULT_LOGS_MAX_ROWS);

    let retention_days = LogSettings::clamp_retention(retention_days);
    let max_rows = LogSettings::clamp_max_rows(max_rows) as i64;

    // 1) 按龄期删除。
    let aged = client
        .execute(
            "DELETE FROM logs WHERE ts < now() - make_interval(days => $1)",
            &[&retention_days],
        )
        .await?;

    // 2) 按行数裁剪：子查询找到「第 max_rows+1 新」的 id，
    //    删除所有 id <= 它的行。表不足 max_rows 行时子查询返回 NULL，
    //    `id <= NULL` 恒不为真，一行不删。
    let trimmed = client
        .execute(
            "DELETE FROM logs WHERE id <= (\
                 SELECT id FROM logs ORDER BY id DESC OFFSET $1 LIMIT 1\
             )",
            &[&max_rows],
        )
        .await?;

    if aged + trimmed > 0 {
        tracing::info!(
            aged,
            trimmed,
            retention_days,
            max_rows,
            "Log purge: removed expired/excess log rows"
        );
    }
    Ok(())
}
