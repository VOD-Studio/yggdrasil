//! 文章统计接口。
//!
//! 返回文章总数、草稿数、已发布数与回收站（软删除）数量，供管理后台仪表盘与
//! 文章列表页使用，结果缓存。Dioxus server function，注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中查询数据库。

use dioxus::prelude::*;

#[cfg(feature = "server")]
use super::helpers::get_current_admin_user;
use super::types::PostStatsResponse;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;
#[cfg(feature = "server")]
use crate::models::post::PostStats;

/// 获取文章统计信息。
///
/// 需要 admin 权限；优先命中缓存，未命中时通过单次条件聚合查询同时统计
/// 未删除文章总数、草稿数、已发布数与回收站（软删除）数量。
#[server(GetPostStats, "/api")]
pub async fn get_post_stats() -> Result<PostStatsResponse, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        if let Some(cached) = crate::cache::get_post_stats().await {
            return Ok(PostStatsResponse { stats: cached });
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;

        // 通过单次条件聚合查询同时统计总数、草稿数、已发布数与回收站数量。
        let row = client
            .query_one(
                "SELECT
                    COUNT(*) FILTER (WHERE deleted_at IS NULL) AS total,
                    COUNT(*) FILTER (WHERE deleted_at IS NULL AND status = 'draft') AS drafts,
                    COUNT(*) FILTER (WHERE deleted_at IS NULL AND status = 'published') AS published,
                    COUNT(*) FILTER (WHERE deleted_at IS NOT NULL) AS trash,
                    COUNT(*) FILTER (WHERE deleted_at IS NULL AND created_at > now() - interval '30 days') AS recent_30d
                 FROM posts",
                &[],
            )
            .await
            .map_err(AppError::query)?;

        let stats = PostStats {
            total: row.get("total"),
            drafts: row.get("drafts"),
            published: row.get("published"),
            trash: row.get("trash"),
            recent_30d: row.get("recent_30d"),
        };
        crate::cache::set_post_stats(stats.clone()).await;
        Ok(PostStatsResponse { stats })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(PostStatsResponse {
            stats: PostStats {
                total: 0,
                drafts: 0,
                published: 0,
                trash: 0,
                recent_30d: 0,
            },
        })
    }
}
