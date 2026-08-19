//! 孤儿素材定期清理后台任务。
//!
//! 仅在 `server` feature 启用时编译，每天运行一次。
//! 每次执行前读取 settings 表：若自动清理关闭则跳过，否则物理删除
//! 「无文章引用（asset_refs）且无存活评论引用、超过保留天数」的素材
//! （文件 + DB 行 + 派生缓存），语义与 /admin/assets 的「一键清理孤儿」
//! 一致，只是改为定时自动。
//!
//! 评论区允许匿名传图后，未提交/被删评论留下的孤儿图会持续累积，
//! 此任务是 uploads/ 磁盘的主要回收手段。

use std::time::Duration;

use tokio::time::interval;

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
        match get_conn().await {
            Ok(client) => match purge_orphans(&client).await {
                Ok((n, bytes)) => {
                    if n > 0 {
                        tracing::info!(
                            "Orphan asset purge: removed {} assets, freed {} bytes",
                            n,
                            bytes
                        );
                    }
                }
                Err(e) => tracing::error!("Orphan asset purge error: {:?}", e),
            },
            Err(e) => tracing::error!("Failed to get DB connection for orphan asset purge: {:?}", e),
        }
        ticker.tick().await;
    }
}

/// 读取配置并删除过期孤儿素材，返回（删除行数, 释放字节数）。
///
/// 逐项删文件（容忍单项失败：NotFound 静默，其他错误仅告警，DB 行照删——
/// 残留文件由重建索引的反向语义兜底），最后批量删 DB 行并失效派生缓存。
async fn purge_orphans(client: &tokio_postgres::Client) -> Result<(u64, i64), tokio_postgres::Error> {
    // 读取配置，缺键时回退默认值（默认启用、7 天）。
    let enabled: bool = client
        .query_opt(
            "SELECT value FROM settings WHERE key = 'asset_orphan_purge_enabled'",
            &[],
        )
        .await?
        .and_then(|r| r.get::<_, String>("value").parse().ok())
        .unwrap_or(crate::models::settings::DEFAULT_ASSET_ORPHAN_PURGE_ENABLED);

    if !enabled {
        return Ok((0, 0));
    }

    let days: i32 = client
        .query_opt(
            "SELECT value FROM settings WHERE key = 'asset_orphan_retention_days'",
            &[],
        )
        .await?
        .and_then(|r| r.get::<_, String>("value").parse().ok())
        .unwrap_or(crate::models::settings::DEFAULT_ASSET_ORPHAN_RETENTION_DAYS);

    let days = AssetPurgeSettings::clamp_retention(days);

    // 孤儿 = 无文章引用（asset_refs）且无存活评论引用（见 COMMENT_REF_CLAUSE）。
    let rows = client
        .query(
            &format!(
                "SELECT a.id AS id, a.path, a.size_bytes FROM assets a \
                 WHERE NOT EXISTS (SELECT 1 FROM asset_refs r WHERE r.asset_id = a.id) \
                   AND NOT {comment_ref} \
                   AND a.created_at < NOW() - make_interval(days => $1)",
                comment_ref = crate::api::assets::COMMENT_REF_CLAUSE
            ),
            &[&days],
        )
        .await?;

    if rows.is_empty() {
        return Ok((0, 0));
    }

    let mut ids: Vec<uuid::Uuid> = Vec::with_capacity(rows.len());
    let mut freed_bytes: i64 = 0;
    for row in &rows {
        let id: uuid::Uuid = row.get("id");
        let path: String = row.get("path");
        freed_bytes += row.get::<_, i64>("size_bytes");
        let file_path = format!("uploads/{}", path);
        if let Err(e) = tokio::fs::remove_file(&file_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("Orphan purge: remove file failed ({}): {}", file_path, e);
            }
        }
        crate::api::image::invalidate_asset_caches(&path).await;
        ids.push(id);
    }

    let n = client
        .execute("DELETE FROM assets WHERE id = ANY($1)", &[&ids])
        .await?;
    Ok((n, freed_bytes))
}
