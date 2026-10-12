//! MCP 写作用域工具：文章 CRUD。
//!
//! bearer-token 鉴权后调用文章业务模块；协议参数与错误在此转换。
//! 每个写操作后执行与 web 后台完全一致的缓存失效（moka + SSR）。
//!
//! 本模块仅 `feature = "server"` 编译；`server.rs` 在最终装配时把 `posts_router`
//! 组合进单一 `ServerHandler`。

#![cfg(feature = "server")]
#![allow(clippy::too_many_arguments)]

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{schemars, tool, tool_router, ErrorData as McpError};
use serde::Deserialize;

use super::common::{internal, ok_json, require_scope};
use crate::api::posts::service::{
    self, FieldChange, PostAction, PostDraft, PostPatch, UpdatePost, WriteError,
};
use crate::db::pool::get_conn;
use crate::models::mcp_token::TokenScope;

// ---------------------------------------------------------------------------
// 结构体
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------

#[tool_router(router = posts_router, vis = "pub")]
impl crate::mcp::server::YggMcpServer {
    #[tool(
        description = "分页查询当前令牌用户的文章（含草稿）。需要 write 权限；status 可选 draft/published，省略返回全部未删除文章。query 按标题搜索，page 从 1 开始，per_page 为 1..=50。"
    )]
    async fn list_posts(
        &self,
        Parameters(p): Parameters<ListPostsParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "list_posts", TokenScope::Write)?;
        list_owned_posts(principal.user_id, p, false).await
    }

    #[tool(
        description = "按 ID 读取当前令牌用户的文章，包含草稿、Markdown 正文、封面、标签、状态和 updated_at。需要 write 权限，不读取回收站文章。"
    )]
    async fn get_post_by_id(
        &self,
        Parameters(p): Parameters<PostIdParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "get_post_by_id", TokenScope::Write)?;
        let client = get_conn().await.map_err(|e| internal(e, "db connection"))?;
        let row = client.query_opt(
            "SELECT p.id, p.author_id, p.title, p.slug, p.summary, p.content_md, p.content_html,
                p.toc_html, p.status, p.published_at, p.created_at, p.updated_at, p.cover_image,
                p.word_count, p.reading_time,
                COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as tags
                FROM posts p LEFT JOIN post_tags pt ON p.id = pt.post_id
                LEFT JOIN tags t ON pt.tag_id = t.id
                WHERE p.id = $1 AND p.author_id = $2 AND p.deleted_at IS NULL GROUP BY p.id",
            &[&p.post_id, &principal.user_id],
        ).await.map_err(|e| internal(e, "read post"))?;
        let row = row.ok_or_else(|| McpError::invalid_request("文章不存在或无权限", None))?;
        let post = crate::api::posts::helpers::row_to_post_full(&row)
            .await
            .map_err(|e| internal(format!("{e:?}"), "read post"))?;
        ok_json(post)
    }

    #[tool(
        description = "分页查询当前令牌用户的回收站文章，按删除时间倒序。支持 status、query、page、per_page，与 list_posts 相同。需要 write 权限。"
    )]
    async fn list_trashed_posts(
        &self,
        Parameters(p): Parameters<ListPostsParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "list_trashed_posts", TokenScope::Write)?;
        list_owned_posts(principal.user_id, p, true).await
    }

    #[tool(
        description = "恢复当前令牌用户的回收站文章，保留原发布状态。slug 被占用时自动追加数字后缀，返回恢复后的 slug。需要 write 权限。"
    )]
    async fn restore_post(
        &self,
        Parameters(p): Parameters<PostIdParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "restore_post", TokenScope::Write)?;
        let result = crate::api::posts::restore_post_impl(p.post_id, Some(principal.user_id))
            .await
            .map_err(|e| internal(e, "restore post"))?;
        ok_json(result)
    }

    #[tool(
        description = "创建一篇新文章。渲染 Markdown 为 HTML，同步标签与素材引用。返回 post_id/slug。"
    )]
    async fn create_post(
        &self,
        Parameters(p): Parameters<CreatePostParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "create_post", TokenScope::Write)?;

        let published_at = match p
            .published_at
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(date) => Some(parse_date_opt(date).ok_or_else(|| {
                McpError::invalid_request(
                    "published_at 格式无效，支持 YYYY-MM-DD 或 ISO 8601",
                    None,
                )
            })?),
            None => None,
        };
        let draft = PostDraft {
            title: p.title,
            slug: p.slug,
            summary: p.summary,
            content_md: p.content_md,
            status: p.status,
            tags: p.tags,
            cover_image: p.cover_image,
        };
        let result = service::create(principal.user_id, draft, published_at)
            .await
            .map_err(write_error)?;
        ok_json(PostResult {
            success: true,
            message: "创建成功".into(),
            post_id: Some(result.post_id),
            slug: Some(result.slug),
        })
    }

    /// 更新指定文章（PATCH 语义：仅更新提供的字段）。要求 write 作用域。
    /// 仅文章原作者可更新。
    #[tool(
        description = "部分更新一篇已有文章（PATCH 语义）。仅更新提供的字段：未提供 content_md 时跳过重新渲染；未提供 summary 时随 content_md 联动（自动提取或保留旧值）。仅文章原作者可更新。可传 expected_updated_at（RFC 3339）检测冲突；成功返回新 updated_at。"
    )]
    async fn update_post(
        &self,
        Parameters(p): Parameters<UpdatePostParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "update_post", TokenScope::Write)?;

        let expected_updated_at = parse_expected_updated_at(p.expected_updated_at.as_deref())?;
        let published_at = match p.published_at.as_deref() {
            Some(date) if date.trim().is_empty() => FieldChange::Clear,
            Some(date) => FieldChange::Set(parse_date_opt(date).ok_or_else(|| {
                McpError::invalid_request(
                    "published_at 格式无效，支持 YYYY-MM-DD 或 ISO 8601",
                    None,
                )
            })?),
            None => FieldChange::Keep,
        };
        let cover_image = match p.cover_image {
            Some(cover) if cover.trim().is_empty() => FieldChange::Clear,
            Some(cover) => FieldChange::Set(cover),
            None => FieldChange::Keep,
        };
        let patch = PostPatch {
            title: p.title,
            slug: p.slug,
            summary: p.summary,
            content_md: p.content_md,
            status: p.status,
            tags: p.tags,
            cover_image,
            published_at,
            expected_updated_at,
        };
        let result = service::update(principal.user_id, p.post_id, UpdatePost::Patch(patch))
            .await
            .map_err(write_error)?;
        ok_json(
            serde_json::json!({"success": true, "message": "更新成功", "post_id": result.post_id,
            "slug": result.slug, "updated_at": result.updated_at}),
        )
    }

    /// 发布指定文章（设置 status=published 与 published_at）。要求 write 作用域。
    #[tool(
        description = "发布一篇草稿文章。设置 status=published，若首次发布则填充 published_at。"
    )]
    async fn publish_post(
        &self,
        Parameters(p): Parameters<PostIdParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "publish_post", TokenScope::Write)?;
        let result = service::act(p.post_id, Some(principal.user_id), PostAction::Publish)
            .await
            .map_err(write_error)?;
        ok_json(PostResult {
            success: true,
            message: "发布成功".into(),
            post_id: Some(result.post_id),
            slug: Some(result.slug),
        })
    }

    /// 将指定文章移入回收站（软删除：设置 deleted_at）。要求 write 作用域。
    #[tool(description = "将文章移入回收站（软删除）。可通过恢复操作还原。")]
    async fn trash_post(
        &self,
        Parameters(p): Parameters<PostIdParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "trash_post", TokenScope::Write)?;
        let result = service::act(p.post_id, Some(principal.user_id), PostAction::Trash)
            .await
            .map_err(write_error)?;
        ok_json(PostResult {
            success: true,
            message: "已移入回收站".into(),
            post_id: Some(result.post_id),
            slug: Some(result.slug),
        })
    }

    /// 彻底删除指定文章（物理删除，不可恢复）。要求 write 作用域。
    #[tool(
        description = "彻底删除文章（物理删除，不可恢复）。post_tags 关联因外键 CASCADE 自动清理。"
    )]
    async fn delete_post(
        &self,
        Parameters(p): Parameters<PostIdParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "delete_post", TokenScope::Write)?;
        let result = service::act(p.post_id, Some(principal.user_id), PostAction::Delete)
            .await
            .map_err(write_error)?;
        ok_json(PostResult {
            success: true,
            message: "已彻底删除".into(),
            post_id: Some(result.post_id),
            slug: Some(result.slug),
        })
    }
}

