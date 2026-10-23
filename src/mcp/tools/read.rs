//! MCP 读作用域工具组（知识库）。
//!
//! 三个 `read` 作用域工具，均经 `Extension<http::request::Parts>` 读取鉴权中间件
//! 注入的 `McpPrincipal`，校验 `scope.grants(TokenScope::Read)`：
//! - `search_posts`：pg_trgm 模糊搜索已发布文章；
//! - `get_post`：按 slug 取单篇已发布文章全文；
//! - `list_tags`：标签 + 关联已发布文章数。
//!
//! 本模块只声明工具路由；`server.rs`（Main 装配）把 `read_router()` 合并进
//! 复合 `ServerHandler`。SQL 与 `src/api/posts/{search,read,tags}.rs` 的
//! server-fn 一致，但不复用后者（它们走 cookie 鉴权的请求上下文），
//! 这里直接走 `crate::db::pool::get_conn()`。

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, TextContent};
use rmcp::{schemars, tool, tool_router, ErrorData as McpError};
use serde::Deserialize;

use crate::db::pool::get_conn;
use crate::mcp::auth::McpPrincipal;
use crate::models::mcp_token::TokenScope;

#[tool_router(router = read_router, vis = "pub")]
impl crate::mcp::server::YggMcpServer {
    /// 全文搜索已发布文章（知识库）。返回标题/slug/摘要/标签。
    #[tool(
        description = "全文搜索已发布文章，作为知识库。返回标题/slug/摘要/标签与匹配 URL。要求 read 作用域。"
    )]
    async fn search_posts(
        &self,
        Parameters(SearchPostsParams { query, limit }): Parameters<SearchPostsParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_read(&parts, "search_posts")?;

        let hits = search_published(&query, limit.unwrap_or(50))
            .await
            .map_err(|e| mcp_internal("search_posts", &principal, &e))?;

        let text = serde_json::to_string_pretty(&hits)
            .map_err(|e| McpError::internal_error(format!("encode failed: {e}"), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::Text(
            TextContent::new(text),
        )]))
    }

    /// 按 slug 取单篇已发布文章（草稿对 read 不可见）。
    #[tool(
        description = "按 slug 读取单篇已发布文章全文（标题/摘要/Markdown 正文/标签/时间）。草稿不可见。要求 read 作用域。"
    )]
    async fn get_post(
        &self,
        Parameters(GetPostParams { slug }): Parameters<GetPostParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_read(&parts, "get_post")?;

        let post = get_published_by_slug(&slug)
            .await
            .map_err(|e| mcp_internal("get_post", &principal, &e))?;

        let text = serde_json::to_string_pretty(&post)
            .map_err(|e| McpError::internal_error(format!("encode failed: {e}"), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::Text(
            TextContent::new(text),
        )]))
    }

    /// 列出全部标签 + 各自关联的已发布文章数。
    #[tool(description = "列出全部标签及其关联的已发布文章数量。要求 read 作用域。")]
    async fn list_tags(
        &self,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_read(&parts, "list_tags")?;

        let tags = list_all_tags()
            .await
            .map_err(|e| mcp_internal("list_tags", &principal, &e))?;

        let text = serde_json::to_string_pretty(&tags)
            .map_err(|e| McpError::internal_error(format!("encode failed: {e}"), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::Text(
            TextContent::new(text),
        )]))
    }
}

// ── 鉴权辅助 ───────────────────────────────────────────────────────────────

/// 从 request extensions 取 principal 并校验 read 作用域；不足返回 insufficient_scope。
fn require_read(parts: &http::request::Parts, tool: &str) -> Result<McpPrincipal, McpError> {
    let principal = parts
        .extensions
        .get::<McpPrincipal>()
        .ok_or_else(|| McpError::invalid_request("missing MCP principal", None))?;
    if !principal.scope.grants(TokenScope::Read) {
        return Err(McpError::invalid_request(
            format!("insufficient_scope: {tool} requires read"),
            None,
        ));
    }
    Ok(principal.clone())
}

