//! 文章全文搜索接口。
//!
//! 通过 `ILIKE` 对 search_text 做子串模糊匹配（两侧 `%`），由 posts.search_text 上的
//! trigram GIN 索引（gin_trgm_ops，见 migrations/019）加速；<3 字符的查询 trigram 不足，
//! 可能退回顺序扫，靠 LIMIT 50 与搜索限流兜底。结果按 pg_trgm 的 `word_similarity`
//! 相似度与发布时间降序返回最多 50 篇已发布文章。
//! Dioxus server function，注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中查询数据库。

use dioxus::prelude::*;

#[cfg(feature = "server")]
use super::helpers::row_to_post_list_item;
use super::types::PostListResponse;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::cache;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;

/// 搜索已发布文章。
///
/// 空查询直接返回空结果；非空查询使用 `word_similarity` 计算相关度，
/// 并限制返回 50 条记录。结果写入短 TTL 内存缓存以减轻 DB 压力。
#[server(SearchPosts, "/api")]
pub async fn search_posts(query: String) -> Result<PostListResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::rate_limit;

        // 对搜索接口进行严格限流，防止滥用 expensive 查询。
        if let Some(ctx) = dioxus::fullstack::FullstackContext::current() {
            let headers = ctx.parts_mut().headers.clone();
            let ip = rate_limit::get_client_ip(&headers).await;
            if let Err(_msg) = rate_limit::check_strict_limit(&ip) {
                return Ok(PostListResponse {
                    posts: Vec::new(),
                    total: 0,
                });
            }
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;

        let q = query.trim();
        if q.is_empty() || q.chars().count() > 200 {
            return Ok(PostListResponse {
                posts: Vec::new(),
                total: 0,
            });
        }

        // 先检查短 TTL 的搜索结果缓存。
        let cache_key = cache::normalize_search_key(q);
        if let Some((posts, total)) = cache::get_search_results(&cache_key).await {
            return Ok(PostListResponse { posts, total });
        }

        // 转义 SQL LIKE 通配符，避免用户输入 % / _ 导致全表扫描。
        let escaped = crate::utils::server::escape_like_pattern(q);

        // 使用 ILIKE 做子串模糊匹配（双侧 %），走 posts.search_text 上的 trigram GIN 索引
        // （见 migrations/019）。<3 字符查询 trigram 不足时可能退回顺序扫，由 LIMIT 50 + 限流兜底。
        let rows = client
            .query(
                "SELECT
                    p.id, p.author_id, p.title, p.slug, p.summary, p.status,
                    p.published_at, p.created_at, p.updated_at, p.cover_image,
                    p.word_count, p.reading_time,
                    COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as tags,
                    word_similarity(p.search_text, $2) AS sml
                 FROM posts p
                 LEFT JOIN post_tags pt ON p.id = pt.post_id
                 LEFT JOIN tags t ON pt.tag_id = t.id
                 WHERE p.status = 'published' AND p.deleted_at IS NULL
                   AND p.search_text ILIKE '%' || $1 || '%' ESCAPE '\\'
                 GROUP BY p.id, p.search_text
                 ORDER BY sml DESC, p.published_at DESC
                 LIMIT 50",
                &[&escaped, &q],
            )
            .await
            .map_err(AppError::query)?;

        let posts: Vec<_> = rows.iter().map(row_to_post_list_item).collect();

        let total = posts.len() as i64;
        cache::set_search_results(&cache_key, posts.clone(), total).await;
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