/// 把共同业务错误转换为 MCP 错误，数据库细节只写日志。
fn write_error(error: WriteError) -> McpError {
    match error {
        WriteError::Invalid(message) => McpError::invalid_request(message, None),
        WriteError::Conflict(current_updated_at) => McpError::invalid_request(
            "conflict: 文章已被修改，请重新读取后合并更改",
            Some(serde_json::json!({"code":"conflict", "current_updated_at":current_updated_at})),
        ),
        WriteError::Failed(error) => internal(format!("{error:?}"), "post write"),
    }
}

// ---------------------------------------------------------------------------
// 参数与输出结构
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreatePostParams {
    /// 文章标题（必填，非空）。
    pub title: String,
    /// Markdown 正文（必填，非空）。
    pub content_md: String,
    /// 摘要；未提供时自动从正文提取。
    #[serde(default)]
    pub summary: Option<String>,
    /// URL slug；未提供时从标题自动生成。
    #[serde(default)]
    pub slug: Option<String>,
    /// 标签列表。
    #[serde(default)]
    pub tags: Vec<String>,
    /// 状态：`draft`（默认）或 `published`。
    #[serde(default = "default_status")]
    pub status: String,
    /// 封面图 URL。
    #[serde(default)]
    pub cover_image: Option<String>,
    /// 发布时间（ISO 8601 或 YYYY-MM-DD）；已发布文章写入 published_at，同时用作 created_at。
    /// 草稿只设置 created_at；未提供时使用当前时间。
    #[serde(default)]
    pub published_at: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdatePostParams {
    /// 上次读取的 updated_at（RFC 3339）。提供时原子校验版本，不匹配返回 conflict；省略保留原有行为。
    #[serde(default)]
    pub expected_updated_at: Option<String>,
    /// 要更新的文章 id。
    pub post_id: i32,
    /// 新标题。未提供则不修改（且不联动 slug）。
    #[serde(default)]
    pub title: Option<String>,
    /// 新 Markdown 正文。未提供则不重新渲染（content_html/toc/度量保持不变）。
    #[serde(default)]
    pub content_md: Option<String>,
    /// 新摘要。content_md 变化且未提供时自动从正文提取；content_md 未变且未提供则保留旧值。
    #[serde(default)]
    pub summary: Option<String>,
    /// 新 slug。未提供则不修改；提供则校验格式并自动去重。
    #[serde(default)]
    pub slug: Option<String>,
    /// 新标签列表。未提供则不修改；提供空列表则清空标签（完全替换旧标签）。
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// 新状态：`draft` / `published`。未提供则不修改。
    #[serde(default)]
    pub status: Option<String>,
    /// 新封面图 URL。未提供则不修改；空字符串清空封面。
    #[serde(default)]
    pub cover_image: Option<String>,
    /// 发布时间（ISO 8601 或 YYYY-MM-DD）；未提供则不修改。
    #[serde(default)]
    pub published_at: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PostIdParams {
    /// 文章 id。
    pub post_id: i32,
}

#[derive(Debug, serde::Serialize)]
struct PostResult {
    success: bool,
    message: String,
    post_id: Option<i32>,
    slug: Option<String>,
}

fn default_status() -> String {
    "draft".to_string()
}

fn parse_expected_updated_at(
    value: Option<&str>,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, McpError> {
    value
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|date| date.with_timezone(&chrono::Utc))
                .map_err(|_| McpError::invalid_params("expected_updated_at must be RFC 3339", None))
        })
        .transpose()
}