/// 把内部错误统一包装成 MCP internal_error（带工具名便于排查）。
fn mcp_internal(tool: &str, principal: &McpPrincipal, e: &str) -> McpError {
    tracing::warn!(
        tool, user_id = principal.user_id, error = %e, "MCP read tool failed"
    );
    McpError::internal_error(format!("{tool} failed: {e}"), None)
}

// ── 参数 DTO ───────────────────────────────────────────────────────────────

/// `search_posts` 入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchPostsParams {
    /// 搜索关键词（SQL 通配符会被转义；空串返回空结果）。
    pub query: String,
    /// 最多返回条数（1–50，默认 50）。
    #[serde(default)]
    pub limit: Option<u32>,
}

/// `get_post` 入参。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetPostParams {
    /// 文章 slug。
    pub slug: String,
}

// ── 输出 DTO ───────────────────────────────────────────────────────────────

/// `search_posts` 命中项（精简，不含正文）。
#[derive(Debug, serde::Serialize)]
pub struct SearchHit {
    pub id: i32,
    pub title: String,
    pub slug: String,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    /// 站内文章 URL（`/post/{slug}`），便于客户端/LLM 引用。
    pub url: String,
}

/// `get_post` 返回的单篇已发布文章（含正文）。
#[derive(Debug, serde::Serialize)]
pub struct PostResource {
    pub id: i32,
    pub title: String,
    pub slug: String,
    pub summary: Option<String>,
    pub content_md: String,
    pub tags: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 站内文章 URL。
    pub url: String,
}

/// `list_tags` 返回的标签项。
#[derive(Debug, serde::Serialize)]
pub struct TagCount {
    pub id: i32,
    pub name: String,
    pub post_count: i64,
}

// ── DB 访问（SQL 镜像 src/api/posts/{search,read,tags}.rs，走连接池） ───────

/// 把行内的标签数组原地清洗（过滤空串），避免二次 Vec 分配。
fn clean_tags(row: &tokio_postgres::Row) -> Vec<String> {
    let mut tags: Vec<String> = row.try_get::<_, Vec<String>>("tags").unwrap_or_default();
    tags.retain(|t| !t.is_empty());
    tags
}

/// 站内文章 URL：`/post/{slug}`（MCP 不知晓外部域名，给相对路径；客户端可拼接 APP_BASE_URL）。
fn post_url(slug: &str) -> String {
    format!("/post/{slug}")
}

/// 与 `src/api/posts/search.rs` 一致的 pg_trgm word_similarity 查询。
///
/// 仅返回 `status='published' AND deleted_at IS NULL` 的文章。
/// SQL 通配符 `%`/`_`/`\` 被转义，防止用户输入注入通配符扩大匹配范围（匹配走 search_text 上的 trigram GIN 索引）。
pub async fn search_published(query: &str, limit: u32) -> Result<Vec<SearchHit>, String> {
    let q = query.trim();
    if q.is_empty() || q.chars().count() > 200 {
        return Ok(Vec::new());
    }
    // 钳制 limit 到 [1,50]（与 web 端 search 的硬上限一致）。
    let limit = limit.clamp(1, 50) as i64;

    let client = get_conn().await.map_err(|e| format!("db conn: {e}"))?;

    let escaped = crate::utils::server::escape_like_pattern(q);

    let rows = client
        .query(
            "SELECT p.id, p.title, p.slug, p.summary,
                    COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as tags
             FROM posts p
             LEFT JOIN post_tags pt ON p.id = pt.post_id
             LEFT JOIN tags t ON pt.tag_id = t.id
             WHERE p.status = 'published' AND p.deleted_at IS NULL
               AND p.search_text ILIKE '%' || $1 || '%' ESCAPE '\\'
             GROUP BY p.id, p.search_text
             ORDER BY word_similarity(p.search_text, $2) DESC, p.published_at DESC
             LIMIT $3",
            &[&escaped, &q, &limit],
        )
        .await
        .map_err(|e| format!("query: {e}"))?;

    let hits = rows
        .iter()
        .map(|r| {
            let slug: String = r.get("slug");
            SearchHit {
                id: r.get("id"),
                title: r.get("title"),
                slug: slug.clone(),
                summary: r.get("summary"),
                tags: clean_tags(r),
                url: post_url(&slug),
            }
        })
        .collect();
    Ok(hits)
}

