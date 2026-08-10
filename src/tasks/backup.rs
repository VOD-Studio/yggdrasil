//! 自动定时备份后台任务。
//!
//! 仅在 `server` feature 启用时编译。每天定点（UTC，见 settings 表 `backup_time_utc`）
//! 执行一次应用内备份（pg_dump + uploads 打包 + 自动轮转，见
//! [`crate::api::database::backup::run_auto_backup`]），执行结果落库
//! `backup_last_run_*` 键供面板展示。
//!
//! 调度语义：
//! - 每次循环重读 DB 配置；关闭时挂起等待，不跑空转 tick。
//! - 面板保存设置后经 [`notify_settings_changed`] 立即唤醒重排，
//!   无需等原定的下次触发。
//! - 任何错误只记录日志，不中断循环（与 post_purge 等任务一致）。

use std::sync::LazyLock;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Notify;

use crate::api::database::backup::{self, BackupRunOutcome};
use crate::api::settings::{load_backup_settings, save_last_backup_run};
use crate::db::pool::get_conn;
use crate::models::settings::LastBackupRun;

/// 设置变更通知。`Notify` 单 permit 语义足够：多次保存折叠为一次重排。
static SETTINGS_CHANGED: LazyLock<Notify> = LazyLock::new(Notify::new);

/// 唤醒调度器立即重读设置并重排（`update_backup_settings` 调用）。
pub(crate) fn notify_settings_changed() {
    SETTINGS_CHANGED.notify_waiters();
}

/// 启动自动备份调度循环。
pub async fn run_scheduler() {
    // 关闭态/读配置失败时的兜底自醒间隔（防丢通知导致永久挂起）。
    const IDLE_RETRY: Duration = Duration::from_secs(3600);

    loop {
        let settings = match get_conn().await {
            Ok(conn) => match load_backup_settings(&conn).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Auto-backup: failed to load settings: {e:?}");
                    tokio::select! {
                        _ = SETTINGS_CHANGED.notified() => {}
                        _ = tokio::time::sleep(IDLE_RETRY) => {}
                    }
                    continue;
                }
            },
            Err(e) => {
                tracing::error!("Auto-backup: failed to get DB connection: {e:?}");
                tokio::time::sleep(IDLE_RETRY).await;
                continue;
            }
        };

        if !settings.auto_enabled {
            tokio::select! {
                _ = SETTINGS_CHANGED.notified() => {}
                _ = tokio::time::sleep(IDLE_RETRY) => {}
            }
            continue;
        }

        let now = Utc::now();
        let Some(next) = settings.next_run_after(now) else {
            // time_utc 非法（面板/API 已规范化，此处仅是纵深防御）。
            tracing::error!(
                "Auto-backup: invalid backup_time_utc {:?}",
                settings.time_utc
            );
            tokio::select! {
                _ = SETTINGS_CHANGED.notified() => {}
                _ = tokio::time::sleep(IDLE_RETRY) => {}
            }
            continue;
        };
        let wait = (next - now).to_std().unwrap_or(Duration::ZERO);
        tracing::info!("Auto-backup: next run scheduled at {}", next.to_rfc3339());

        // 睡眠期间设置变更 → 立即重排；否则睡到触发点执行。
        tokio::select! {
            _ = SETTINGS_CHANGED.notified() => continue,
            _ = tokio::time::sleep(wait) => {}
        }

        tracing::info!("Auto-backup: starting scheduled backup");
        let outcome = backup::run_auto_backup().await;
        persist_last_run(&outcome).await;
        match &outcome {
            Ok(o) => tracing::info!(
                "Auto-backup: done (sql={}, uploads={:?}, warning={:?})",
                o.sql_filename,
                o.uploads_filename,
                o.warning
            ),
            Err(e) => tracing::error!("Auto-backup: failed: {e}"),
        }
    }
}

/// 落库最近一次自动备份结果（面板展示用）。写库失败只记日志。
async fn persist_last_run(outcome: &Result<BackupRunOutcome, String>) {
    let run = match outcome {
        Ok(o) => LastBackupRun {
            at: Utc::now().to_rfc3339(),
            ok: true,
            file: Some(o.sql_filename.clone()),
            error: o.warning.clone(),
        },
        Err(e) => LastBackupRun {
            at: Utc::now().to_rfc3339(),
            ok: false,
            file: None,
            error: Some(e.clone()),
        },
    };
    match get_conn().await {
        Ok(conn) => {
            if let Err(e) = save_last_backup_run(&conn, &run).await {
                tracing::error!("Auto-backup: failed to persist last_run: {e:?}");
            }
        }
        Err(e) => tracing::error!("Auto-backup: failed to get DB connection for last_run: {e:?}"),
    }
}
