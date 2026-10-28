//! 文章列表查询接口。
//!
//! 提供已发布文章分页、管理员全量列表、以及按标签筛选三种查询能力，
//! 均通过缓存层减少重复数据库访问。
//! Leptos server function，注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中查询数据库。

use crate::api::error::ApiError;
use leptos::prelude::*;

#[cfg(feature = "server")]
use super::helpers::{get_current_admin_user, row_to_post_list_item};
use super::types::{AdminPostQuery, PostListResponse};
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;

/// 单页允许的最大文章数。
///
/// 公开的 `list_published_posts` 接口无需认证，若不对 `per_page` 设上限，
/// 攻击者可传入巨大值迫使数据库扫描并实例化超大 Vec，造成内存放大与拒绝服务。
#[cfg(feature = "server")]
const MAX_PER_PAGE: i32 = 50;

/// 允许的最大页码。
///
/// `page` 无上限时，攻击者可用海量不同 `page` 值撑大缓存键空间（缓存污染），
/// 并触发无意义的超大 `OFFSET` 扫描。10_000 对任何实际博客都足够宽裕
/// （配合 `MAX_PER_PAGE` 最多覆盖 50 万篇文章），同时把缓存键空间限制在有限范围。
#[cfg(feature = "server")]
const MAX_PAGE: i32 = 10_000;

/// 将分页参数钳制到安全范围：页码 1–`MAX_PAGE`，每页 1–`MAX_PER_PAGE`。
///
/// 注意：返回值必须同时用于缓存键与 SQL 查询，避免同一逻辑页落入不同缓存条目。
#[cfg(feature = "server")]
fn clamp_pagination(page: i32, per_page: i32) -> (i32, i32) {
    (page.clamp(1, MAX_PAGE), per_page.clamp(1, MAX_PER_PAGE))
}

