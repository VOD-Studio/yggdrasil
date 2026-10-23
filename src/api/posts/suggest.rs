//! 搜索面板（⌘K）的文章建议接口。
//!
//! 与全文搜索不同：只匹配标题、摘要与标签名，最多返回 8 条，按「标题命中 >
//! 摘要命中 > 标签命中」再按发布时间排序。面板逐字查询，所以另配一档宽松限流
//! （见 `rate_limit::check_suggest_limit`），结果写入短 TTL 缓存，并随文章写操作失效。
//! Leptos server function，注册在 `/api` 路径下。

use leptos::prelude::*;

use super::types::PostSuggestionResponse;
#[cfg(feature = "server")]
use super::types::{PostSuggestion, SUGGESTION_LIMIT};
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;

/// 查询长度上限：超出时视为无效查询，直接返回空。
#[cfg(feature = "server")]
const MAX_QUERY_CHARS: usize = 100;

/// 文章建议的 SQL：`$1` 为已转义 LIKE 通配符的查询词。
#[cfg(feature = "server")]
const SUGGEST_SQL: &str = "SELECT p.slug, p.title, p.summary, p.published_at,
        COALESCE(array_agg(t.name ORDER BY t.name) FILTER (WHERE t.name IS NOT NULL), '{}') AS tags,
        CASE
            WHEN p.title ILIKE '%' || $1 || '%' ESCAPE '\\' THEN 0
            WHEN COALESCE(p.summary, '') ILIKE '%' || $1 || '%' ESCAPE '\\' THEN 1
            ELSE 2
        END AS rank
     FROM posts p
     LEFT JOIN post_tags pt ON p.id = pt.post_id
     LEFT JOIN tags t ON pt.tag_id = t.id
     WHERE p.status = 'published' AND p.deleted_at IS NULL
       AND (p.title ILIKE '%' || $1 || '%' ESCAPE '\\'
            OR COALESCE(p.summary, '') ILIKE '%' || $1 || '%' ESCAPE '\\'
            OR EXISTS (
                SELECT 1 FROM post_tags pt2 JOIN tags t2 ON t2.id = pt2.tag_id
                WHERE pt2.post_id = p.id AND t2.name ILIKE '%' || $1 || '%' ESCAPE '\\'))
     GROUP BY p.id
     ORDER BY rank, p.published_at DESC NULLS LAST
     LIMIT $2";

/// 执行建议查询（不经缓存与限流），供接口与数据库回归测试共用。
#[cfg(feature = "server")]
pub(crate) async fn query_suggestions(
    client: &deadpool_postgres::Client,
    query: &str,
) -> Result<Vec<PostSuggestion>, AppError> {
    let q = query.trim();
    if q.is_empty() || q.chars().count() > MAX_QUERY_CHARS {
        return Ok(Vec::new());
    }
    let escaped = crate::utils::server::escape_like_pattern(q);
    let rows = client
        .query(SUGGEST_SQL, &[&escaped, &SUGGESTION_LIMIT])
        .await
        .map_err(AppError::query)?;
    Ok(rows
        .iter()
        .map(|r| PostSuggestion {
            slug: r.get("slug"),
            title: r.get("title"),
            summary: r.get("summary"),
            published_at: r.get("published_at"),
            tags: r.get("tags"),
        })
        .collect())
}