fn parse_date_opt(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let s = s.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(nd) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(ndt) = nd.and_hms_opt(0, 0, 0) {
            return Some(chrono::DateTime::from_naive_utc_and_offset(
                ndt,
                chrono::Utc,
            ));
        }
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(chrono::DateTime::from_naive_utc_and_offset(
            ndt,
            chrono::Utc,
        ));
    }
    None
}

#[derive(Debug, Deserialize, schemars::JsonSchema, Default)]
pub struct ListPostsParams {
    pub status: Option<String>,
    pub query: Option<String>,
    pub page: Option<i32>,
    pub per_page: Option<i32>,
}

impl ListPostsParams {
    fn normalize(&self) -> Result<(i32, i32, String), McpError> {
        if self
            .status
            .as_deref()
            .is_some_and(|s| !matches!(s, "draft" | "published"))
        {
            return Err(McpError::invalid_params(
                "status must be draft or published",
                None,
            ));
        }
        let query: String = self
            .query
            .as_deref()
            .unwrap_or("")
            .trim()
            .chars()
            .take(200)
            .collect();
        Ok((
            self.page.unwrap_or(1).max(1),
            self.per_page.unwrap_or(20).clamp(1, 50),
            format!("%{}%", crate::utils::server::escape_like_pattern(&query)),
        ))
    }
}

