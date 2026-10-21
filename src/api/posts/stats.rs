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
use crate::models::post::{MonthCount, PostStats};

/// 获取文章统计信息。
///
/// 需要 admin 权限；优先命中缓存，未命中时通过单次条件聚合查询同时统计
/// 未删除文章总数、草稿数、已发布数与回收站（软删除）数量。
#[server(GetPostStats, "/api")]
pub async fn get_post_stats() -> Result<PostStatsResponse, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let stats = load_post_stats().await?;
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
                monthly_published: Vec::new(),
            },
        })
    }
}

/// 读取文章统计：优先命中缓存，未命中时查询并写回。侧栏数量与仪表盘共用。
/// 调用方须先完成管理员鉴权。
#[cfg(feature = "server")]
pub(crate) async fn load_post_stats() -> Result<PostStats, AppError> {
    if let Some(cached) = crate::cache::get_post_stats().await {
        return Ok(cached);
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
        monthly_published: Vec::new(),
    };

    // 近 12 个月每月发布篇数：generate_series 补齐没有文章的月（0），
    // 恰好 12 行、按月升序。
    let month_rows = client
        .query(
            "SELECT EXTRACT(YEAR FROM gs.m)::int AS year, EXTRACT(MONTH FROM gs.m)::int AS month,
                    COUNT(p.id) AS cnt
             FROM generate_series(
                    date_trunc('month', now() AT TIME ZONE 'UTC') - interval '11 months',
                    date_trunc('month', now() AT TIME ZONE 'UTC'),
                    interval '1 month'
                  ) AS gs(m)
             LEFT JOIN posts p
               ON p.status = 'published'
              AND p.deleted_at IS NULL
              AND date_trunc('month', p.published_at AT TIME ZONE 'UTC') = gs.m
             GROUP BY gs.m
             ORDER BY gs.m",
            &[],
        )
        .await
        .map_err(AppError::query)?;
    let monthly_published = month_rows
        .iter()
        .map(|r| MonthCount {
            year: r.get("year"),
            month: r.get::<_, i32>("month") as u32,
            count: r.get("cnt"),
        })
        .collect();

    let stats = PostStats {
        monthly_published,
        ..stats
    };
    crate::cache::set_post_stats(stats.clone()).await;
    Ok(stats)
}
