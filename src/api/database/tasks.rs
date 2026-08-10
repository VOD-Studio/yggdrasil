#![allow(clippy::unused_unit, deprecated)]

//! 备份/恢复的异步任务进度表（DashMap）。
//!
//! create_backup/restore_backup 立即返回 task_id，后台任务跑时通过
//! `update` 更新进度，前端轮询 [`get_task_progress`][crate::api::database::tasks::get_task_progress]。
//! 已完成超过 1 小时的任务惰性清理，避免内存累积。

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

// admin 鉴权仅 server 构建用到。
#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
// DashMap / LazyLock 仅 server 构建持有任务进度表；WASM 端只序列化 TaskProgress。
#[cfg(feature = "server")]
use dashmap::DashMap;
#[cfg(feature = "server")]
use std::sync::LazyLock;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum TaskKind {
    Backup,
    Restore,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum TaskStatus {
    Running,
    Done,
    Failed,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TaskProgress {
    pub id: String,
    pub kind: TaskKind,
    pub stage: String,
    pub percent: u8,
    pub detail: Option<String>,
    pub status: TaskStatus,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    /// 完成后的备份文件名（下载/恢复用）。
    pub result_filename: Option<String>,
}

#[cfg(feature = "server")]
static TASKS: LazyLock<DashMap<String, TaskProgress>> = LazyLock::new(DashMap::new);

/// 注册新任务（初始 Running，0%）。
///
/// pub(crate)：定时备份调度任务（[`crate::tasks::backup`]）也需要注册，
/// 让 admin 打开面板时能看到进行中的自动备份。
#[cfg(feature = "server")]
pub(crate) fn insert(id: String, kind: TaskKind) {
    TASKS.insert(
        id.clone(),
        TaskProgress {
            id,
            kind,
            stage: "排队中".to_string(),
            percent: 0,
            detail: None,
            status: TaskStatus::Running,
            error: None,
            created_at: Utc::now(),
            result_filename: None,
        },
    );
}

/// 更新任务进度（后台任务调用）。
#[cfg(feature = "server")]
pub(super) fn update(
    id: &str,
    stage: &str,
    percent: u8,
    status: TaskStatus,
    detail: Option<String>,
    error: Option<String>,
    result_filename: Option<String>,
) {
    if let Some(mut p) = TASKS.get_mut(id) {
        p.stage = stage.to_string();
        p.percent = percent;
        p.status = status;
        p.detail = detail;
        p.error = error;
        p.result_filename = result_filename;
    }
}

/// 惰性清理已完成超过 1 小时的任务（查询时顺便清）。
#[cfg(feature = "server")]
fn gc_old() {
    let cutoff = Utc::now() - chrono::Duration::hours(1);
    TASKS.retain(|_, p| {
        !(matches!(p.status, TaskStatus::Done | TaskStatus::Failed) && p.created_at < cutoff)
    });
}

/// 查询任务进度（轮询用）。
#[server(GetTaskProgress, "/api")]
pub async fn get_task_progress(task_id: String) -> Result<TaskProgress, ServerFnError> {
    let _user = get_current_admin_user().await?;
    #[cfg(feature = "server")]
    {
        gc_old();
        TASKS
            .get(&task_id)
            .map(|p| p.clone())
            .ok_or_else(|| crate::api::error::AppError::NotFound("任务不存在").into())
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(TaskProgress {
            id: task_id,
            kind: TaskKind::Backup,
            stage: String::new(),
            percent: 0,
            detail: None,
            status: TaskStatus::Done,
            error: None,
            created_at: Utc::now(),
            result_filename: None,
        })
    }
}