/// 获取已发布文章分页列表。
///
/// 优先命中缓存；未命中时查询总数与分页记录，并按 published_at 降序排列。
#[server(ListPublishedPosts, endpoint = "list_published_posts", input = leptos::server_fn::codec::Json)]
pub async fn list_published_posts(page: i32, per_page: i32) -> Result<PostListResponse, ApiError> {
    // 钳制分页参数，防止无认证调用方请求超大每页数量导致内存放大 / DoS。
    let (page, per_page) = clamp_pagination(page, per_page);

    #[cfg(feature = "server")]
    {
        let cache_key = crate::cache::CacheKey::PublishedPosts { page, per_page };
        if let Some((cached_posts, cached_total)) = crate::cache::get_post_list(&cache_key).await {
            return Ok(PostListResponse {
                posts: cached_posts,
                total: cached_total,
            });
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;

        // 优先读取缓存中的已发布文章总数，否则查询数据库并回填缓存。
        let total = if let Some(cached_total) = crate::cache::get_total_published_posts().await {
            cached_total
        } else {
            let count_row = client
                .query_one(
                    "SELECT COUNT(*) FROM posts WHERE status = 'published' AND deleted_at IS NULL",
                    &[],
                )
                .await
                .map_err(AppError::query)?;
            let total: i64 = count_row.get(0);
            crate::cache::set_total_published_posts(total).await;
            total
        };

        let offset = ((page - 1).max(0) as i64) * (per_page as i64);
        let limit = per_page as i64;
        let rows = client
            .query(
                "SELECT
                    p.id, p.author_id, p.title, p.slug, p.summary, p.status,
                    p.published_at, p.created_at, p.updated_at, p.cover_image,
                    p.word_count, p.reading_time,
                    COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as tags
                 FROM posts p
                 LEFT JOIN post_tags pt ON p.id = pt.post_id
                 LEFT JOIN tags t ON pt.tag_id = t.id
                 WHERE p.status = 'published' AND p.deleted_at IS NULL
                 GROUP BY p.id
                 ORDER BY p.published_at DESC
                 LIMIT $1 OFFSET $2",
                &[&limit, &offset],
            )
            .await
            .map_err(AppError::query)?;

        let posts: Vec<_> = rows.iter().map(row_to_post_list_item).collect();

        crate::cache::set_post_list(&cache_key, posts.clone(), total).await;
        Ok(PostListResponse { posts, total })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(PostListResponse {
            posts: Vec::new(),
            total: 0,
        })
    }
}

/// 获取后台文章列表（含草稿，不含回收站），按创建时间倒序。
///
/// 需要 admin 权限；可按标题子串、发布状态与标签筛选，不走缓存。
#[server(ListPosts, endpoint = "list_posts", input = leptos::server_fn::codec::Json)]
pub async fn list_posts(
    page: i32,
    per_page: i32,
    query: AdminPostQuery,
) -> Result<PostListResponse, ApiError> {
    // 与公开接口保持一致的分页钳制，避免单次请求拉取过多记录。
    let (page, per_page) = clamp_pagination(page, per_page);
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        // 标题搜索词：trim 后为空视为不搜索；限长 200 字符并转义 LIKE 通配符
        // （% / _ / \），避免用户输入导致模式错配或全表误匹配。
        let title_pattern: Option<String> = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
                let truncated: String = s.chars().take(200).collect();
                format!(
                    "%{}%",
                    crate::utils::server::escape_like_pattern(&truncated)
                )
            });
        let status: Option<&str> = query.status.as_ref().map(|s| s.as_str());
        let tag: Option<&str> = query.tag.as_deref().filter(|t| !t.is_empty());

        // 条件统一写成「参数为空则不筛选」，参数位置固定，免去拼接 SQL。
        // posts 表对管理后台属低频访问、行数有限，ILIKE 全表扫可接受，靠 LIMIT 兜底。
        const FILTER: &str = "p.deleted_at IS NULL
            AND ($1::text IS NULL OR p.title ILIKE $1 ESCAPE '\\')
            AND ($2::text IS NULL OR p.status = $2)
            AND ($3::text IS NULL OR EXISTS (
                SELECT 1 FROM post_tags fpt JOIN tags ft ON ft.id = fpt.tag_id
                WHERE fpt.post_id = p.id AND ft.name = $3))";

        let total: i64 = client
            .query_one(
                &format!("SELECT COUNT(*) FROM posts p WHERE {FILTER}"),
                &[&title_pattern, &status, &tag],
            )
            .await
            .map_err(AppError::query)?
            .get(0);

        let offset = ((page - 1).max(0) as i64) * (per_page as i64);
        let limit = per_page as i64;
        let rows = client
            .query(
                &format!(
                    "SELECT
                        p.id, p.author_id, p.title, p.slug, p.summary, p.status,
                        p.published_at, p.created_at, p.updated_at, p.cover_image,
                        p.word_count, p.reading_time,
                        COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{{}}') as tags
                     FROM posts p
                     LEFT JOIN post_tags pt ON p.id = pt.post_id
                     LEFT JOIN tags t ON pt.tag_id = t.id
                     WHERE {FILTER}
                     GROUP BY p.id
                     ORDER BY p.created_at DESC
                     LIMIT $4 OFFSET $5"
                ),
                &[&title_pattern, &status, &tag, &limit, &offset],
            )
            .await
            .map_err(AppError::query)?;

        let posts: Vec<_> = rows.iter().map(row_to_post_list_item).collect();

        Ok(PostListResponse { posts, total })
    }

    #[cfg(not(feature = "server"))]
    {
        let _ = query;
        Ok(PostListResponse {
            posts: Vec::new(),
            total: 0,
        })
    }
}

/// 获取回收站中已软删除的文章列表。
///
/// 需要 admin 权限；按删除时间降序，不走缓存。
#[server(ListDeletedPosts, endpoint = "list_deleted_posts", input = leptos::server_fn::codec::Json)]
pub async fn list_deleted_posts(page: i32, per_page: i32) -> Result<PostListResponse, ApiError> {
    // 与 list_posts 一致的分页钳制。
    let (page, per_page) = clamp_pagination(page, per_page);
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        let count_row = client
            .query_one(
                "SELECT COUNT(*) FROM posts WHERE deleted_at IS NOT NULL",
                &[],
            )
            .await
            .map_err(AppError::query)?;
        let total: i64 = count_row.get(0);

        let offset = ((page - 1).max(0) as i64) * (per_page as i64);
        let limit = per_page as i64;
        let rows = client
            .query(
                "SELECT
                    p.id, p.author_id, p.title, p.slug, p.summary, p.status,
                    p.published_at, p.created_at, p.updated_at, p.cover_image, p.deleted_at,
                    p.word_count, p.reading_time,
                    COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as tags
                 FROM posts p
                 LEFT JOIN post_tags pt ON p.id = pt.post_id
                 LEFT JOIN tags t ON pt.tag_id = t.id
                 WHERE p.deleted_at IS NOT NULL
                 GROUP BY p.id
                 ORDER BY p.deleted_at DESC
                 LIMIT $1 OFFSET $2",
                &[&limit, &offset],
            )
            .await
            .map_err(AppError::query)?;

        let posts: Vec<_> = rows.iter().map(row_to_post_list_item).collect();

        Ok(PostListResponse { posts, total })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(PostListResponse {
            posts: Vec::new(),
            total: 0,
        })
    }
}

