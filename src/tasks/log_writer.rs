//! 日志批量落库后台任务（server-only）。
//!
//! 从 [`crate::api::logs::capture`] 的 mpsc 通道接收捕获的日志事件，
//! 攒批（200 条或 500ms 窗口，先到先发）后用一条 `INSERT ... UNNEST`
//! 批量写入 logs 表。
//!
//! 失败语义：
//! - INSERT 失败：记 error 日志（target 在 capture 的防递归排除名单内，
//!   不会回流进管道）、整批丢弃、按批大小累加 dropped 计数——
//!   日志查看器允许丢数据，绝不允许反过来拖垮主服务；
//! - `get_conn()` 失败：保留本批，sleep 后重试（不丢批也不重试风暴），
//!   重试期间通道继续缓冲，缓冲溢出走 capture 的 dropped 计数。

use std::time::Duration;

use crate::api::logs::capture::{self, LogRecord};
use crate::db::pool::get_conn;

/// 单批最大条数。
const BATCH_SIZE: usize = 200;
/// 攒批窗口：首条到达后最长等待这么久就冲刷（不足一批也发）。
const FLUSH_WINDOW: Duration = Duration::from_millis(500);
/// `get_conn()` 失败后的重试间隔（保留本批，不丢）。
const CONN_RETRY_INTERVAL: Duration = Duration::from_secs(2);

/// 启动日志落库循环（serve() 内 spawn 一次）。
pub async fn run_writer() {
    let mut rx = match capture::take_db_receiver() {
        Some(rx) => rx,
        None => {
            // target 在 capture 排除名单内，此日志只到控制台，不会回流。
            tracing::error!("log writer: capture receiver already taken; task exiting");
            return;
        }
    };
    tracing::info!(
        batch_size = BATCH_SIZE,
        flush_window_ms = FLUSH_WINDOW.as_millis() as u64,
        "log writer started"
    );

    let mut batch: Vec<LogRecord> = Vec::with_capacity(BATCH_SIZE);
    loop {
        // 阻塞等首条；通道关闭（进程退出）时冲刷余量后退出。
        let first = match rx.recv().await {
            Some(r) => r,
            None => {
                flush(&mut batch).await;
                return;
            }
        };
        batch.push(first);

        // 窗口内尽力拉满一批；窗口到点或通道关闭即冲刷。
        let deadline = tokio::time::Instant::now() + FLUSH_WINDOW;
        while batch.len() < BATCH_SIZE {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(r)) => batch.push(r),
                Ok(None) | Err(_) => break,
            }
        }
        flush(&mut batch).await;
    }
}

/// 冲刷一批：UNNEST 数组批量 INSERT。空批直接返回。
async fn flush(batch: &mut Vec<LogRecord>) {
    if batch.is_empty() {
        return;
    }

    // 连接失败：保留本批，sleep 后重试——启动窗口内 DB 可能尚未就绪，
    // 运行期 DB 短暂抖动也不该丢批。
    let client = loop {
        match get_conn().await {
            Ok(c) => break c,
            Err(e) => {
                tracing::error!(error = %e, "log writer: failed to get DB connection; retrying");
                tokio::time::sleep(CONN_RETRY_INTERVAL).await;
            }
        }
    };

    let ts: Vec<chrono::DateTime<chrono::Utc>> = batch.iter().map(|r| r.ts).collect();
    let levels: Vec<&str> = batch.iter().map(|r| r.level.as_str()).collect();
    let targets: Vec<&str> = batch.iter().map(|r| r.target.as_str()).collect();
    let messages: Vec<&str> = batch.iter().map(|r| r.message.as_str()).collect();

    let result = client
        .execute(
            "INSERT INTO logs (ts, level, target, message) \
             SELECT * FROM UNNEST($1::timestamptz[], $2::text[], $3::text[], $4::text[])",
            &[&ts, &levels, &targets, &messages],
        )
        .await;

    if let Err(e) = result {
        // target 在 capture 排除名单内，此 error 不会回流进管道。
        tracing::error!(
            error = %e,
            dropped = batch.len() as u64,
            "log writer: batch insert failed; dropping batch"
        );
        capture::record_dropped(batch.len() as u64);
    }
    batch.clear();
}
