#![allow(clippy::unused_unit, deprecated)]

//! 备份与恢复（读写，最高风险）。
//!
//! 备份：pg_dump 成功后才发布带签名的完整、角色无关的 .sql
//! （`--clean --if-exists --no-owner --no-privileges`），失败不发布文件。
//! 每次备份按设置附带 uploads 素材打包（`_uploads.tar.gz`，
//! 排除可重建的 `.cache/`），与 .sql 成对展示/下载/删除。
//!
//! 来源标记：手动备份文件名前缀 `backup_`，定时任务自动备份前缀 `auto_`；
//! 只有通过完整导出检查的自动备份才按 `backup_retention_count` 轮转。
//!
//! 恢复：仅接受本系统生成的备份（签名校验）+ 二次确认 + 路径穿越防护；
//! 旧 CSV 导出仅供下载抢救，不可直接恢复。
//! 旧备份的 pg_dump 属主语句会在 COPY 感知的预处理阶段移除，再由
//! `psql --single-transaction -v ON_ERROR_STOP=1` 原子重放，失败时完整回滚；
//! 仅恢复数据库——uploads 需从配对 tar.gz 手动还原；成功后全量失效缓存。
//! 长耗时操作走后台任务 + 进度轮询（见 [`crate::api::database::tasks`]）。
//!
//! 导入：`POST /api/database/backups/import`（纯 Axum 路由，multipart 流式落盘）
//! 把本机备份回灌进 backups/。两步式——导入只入库，恢复仍走上述管线。
//! 导入时即强制签名校验（外来 SQL 拒收不留盘）；保留原始文件名、同名冲突拒绝；
//! `.tmp` 落盘 + 原子 rename，半截文件永不出现在列表。上限 `BACKUP_IMPORT_MAX_MB`
//! （默认 512MB），路由层 DefaultBodyLimit + 流式计数双保险。
//!
//! **兼容性提示**：未使用 `--clean --if-exists` 生成的旧备份不含 DROP，
//! 恢复到已有表的数据库会在「relation already exists」处中止并回滚，
//! 需重新创建完整备份才能覆盖已有数据库。

#[cfg(feature = "server")]
mod commands;
#[cfg(feature = "server")]
mod creation;
#[cfg(feature = "server")]
mod files;
#[cfg(feature = "server")]
mod restore;
#[cfg(feature = "server")]
mod transfer;

use crate::api::error::ApiError;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::database::tasks::{self, TaskKind};
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
pub(crate) use creation::run_auto_backup;
#[cfg(feature = "server")]
use files::{backup_path, is_valid_backup_filename, read_backup_header};
#[cfg(feature = "server")]
use std::{fs::File, io::Seek};
#[cfg(feature = "server")]
pub(crate) use transfer::import_max_bytes;
#[cfg(feature = "server")]
pub use transfer::{download_backup, import_backup};

/// multipart 框架开销宽限，加在路由 body limit 上；流式计数只计算载荷。
#[cfg(feature = "server")]
pub(crate) const MULTIPART_FRAME_SLACK: u64 = 1024 * 1024;

/// 备份文件元信息（列表展示用）。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BackupInfo {
    pub filename: String,
    pub size_bytes: u64,
    /// 备份模式：pg_dump / sql-fallback（从签名头解析）。
    pub mode: String,
    pub created_at: Option<String>,
    /// 来源：manual / auto（从文件名前缀解析）。
    pub origin: String,
    /// 配对的 uploads 打包文件名与大小（无则 None——老备份或禁用了素材打包）。
    pub uploads_filename: Option<String>,
    pub uploads_size_bytes: Option<u64>,
}

/// 旧 CSV 导出仅供数据抢救；已知的模式优先于文件名。
pub(crate) fn is_salvage_only(filename: &str, mode: &str) -> bool {
    mode == "sql-fallback" || (mode == "unknown" && filename.ends_with("_sqlfallback.sql"))
}

#[cfg(test)]
mod tests {
    use super::is_salvage_only;

    #[test]
    fn salvage_classification_prefers_explicit_mode() {
        assert!(is_salvage_only("renamed.sql", "sql-fallback"));
        assert!(is_salvage_only("backup_sqlfallback.sql", "unknown"));
        assert!(!is_salvage_only("backup_sqlfallback.sql", "pg_dump"));
        assert!(!is_salvage_only("old.sql", "unknown"));
    }
}

/// 备份来源：决定文件名前缀与是否参与自动轮转。
#[cfg(feature = "server")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackupOrigin {
    /// admin 面板手动触发（`backup_` 前缀，永不自动删除）。
    Manual,
    /// 定时任务触发（`auto_` 前缀，按 retention_count 轮转）。
    Auto,
}