/// 获取指定标签下的全部已发布文章（上限 200），用于无分页 UI 的标签详情页。
/// 结果缓存于按标签的键空间。
#[server(GetPostsByTag, endpoint = "get_posts_by_tag", input = leptos::server_fn::codec::Json)]
pub async fn get_posts_by_tag(tag_name: String) -> Result<PostListResponse, ApiError> {
    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        if let Some((cached_posts, cached_total)) = crate::cache::get_posts_by_tag(&tag_name).await
        {
            return Ok(PostListResponse {
                posts: cached_posts,
                total: cached_total,
            });
        }

        // 真实总数（即使被 LIMIT 截断也返回完整计数）。
        let total: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM posts p
                 JOIN post_tags pt ON p.id = pt.post_id
                 JOIN tags t ON pt.tag_id = t.id
                 WHERE t.name = $1 AND p.status = 'published' AND p.deleted_at IS NULL",
                &[&tag_name],
            )
            .await
            .map_err(AppError::query)?
            .get(0);

        let rows = client
            .query(
                "SELECT
                    p.id, p.author_id, p.title, p.slug, p.summary, p.status,
                    p.published_at, p.created_at, p.updated_at, p.cover_image,
                    p.word_count, p.reading_time,
                    COALESCE(array_agg(t2.name) FILTER (WHERE t2.name IS NOT NULL), '{}') as tags
                 FROM posts p
                 JOIN post_tags pt ON p.id = pt.post_id
                 JOIN tags t ON pt.tag_id = t.id
                 LEFT JOIN post_tags pt2 ON p.id = pt2.post_id
                 LEFT JOIN tags t2 ON pt2.tag_id = t2.id
                 WHERE t.name = $1 AND p.status = 'published' AND p.deleted_at IS NULL
                 GROUP BY p.id
                 ORDER BY p.published_at DESC
                 LIMIT 200",
                &[&tag_name],
            )
            .await
            .map_err(AppError::query)?;

        let posts: Vec<_> = rows.iter().map(row_to_post_list_item).collect();

        // total 为真实 COUNT(*)，不再用 posts.len()。
        crate::cache::set_posts_by_tag(&tag_name, posts.clone(), total).await;
        Ok(PostListResponse { posts, total })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(PostListResponse {
            posts: Vec::new(),
            total: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_pagination_keeps_valid_values() {
        assert_eq!(clamp_pagination(1, 10), (1, 10));
        assert_eq!(clamp_pagination(3, 20), (3, 20));
    }

    #[test]
    fn clamp_pagination_clamps_oversized_per_page() {
        // 攻击者传入超大 per_page 必须被压回上限，避免内存放大 / DoS。
        assert_eq!(clamp_pagination(1, 1_000_000_000), (1, MAX_PER_PAGE));
        assert_eq!(clamp_pagination(2, 51), (2, MAX_PER_PAGE));
    }

    #[test]
    fn clamp_pagination_clamps_non_positive() {
        assert_eq!(clamp_pagination(0, 10), (1, 10));
        assert_eq!(clamp_pagination(-5, 10), (1, 10));
        assert_eq!(clamp_pagination(1, 0), (1, 1));
        assert_eq!(clamp_pagination(1, -100), (1, 1));
    }

    #[test]
    fn clamp_pagination_clamps_oversized_page() {
        // 巨大 page 必须被压回上限，避免无界 OFFSET 扫描与缓存键扇出。
        assert_eq!(clamp_pagination(i32::MAX, 10), (MAX_PAGE, 10));
        assert_eq!(clamp_pagination(MAX_PAGE + 1, 10), (MAX_PAGE, 10));
    }

    #[test]
    fn clamp_pagination_max_page_boundary() {
        assert_eq!(clamp_pagination(MAX_PAGE, 10), (MAX_PAGE, 10));
        assert_eq!(clamp_pagination(MAX_PAGE - 1, 10), (MAX_PAGE - 1, 10));
    }

    #[test]
    fn clamp_pagination_max_per_page_boundary() {
        assert_eq!(clamp_pagination(1, MAX_PER_PAGE), (1, MAX_PER_PAGE));
        assert_eq!(clamp_pagination(1, MAX_PER_PAGE - 1), (1, MAX_PER_PAGE - 1));
    }
}
