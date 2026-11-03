//! 孤儿素材定期清理后台任务。
//!
//! 仅在 `server` feature 启用时编译，每天运行一次。
//! 每次执行前读取 settings 表：若自动清理关闭则跳过，否则物理删除
//! 「无文章、笔记版本、存活评论或头像引用，且超过保留天数」的素材
//! （文件 + DB 行 + 派生缓存），语义与 /admin/assets 的「一键清理孤儿」
//! 一致，只是改为定时自动。
//!
//! 评论区允许匿名传图后，未提交/被删评论留下的孤儿图会持续累积，
//! 此任务是 uploads/ 磁盘的主要回收手段。

use std::time::Duration;

use tokio::time::interval;

use crate::api::assets::delete::purge_orphan_assets_impl;
use crate::api::assets::types::PurgeOrphansResponse;
use crate::api::error::{ApiError, AppError};
use crate::db::pool::get_conn;
use crate::models::settings::AssetPurgeSettings;

/// 启动孤儿素材清理循环，每天触发一次。
///
/// 每次读取最新配置：若 `asset_orphan_purge_enabled` 关闭则 no-op，
/// 否则删除创建时间早于 `asset_orphan_retention_days` 的孤儿素材。
/// 任何错误只记录日志，不中断循环。
pub async fn run_purge() {
    let mut ticker = interval(Duration::from_secs(86400));
    loop {
        match purge_orphans().await {
            Ok(Some(res)) if res.deleted_count > 0 || res.failures > 0 => tracing::info!(
                "Orphan asset purge: removed {} assets, freed {} bytes, {} file failures",
                res.deleted_count,
                res.freed_bytes,
                res.failures
            ),
            Ok(_) => {}
            Err(e) => tracing::error!("Orphan asset purge error: {}", e),
        }
        ticker.tick().await;
    }
}

/// 读取配置并删除过期孤儿素材；自动清理关闭时返回 `None`。
///
/// 删除复用管理端 [`purge_orphan_assets_impl`] 的流程：事务内锁候选行、
/// 删除语句复核引用、提交后再删文件，避免并发保存刚建立的引用被级联删除。
///
/// [`purge_orphan_assets_impl`]: crate::api::assets::delete::purge_orphan_assets_impl
async fn purge_orphans() -> Result<Option<PurgeOrphansResponse>, ApiError> {
    let client = get_conn().await.map_err(AppError::db_conn)?;
    let setting = |key: &'static str| {
        let client = &client;
        async move {
            client
                .query_opt("SELECT value FROM settings WHERE key = $1", &[&key])
                .await
                .map(|row| row.map(|r| r.get::<_, String>("value")))
                .map_err(AppError::query)
        }
    };

    // 缺键时回退默认值（默认启用、7 天）。
    let enabled = setting("asset_orphan_purge_enabled")
        .await?
        .and_then(|v| v.parse().ok())
        .unwrap_or(crate::models::settings::DEFAULT_ASSET_ORPHAN_PURGE_ENABLED);
    if !enabled {
        return Ok(None);
    }
    let days = setting("asset_orphan_retention_days")
        .await?
        .and_then(|v| v.parse().ok())
        .unwrap_or(crate::models::settings::DEFAULT_ASSET_ORPHAN_RETENTION_DAYS);
    // 先归还读配置的连接，清理流程自行取连接开事务。
    drop(client);

    purge_orphan_assets_impl(AssetPurgeSettings::clamp_retention(days))
        .await
        .map(Some)
}
