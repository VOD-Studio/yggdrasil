//! MCP 写作用域工具：标签管理。
//!
//! `create_tag` 与 `rename_tag` 没有独立的 web 后台 server-fn（标签在文章
//! 保存时由 `sync_tags` 隐式创建）。本模块直接操作 tags 表，并在写后经
//! `invalidate_for_tag_write` 失效文章/标签类缓存与公开页 SSR 缓存。
//!
//! 本模块仅 `feature = "server"` 编译。

#![cfg(feature = "server")]

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{schemars, tool, tool_router, ErrorData as McpError};
use serde::Deserialize;

use super::common::{internal, ok_json, require_scope};
use crate::cache;
use crate::db::pool::get_conn;
use crate::models::mcp_token::TokenScope;

#[tool_router(router = tags_router, vis = "pub")]
impl crate::mcp::server::YggMcpServer {
    /// 创建一个新标签。若同名标签已存在则返回已有标签 id。要求 write 作用域。
    #[tool(description = "创建一个新标签。若同名标签已存在则返回其 id（幂等）。")]
    async fn create_tag(
        &self,
        Parameters(p): Parameters<CreateTagParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let _principal = require_scope(&parts, "create_tag", TokenScope::Write)?;

        let name = p.name.trim().to_string();
        if name.is_empty() {
            return Err(McpError::invalid_request("name must not be empty", None));
        }

        let client = get_conn().await.map_err(|e| internal(e, "db connection"))?;

        // 与 sync_tags 一致的 upsert 语义。
        let row = client
            .query_opt(
                "INSERT INTO tags (name) VALUES ($1) ON CONFLICT (name) DO NOTHING RETURNING id",
                &[&name],
            )
            .await
            .map_err(|e| internal(e, "insert tag"))?;
        let (tag_id, created): (i32, bool) = match row {
            Some(r) => (r.get(0), true),
            None => {
                let r = client
                    .query_one("SELECT id FROM tags WHERE name = $1", &[&name])
                    .await
                    .map_err(|e| internal(e, "select existing tag"))?;
                (r.get(0), false)
            }
        };

        // 同名标签已存在时数据未变，无需冲刷缓存。
        if created {
            cache::invalidate_for_tag_write();
        }

        ok_json(TagResult {
            success: true,
            message: if created {
                "标签已创建".into()
            } else {
                "标签已存在".into()
            },
            tag_id: Some(tag_id),
            name,
        })
    }

    /// 重命名指定标签。要求 write 作用域。
    #[tool(description = "重命名指定标签。若目标名称已被其他标签占用则报错。")]
    async fn rename_tag(
        &self,
        Parameters(p): Parameters<RenameTagParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let _principal = require_scope(&parts, "rename_tag", TokenScope::Write)?;

        let name = p.name.trim().to_string();
        if name.is_empty() {
            return Err(McpError::invalid_request("name must not be empty", None));
        }

        let client = get_conn().await.map_err(|e| internal(e, "db connection"))?;

        // 检查目标名称是否已被其他标签占用（排除自身）。
        let conflict = client
            .query_opt(
                "SELECT 1 FROM tags WHERE name = $1 AND id != $2",
                &[&name, &p.tag_id],
            )
            .await
            .map_err(|e| internal(e, "check conflict"))?;
        if conflict.is_some() {
            return Err(McpError::invalid_request(
                format!("标签名「{name}」已被占用"),
                None,
            ));
        }

        let result = client
            .execute(
                "UPDATE tags SET name = $1 WHERE id = $2",
                &[&name, &p.tag_id],
            )
            .await
            .map_err(|e| internal(e, "rename tag"))?;
        if result == 0 {
            return Err(McpError::invalid_request("标签不存在", None));
        }

        cache::invalidate_for_tag_write();

        ok_json(TagResult {
            success: true,
            message: "标签已重命名".into(),
            tag_id: Some(p.tag_id),
            name,
        })
    }
}

// ---------------------------------------------------------------------------
// 参数与输出结构
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTagParams {
    /// 标签名称。
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameTagParams {
    /// 要重命名的标签 id。
    pub tag_id: i32,
    /// 新标签名称。
    pub name: String,
}

#[derive(Debug, serde::Serialize)]
struct TagResult {
    success: bool,
    message: String,
    tag_id: Option<i32>,
    name: String,
}