/// 按 slug 取单篇已发布文章（草稿/已删除返回 None）。
///
/// SQL 镜像 `src/api/posts/read.rs::get_post_by_slug` 的 published 过滤，
/// 但省略上下篇导航（MCP 知识库场景不需要），返回 `content_md` 原文。
pub async fn get_published_by_slug(slug: &str) -> Result<Option<PostResource>, String> {
    let client = get_conn().await.map_err(|e| format!("db conn: {e}"))?;

    let row = client
        .query_opt(
            "SELECT p.id, p.title, p.slug, p.summary, p.content_md,
                    p.created_at, p.published_at,
                    COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as tags
             FROM posts p
             LEFT JOIN post_tags pt ON p.id = pt.post_id
             LEFT JOIN tags t ON pt.tag_id = t.id
             WHERE p.slug = $1 AND p.status = 'published' AND p.deleted_at IS NULL
             GROUP BY p.id",
            &[&slug],
        )
        .await
        .map_err(|e| format!("query: {e}"))?;

    Ok(row.map(|r| {
        let s: String = r.get("slug");
        PostResource {
            id: r.get("id"),
            title: r.get("title"),
            slug: s.clone(),
            summary: r.get("summary"),
            content_md: r.get("content_md"),
            tags: clean_tags(&r),
            created_at: r.get("created_at"),
            published_at: r.get("published_at"),
            url: post_url(&s),
        }
    }))
}

/// 全部标签 + 各自关联的已发布文章数（镜像 `src/api/posts/tags.rs::list_tags`）。
pub async fn list_all_tags() -> Result<Vec<TagCount>, String> {
    let client = get_conn().await.map_err(|e| format!("db conn: {e}"))?;

    let rows = client
        .query(
            "SELECT t.id, t.name, COUNT(p.id) as post_count
             FROM tags t
             LEFT JOIN post_tags pt ON t.id = pt.tag_id
             LEFT JOIN posts p ON pt.post_id = p.id AND p.deleted_at IS NULL AND p.status = 'published'
             GROUP BY t.id, t.name
             ORDER BY t.name",
            &[],
        )
        .await
        .map_err(|e| format!("query: {e}"))?;

    Ok(rows
        .iter()
        .map(|r| TagCount {
            id: r.get("id"),
            name: r.get("name"),
            post_count: r.get("post_count"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_url_format() {
        assert_eq!(post_url("hello"), "/post/hello");
        assert_eq!(post_url("a-b_c"), "/post/a-b_c");
    }

    #[test]
    fn search_published_empty_query_returns_empty() {
        // 纯逻辑：空/超长查询直接返回空，不发 DB。
        // 用一个 fake runtime 验证 early-return（不连库）。
        let rt = tokio::runtime::Runtime::new().expect("rt");
        let empty = rt.block_on(search_published("", 10)).expect("empty ok");
        assert!(empty.is_empty());
        let spaces = rt.block_on(search_published("   ", 10)).expect("spaces ok");
        assert!(spaces.is_empty());
        let long: String = "x".repeat(201);
        let long_hit = rt.block_on(search_published(&long, 10)).expect("long ok");
        assert!(long_hit.is_empty());
    }

    #[test]
    fn limit_is_clamped_in_signature_not_query() {
        // limit 钳制发生在 DB 调用前；这里只验证 clamp 算术（不触发 DB）。
        assert_eq!(0u32.clamp(1, 50), 1);
        assert_eq!(51u32.clamp(1, 50), 50);
        assert_eq!(10u32.clamp(1, 50), 10);
    }
}