async fn list_owned_posts(
    user_id: i32,
    p: ListPostsParams,
    trashed: bool,
) -> Result<CallToolResult, McpError> {
    let (page, per_page, pattern) = p.normalize()?;
    let client = get_conn().await.map_err(|e| internal(e, "db connection"))?;
    let offset = (i64::from(page) - 1) * i64::from(per_page);
    let limit = i64::from(per_page);
    let deleted = if trashed { "IS NOT NULL" } else { "IS NULL" };
    let order = if trashed {
        "p.deleted_at"
    } else {
        "p.created_at"
    };
    let condition = format!("p.author_id = $1 AND p.deleted_at {deleted} AND ($2::text IS NULL OR p.status = $2) AND p.title ILIKE $3");
    let total: i64 = client
        .query_one(
            &format!("SELECT COUNT(*) FROM posts p WHERE {condition}"),
            &[&user_id, &p.status, &pattern],
        )
        .await
        .map_err(|e| internal(e, "count posts"))?
        .get(0);
    let rows = client
        .query(
            &format!(
                "SELECT p.id, p.author_id, p.title, p.slug, p.summary, p.status,
                p.published_at, p.created_at, p.updated_at, p.deleted_at, p.cover_image,
                p.word_count, p.reading_time,
                COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{{}}') as tags
                FROM posts p LEFT JOIN post_tags pt ON p.id = pt.post_id
                LEFT JOIN tags t ON pt.tag_id = t.id WHERE {condition}
                GROUP BY p.id ORDER BY {order} DESC, p.id DESC LIMIT $4 OFFSET $5"
            ),
            &[&user_id, &p.status, &pattern, &limit, &offset],
        )
        .await
        .map_err(|e| internal(e, "list posts"))?;
    let posts: Vec<_> = rows
        .iter()
        .map(crate::api::posts::helpers::row_to_post_list_item)
        .collect();
    ok_json(serde_json::json!({"posts": posts, "total": total, "page": page, "per_page": per_page}))
}

#[cfg(test)]
mod tests;