#[cfg(feature = "server")]
impl BackupOrigin {
    fn file_prefix(self) -> &'static str {
        match self {
            Self::Manual => "backup",
            Self::Auto => "auto",
        }
    }
}

/// 一次备份运行的结果（调度任务据此落库 last_run）。
#[cfg(feature = "server")]
#[derive(Debug)]
pub(crate) struct BackupRunOutcome {
    /// 数据库 SQL 文件名。
    pub sql_filename: String,
    /// uploads 打包文件名（禁用了素材打包时为 None）。
    pub uploads_filename: Option<String>,
    /// 非致命告警（如 DB 成功但 uploads 打包失败）。
    pub warning: Option<String>,
}

/// 发起备份，立即返回 task_id，后台任务执行。已有备份在跑时拒绝。
#[server(CreateBackup, endpoint = "create_backup", input = leptos::server_fn::codec::Json)]
pub async fn create_backup() -> Result<String, ApiError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let guard = creation::BackupGuard::try_acquire()
            .ok_or_else(|| AppError::BadRequest(creation::BACKUP_BUSY.to_string()))?;
        let task_id = uuid::Uuid::new_v4().to_string();
        tasks::insert(task_id.clone(), TaskKind::Backup);
        let tid = task_id.clone();
        tokio::spawn(async move {
            // 手动触发：进度经任务表轮询展示，返回值无人消费。
            let _ = creation::run_backup(&tid, BackupOrigin::Manual, guard).await;
        });
        Ok(task_id)
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(String::new())
    }
}

/// 发起恢复：校验签名 + 路径穿越防护 + 二次确认，立即返回 task_id。
#[server(RestoreBackup, endpoint = "restore_backup", input = leptos::server_fn::codec::Json)]
pub async fn restore_backup(filename: String, confirm: bool) -> Result<String, ApiError> {
    let _user = get_current_admin_user().await?;

    // 全部校验都在 server cfg 块内：confirm/regex/backup_path/std::fs 都是 server-only。
    // WASM 侧的 server-function 客户端桩只返回 Ok(String::new())。
    #[cfg(feature = "server")]
    {
        if !confirm {
            return Err(AppError::BadRequest("需确认恢复（会覆盖现有数据）".to_string()).into());
        }
        // 路径穿越防护
        if !is_valid_backup_filename(&filename) {
            return Err(AppError::BadRequest("无效的文件名".to_string()).into());
        }
        let mut source = File::open(backup_path(&filename)).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                AppError::NotFound("备份文件不存在")
            } else {
                AppError::Internal("无法打开备份文件")
            }
        })?;
        let (valid_signature, mode) = read_backup_header(&mut source)
            .map_err(|_| AppError::Internal("读取备份文件头失败"))?;
        if !valid_signature {
            return Err(
                AppError::BadRequest("非本系统生成的备份文件，拒绝恢复".to_string()).into(),
            );
        }
        if is_salvage_only(&filename, &mode) {
            return Err(AppError::BadRequest(
                "这是仅供数据抢救的旧导出，不能直接恢复数据库".to_string(),
            )
            .into());
        }
        source
            .rewind()
            .map_err(|_| AppError::Internal("重新定位备份文件失败"))?;

        let task_id = uuid::Uuid::new_v4().to_string();
        tasks::insert(task_id.clone(), TaskKind::Restore);
        let tid = task_id.clone();
        // 复用已查验的文件句柄，避免恢复时按文件名重新打开别的文件。
        tokio::spawn(async move {
            restore::run_restore(&tid, source).await;
        });
        Ok(task_id)
    }
    #[cfg(not(feature = "server"))]
    {
        // WASM 客户端桩：忽略参数，返回空 task_id。
        let _ = (filename, confirm);
        Ok(String::new())
    }
}

/// 列出 SQL 备份，并附带配对 uploads 归档的元信息。
#[server(ListBackups, endpoint = "list_backups", input = leptos::server_fn::codec::Json)]
pub async fn list_backups() -> Result<Vec<BackupInfo>, ApiError> {
    let _user = get_current_admin_user().await?;
    #[cfg(feature = "server")]
    {
        Ok(files::list_backup_files())
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(vec![])
    }
}

/// 管理员删除备份文件，并清理配对的 uploads 归档。
#[server(DeleteBackup, endpoint = "delete_backup", input = leptos::server_fn::codec::Json)]
pub async fn delete_backup(filename: String) -> Result<(), ApiError> {
    let _user = get_current_admin_user().await?;
    #[cfg(feature = "server")]
    {
        files::delete_backup_files(&filename).map_err(Into::into)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = filename;
        Ok(())
    }
}
