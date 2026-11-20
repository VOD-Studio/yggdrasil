//! 文章全文搜索接口。
//!
//! 通过 `ILIKE` 对 search_text 做子串模糊匹配（两侧 `%`），由 posts.search_text 上的
//! trigram GIN 索引（gin_trgm_ops，见 migrations/019）加速；<3 字符的查询 trigram 不足，
//! 可能退回顺序扫，靠 LIMIT 50 与搜索限流兜底。结果按 pg_trgm 的 `word_similarity`
//! 相似度与发布时间降序返回最多 50 篇已发布文章。
//! Leptos server function，注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中查询数据库。

use crate::api::error::ApiError;
use leptos::prelude::*;

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
#[server(SearchPosts, endpoint = "search_posts", input = leptos::server_fn::codec::Json)]
pub async fn search_posts(query: String) -> Result<PostListResponse, ApiError> {
    use crate::api::rate_limit;

    // 对搜索接口进行严格限流，防止滥用 expensive 查询。
    if let Some(headers) = crate::request_ctx::request_headers() {
        let ip = rate_limit::get_client_ip(&headers).await;
        if let Err(_msg) = rate_limit::check_strict_limit(&ip) {
            return Ok(PostListResponse {
                posts: Vec::new(),
                total: 0,
            });
        }
    }

    let q = query.trim();
    if q.is_empty() || q.chars().count() > 200 {
        return Ok(PostListResponse {
            posts: Vec::new(),
            total: 0,
        });
    }

    // 先检查短 TTL 的搜索结果缓存。
    if let Some((posts, total)) = cache::get_search_results(q).await {
        return Ok(PostListResponse { posts, total });
    }

    let client = get_conn().await.map_err(AppError::db_conn)?;

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
                ARRAY(SELECT t.name FROM post_tags pt JOIN tags t ON pt.tag_id = t.id
                      WHERE pt.post_id = p.id ORDER BY pt.tag_id) AS tags
             FROM (
                 SELECT id, author_id, title, slug, summary, status, published_at, created_at,
                        updated_at, cover_image, word_count, reading_time,
                        word_similarity(search_text, $2) AS sml
                 FROM posts
                 WHERE status = 'published' AND deleted_at IS NULL
                   AND search_text ILIKE '%' || $1 || '%' ESCAPE '\\'
                 ORDER BY sml DESC, published_at DESC LIMIT 50
             ) p
             ORDER BY p.sml DESC, p.published_at DESC",
            &[&escaped, &q],
        )
        .await
        .map_err(AppError::query)?;

    let posts: Vec<_> = rows.iter().map(row_to_post_list_item).collect();

    let total = posts.len() as i64;
    cache::set_search_results(q, posts.clone(), total).await;
    Ok(PostListResponse { posts, total })
}