/// 搜索面板的文章建议。
///
/// 空查询或超长查询返回空结果；被限流时同样返回空，让面板保留上一次结果。
#[server(SuggestPosts, endpoint = "suggest_posts", input = leptos::server_fn::codec::Json)]
pub async fn suggest_posts(query: String) -> Result<PostSuggestionResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::rate_limit;

        if let Some(headers) = crate::request_ctx::request_headers() {
            let ip = rate_limit::get_client_ip(&headers).await;
            if rate_limit::check_suggest_limit(&ip).is_err() {
                return Ok(PostSuggestionResponse { posts: Vec::new() });
            }
        }

        let q = query.trim();
        if q.is_empty() || q.chars().count() > MAX_QUERY_CHARS {
            return Ok(PostSuggestionResponse { posts: Vec::new() });
        }
        if let Some(posts) = crate::cache::get_suggestions(q).await {
            return Ok(PostSuggestionResponse { posts });
        }
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let posts = query_suggestions(&client, q).await?;
        crate::cache::set_suggestions(q, posts.clone()).await;
        Ok(PostSuggestionResponse { posts })
    }

    #[cfg(not(feature = "server"))]
    {
        let _ = query;
        Ok(PostSuggestionResponse { posts: Vec::new() })
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    /// 与文章写入回归共用一次性数据库 ygg_mcp_test。
    #[test]
    #[ignore = "requires disposable DATABASE_URL database ygg_mcp_test"]
    fn suggestions_rank_and_filter_database() {
        crate::db::TEST_DATABASE_RUNTIME.block_on(suggestions_rank_and_filter_impl());
    }

    async fn suggestions_rank_and_filter_impl() {
        let _guard = crate::db::TEST_DATABASE_LOCK.lock().await;
        let mut client = get_conn().await.unwrap();
        let database: String = client
            .query_one("SELECT current_database()", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(database, "ygg_mcp_test", "requires isolated test database");
        crate::db::migrate::run_on_conn(&mut client).await.unwrap();
        client
            .batch_execute(
                "TRUNCATE users RESTART IDENTITY CASCADE;
                 DELETE FROM tags WHERE name LIKE 'suggest-%';
                 INSERT INTO users(id,username,email,password_hash,role)
                     VALUES (1,'suggest-owner','suggest@test.invalid','unused','admin');
                 INSERT INTO posts(author_id,title,slug,summary,content_md,status,published_at) VALUES
                     (1,'从 Future 到 Waker','by-title',NULL,'正文','published','2026-01-01'),
                     (1,'执行器','by-summary','谈谈 waker 的唤醒','正文','published','2026-03-01'),
                     (1,'调度笔记','by-tag','无关摘要','正文','published','2026-05-01'),
                     (1,'Waker 草稿','draft','未发布','正文','draft',NULL),
                     (1,'完成度 100%','percent','含百分号','正文','published','2025-12-01');
                 INSERT INTO tags(name) VALUES ('suggest-waker');
                 INSERT INTO post_tags(post_id, tag_id)
                     SELECT p.id, t.id FROM posts p, tags t
                     WHERE p.slug = 'by-tag' AND t.name = 'suggest-waker';",
            )
            .await
            .unwrap();

        let slugs =
            |posts: Vec<PostSuggestion>| posts.into_iter().map(|p| p.slug).collect::<Vec<_>>();
        // 标题命中优先，其次摘要，最后标签；草稿不出现，大小写不敏感。
        let found = query_suggestions(&client, "waker").await.unwrap();
        assert_eq!(found[2].tags, vec!["suggest-waker".to_string()]);
        assert_eq!(slugs(found), ["by-title", "by-summary", "by-tag"]);
        // LIKE 通配符按字面匹配。
        assert_eq!(
            slugs(query_suggestions(&client, "%").await.unwrap()),
            ["percent"]
        );
        assert!(query_suggestions(&client, "  ").await.unwrap().is_empty());
        assert!(query_suggestions(&client, &"长".repeat(101))
            .await
            .unwrap()
            .is_empty());

        client
            .batch_execute(
                "TRUNCATE users RESTART IDENTITY CASCADE; DELETE FROM tags WHERE name LIKE 'suggest-%';",
            )
            .await
            .unwrap();
    }
}

/// 404 页「也许你在找」：从地址里拆出的词。
#[cfg_attr(not(feature = "server"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PathRecommendations {
    /// 参与匹配的词（去掉了路由前缀、数字与过短的片段）。
    pub words: Vec<String>,
    pub posts: Vec<super::types::PostSuggestion>,
}

/// 404 推荐最多列出的文章数。
#[cfg(feature = "server")]
const PATH_RECOMMEND_LIMIT: i64 = 3;

