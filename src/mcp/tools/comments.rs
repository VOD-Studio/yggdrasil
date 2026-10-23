//! MCP 写作用域工具：评论审核管理。
//!
//! 委托 `src/api/comments` 的 `_impl`，仅鉴权方式不同（bearer token，不走 cookie）。
//! 状态变更后的评论缓存失效在 `_impl` 内完成，与 web 后台一致。
//!
//! 本模块仅 `feature = "server"` 编译。

#![cfg(feature = "server")]

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{schemars, tool, tool_router, ErrorData as McpError};
use serde::Deserialize;

use super::common::{internal, ok_json, require_scope};
use crate::models::mcp_token::TokenScope;

#[tool_router(router = comments_router, vis = "pub")]
impl crate::mcp::server::YggMcpServer {
    /// 列出评论（分页，可按状态筛选）。要求 write 作用域。
    #[tool(
        description = "列出全部评论（分页，每页 20 条）。可按状态筛选：pending/approved/spam/trash。"
    )]
    async fn list_comments(
        &self,
        Parameters(p): Parameters<ListCommentsParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let _principal = require_scope(&parts, "list_comments", TokenScope::Write)?;

        let page = p.page.unwrap_or(1).max(1);
        let resp = crate::api::comments::list_admin_comments_impl(page, p.status.clone())
            .await
            .map_err(|e| internal(format!("{e:?}"), "list comments"))?;

        // 不暴露邮箱与头像。
        let comments: Vec<CommentItem> = resp
            .comments
            .into_iter()
            .map(|c| CommentItem {
                id: c.id,
                post_id: c.post_id,
                post_title: c.post_title,
                post_slug: c.post_slug,
                parent_id: c.parent_id,
                depth: c.depth,
                author_name: c.author_name,
                author_url: c.author_url,
                content_md: c.content_md,
                status: c.status,
                created_at: c.created_at.to_rfc3339(),
            })
            .collect();

        ok_json(CommentsList {
            comments,
            total: resp.total,
            page,
            per_page: 20,
        })
    }

    /// 通过指定评论（同时递归通过所有 pending 祖先评论）。要求 write 作用域。
    #[tool(description = "通过指定评论。同时递归通过所有 pending 的祖先评论，确保嵌套链可见。")]
    async fn approve_comment(
        &self,
        Parameters(p): Parameters<CommentIdParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let _principal = require_scope(&parts, "approve_comment", TokenScope::Write)?;

        let found = crate::api::comments::approve_comment_impl(p.comment_id)
            .await
            .map_err(|e| internal(format!("{e:?}"), "approve comment"))?;
        if !found {
            return Err(McpError::invalid_request("评论不存在", None));
        }

        ok_json(CommentResult {
            success: true,
            message: "已通过".into(),
        })
    }

    /// 删除指定评论（软删除：设置 deleted_at 与 status=trash）。要求 write 作用域。
    #[tool(description = "删除指定评论（移入回收站，软删除）。")]
    async fn delete_comment(
        &self,
        Parameters(p): Parameters<CommentIdParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let _principal = require_scope(&parts, "delete_comment", TokenScope::Write)?;

        crate::api::comments::trash_comment_impl(p.comment_id)
            .await
            .map_err(|e| internal(format!("{e:?}"), "trash comment"))?;

        ok_json(CommentResult {
            success: true,
            message: "已删除".into(),
        })
    }

    /// 设置评论状态（approved/spam/trash）。要求 write 作用域。
    #[tool(description = "设置评论审核状态。status 可选 approved/spam/trash。trash 会软删除。")]
    async fn set_comment_status(
        &self,
        Parameters(p): Parameters<SetCommentStatusParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let _principal = require_scope(&parts, "set_comment_status", TokenScope::Write)?;

        let normalized = p.status.trim().to_lowercase();
        if !matches!(normalized.as_str(), "approved" | "spam" | "trash") {
            return Err(McpError::invalid_request(
                "status must be one of: approved, spam, trash",
                None,
            ));
        }

        let found = match normalized.as_str() {
            // 统一为递归通过 pending 祖先，与 approve_comment 工具一致。
            "approved" => crate::api::comments::approve_comment_impl(p.comment_id).await,
            "spam" => crate::api::comments::spam_comment_impl(p.comment_id).await,
            "trash" => crate::api::comments::trash_comment_impl(p.comment_id).await,
            _ => unreachable!("validated above"),
        }
        .map_err(|e| internal(format!("{e:?}"), "set comment status"))?;
        if !found {
            return Err(McpError::invalid_request("评论不存在", None));
        }

        ok_json(CommentResult {
            success: true,
            message: format!("状态已设为 {normalized}"),
        })
    }
}

// ---------------------------------------------------------------------------
// 参数与输出结构
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListCommentsParams {
    /// 页码（从 1 开始，默认 1）。
    #[serde(default)]
    pub page: Option<i32>,
    /// 按状态筛选：pending / approved / spam / trash。不传则返回全部。
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CommentIdParams {
    /// 评论 id。
    pub comment_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetCommentStatusParams {
    /// 评论 id。
    pub comment_id: i64,
    /// 目标状态：approved / spam / trash。
    pub status: String,
}

#[derive(Debug, serde::Serialize)]
struct CommentItem {
    id: i64,
    post_id: i32,
    post_title: String,
    post_slug: String,
    parent_id: Option<i64>,
    depth: i32,
    author_name: String,
    author_url: Option<String>,
    content_md: String,
    status: crate::models::comment::CommentStatus,
    created_at: String,
}

#[derive(Debug, serde::Serialize)]
struct CommentsList {
    comments: Vec<CommentItem>,
    total: i64,
    page: i32,
    per_page: i64,
}

#[derive(Debug, serde::Serialize)]
struct CommentResult {
    success: bool,
    message: String,
}
