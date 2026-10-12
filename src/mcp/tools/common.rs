//! MCP 工具共享辅助函数。
//!
//! `require_scope`/`require_admin`/`internal`/`ok_json` 此前在每个工具文件
//! （posts/comments/media/tags/settings/runner）各有一份逐字拷贝，仅 `tracing::error!`
//! 前缀字符串不同。集中到本模块，消除 ~10 份拷贝。

#![cfg(feature = "server")]

use rmcp::model::{CallToolResult, ContentBlock, TextContent};
use rmcp::ErrorData as McpError;

use crate::mcp::auth::McpPrincipal;
use crate::models::mcp_token::TokenScope;

/// 从 request.extensions 读取 McpPrincipal 并校验作用域。
///
/// 作用域不足返回 `insufficient_scope`；principal 缺失返回 `invalid_request`。
/// 成功返回 principal 克隆（调用方常需 `principal.user_id` 作 author_id）。
pub(super) fn require_scope(
    parts: &http::request::Parts,
    tool: &str,
    scope: TokenScope,
) -> Result<McpPrincipal, McpError> {
    let p = parts
        .extensions
        .get::<McpPrincipal>()
        .ok_or_else(|| McpError::invalid_request("missing MCP principal", None))?;
    if !p.scope.grants(scope) {
        return Err(McpError::invalid_request(
            format!("insufficient_scope: {tool} requires {}", scope.as_str()),
            None,
        ));
    }
    Ok(p.clone())
}

/// admin 作用域守卫：要求 `token.scope >= admin`。
pub(super) fn require_admin(parts: &http::request::Parts, tool: &str) -> Result<(), McpError> {
    let principal = parts
        .extensions
        .get::<McpPrincipal>()
        .ok_or_else(|| McpError::invalid_request("missing MCP principal", None))?;
    if !principal.scope.grants(TokenScope::Admin) {
        return Err(McpError::invalid_request(
            format!("insufficient_scope: {tool} requires admin"),
            None,
        ));
    }
    Ok(())
}

/// 记录错误详情并返回脱敏的 internal_error（不向客户端泄露 SQL 细节）。
///
/// 服务端日志保留完整 `{e}`；客户端只收到静态 `ctx`。
pub(super) fn internal<E: std::fmt::Display>(e: E, ctx: &'static str) -> McpError {
    tracing::error!("mcp {ctx}: {e}");
    McpError::internal_error(ctx, None)
}

/// 把可序列化值编码为 MCP 工具成功结果（pretty JSON 文本块）。
pub(super) fn ok_json<T: serde::Serialize>(val: T) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(&val).map_err(|e| internal(e, "encode result"))?;
    Ok(CallToolResult::success(vec![ContentBlock::Text(
        TextContent::new(text),
    )]))
}