/// 地址里不代表内容的段：路由前缀与常见文件名。
#[cfg(any(feature = "server", test))]
const ROUTE_WORDS: [&str; 10] = [
    "post", "posts", "page", "tags", "tag", "notes", "book", "about", "index", "html",
];

/// 把地址拆成用于匹配的词：按 `/ - _ .` 与空白切分，解码百分号编码，
/// 去掉路由前缀、纯数字与不足两个字符的片段，最多保留 5 个、去重。
#[cfg(any(feature = "server", test))]
fn path_words(path: &str) -> Vec<String> {
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let decoded = percent_decode(path);
    let mut words: Vec<String> = Vec::new();
    for word in decoded.split(|c: char| "/-_.".contains(c) || c.is_whitespace()) {
        let word = word.trim().to_lowercase();
        if word.chars().count() < 2
            || word.chars().all(|c| c.is_ascii_digit())
            || ROUTE_WORDS.contains(&word.as_str())
            || words.contains(&word)
        {
            continue;
        }
        words.push(word);
        if words.len() == 5 {
            break;
        }
    }
    words
}

/// 解码 `%XX`；不是合法 UTF-8 时退回原文。
#[cfg(any(feature = "server", test))]
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// 按地址里的词匹配已发布文章的标题与 slug，命中词多的排前，最多 3 篇。
/// 地址里没有可用的词时不查询。公开接口，结果不缓存（只在 404 时调用）。
#[server(RecommendForPath, endpoint = "recommend_for_path", input = leptos::server_fn::codec::Json)]
pub async fn recommend_for_path(path: String) -> Result<PathRecommendations, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let words = path_words(&path);
        if words.is_empty() {
            return Ok(PathRecommendations {
                words,
                posts: Vec::new(),
            });
        }
        let patterns: Vec<String> = words
            .iter()
            .map(|w| format!("%{}%", crate::utils::server::escape_like_pattern(w)))
            .collect();
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let rows = client
            .query(
                "SELECT p.slug, p.title, p.summary, p.published_at,
                        COALESCE(array_agg(t.name ORDER BY t.name) FILTER (WHERE t.name IS NOT NULL), '{}') AS tags,
                        (SELECT COUNT(*) FROM unnest($1::text[]) w
                         WHERE p.title ILIKE w ESCAPE '\\' OR p.slug ILIKE w ESCAPE '\\') AS hits
                 FROM posts p
                 LEFT JOIN post_tags pt ON p.id = pt.post_id
                 LEFT JOIN tags t ON pt.tag_id = t.id
                 WHERE p.status = 'published' AND p.deleted_at IS NULL
                   AND (p.title ILIKE ANY($1) OR p.slug ILIKE ANY($1))
                 GROUP BY p.id
                 ORDER BY hits DESC, p.published_at DESC NULLS LAST
                 LIMIT $2",
                &[&patterns, &PATH_RECOMMEND_LIMIT],
            )
            .await
            .map_err(AppError::query)?;
        let posts = rows
            .iter()
            .map(|r| PostSuggestion {
                slug: r.get("slug"),
                title: r.get("title"),
                summary: r.get("summary"),
                published_at: r.get("published_at"),
                tags: r.get("tags"),
            })
            .collect();
        Ok(PathRecommendations { words, posts })
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = path;
        Ok(PathRecommendations {
            words: Vec::new(),
            posts: Vec::new(),
        })
    }
}

#[cfg(test)]
mod path_tests {
    use super::path_words;

    #[test]
    fn path_words_drop_route_prefixes_numbers_and_short_bits() {
        assert_eq!(path_words("/post/hesper-apple-1"), ["hesper", "apple"]);
        assert_eq!(path_words("/page/2"), Vec::<String>::new());
        assert_eq!(path_words("/tags/Rust?x=1"), ["rust"]);
        assert_eq!(
            path_words("/%E5%BC%82%E6%AD%A5-rust/rust"),
            ["异步", "rust"]
        );
        assert_eq!(path_words("/a/b/c-d"), Vec::<String>::new());
        assert_eq!(path_words("/one/two/three/four/five/six").len(), 5);
    }
}
