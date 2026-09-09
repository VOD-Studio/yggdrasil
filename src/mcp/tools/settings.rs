//! 站点设置 MCP 工具：读取/更新回收站自动清理配置。
//!
//! 与 Web server-fn 共用 `api::settings` 的配置读写函数。鉴权入口各自保留：Web 走 cookie
//! `get_current_admin_user()`，MCP 走 bearer token → `McpPrincipal`，要求 admin 作用域。
//!
//! 缓存失效：与 web fn 保持一致——`update_trash_settings` **不做任何缓存失效**。
//! 理由：回收站配置只影响管理后台（SSR 缓存在 `admin/`，`invalidate_ssr_all_public`
//! 明确保留不动）和后台清理任务，没有公开页缓存表面，故无需失效。
//! （约束 #5 要求「按 web admin server fn 的方式失效」——该 fn 的方式就是不失效。）

#![cfg(feature = "server")]

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, TextContent};
use rmcp::{schemars, tool, tool_router, ErrorData as McpError};

use super::common::require_admin;
use serde::Deserialize;

use crate::api::error::AppError;
use crate::api::settings::{load_trash_settings, save_trash_settings};
use crate::db::pool::get_conn;

/// `get_settings` 入参（无字段——预留扩展点，未来可按子域过滤）。
#[derive(Debug, Deserialize, schemars::JsonSchema, Default)]
pub struct GetSettingsParams {}

/// `update_settings` 入参：两项回收站配置。
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateSettingsParams {
    /// 是否启用回收站自动清理。
    pub auto_purge_enabled: bool,
    /// 已删除文章保留天数（会被 clamp 到 [1, 365]）。
    pub retention_days: i32,
}

#[tool_router(router = settings_router, vis = "pub")]
impl crate::mcp::server::YggMcpServer {
    /// 读取站点回收站设置。要求 admin 作用域。
    #[tool(description = "读取站点回收站配置（自动清理开关 + 保留天数）。需要 admin 作用域。")]
    async fn get_settings(
        &self,
        Parameters(_p): Parameters<GetSettingsParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        require_admin(&parts, "get_settings")?;

        let settings = async {
            let client = get_conn().await.map_err(AppError::db_conn)?;
            load_trash_settings(&client).await
        }
        .await
        .map_err(|e| McpError::internal_error(format!("settings read failed: {e:?}"), None))?;

        let text = serde_json::to_string_pretty(&settings)
            .map_err(|e| McpError::internal_error(format!("encode failed: {e}"), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::Text(
            TextContent::new(text),
        )]))
    }

    /// 更新站点回收站设置。要求 admin 作用域。retention_days 会 clamp 到 [1, 365]。
    #[tool(
        description = "更新站点回收站配置（自动清理开关 + 保留天数）。retention_days 会被钳制到 1..=365。需要 admin 作用域。"
    )]
    async fn update_settings(
        &self,
        Parameters(UpdateSettingsParams {
            auto_purge_enabled,
            retention_days,
        }): Parameters<UpdateSettingsParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        require_admin(&parts, "update_settings")?;

        let updated = async {
            let client = get_conn().await.map_err(AppError::db_conn)?;
            save_trash_settings(&client, auto_purge_enabled, retention_days).await
        }
        .await
        .map_err(|e| McpError::internal_error(format!("settings write failed: {e:?}"), None))?;

        tracing::info!(
            "MCP: trash settings updated: auto_purge={}, retention_days={}",
            updated.auto_purge_enabled,
            updated.retention_days
        );

        let text = serde_json::to_string_pretty(&updated)
            .map_err(|e| McpError::internal_error(format!("encode failed: {e}"), None))?;
        Ok(CallToolResult::success(vec![ContentBlock::Text(
            TextContent::new(text),
        )]))
    }
}
