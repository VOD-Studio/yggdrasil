//! 构建并返回挂载于 `/mcp` 的 axum 子路由。
//!
//! 装配：
//! - rmcp `StreamableHttpService`（无状态、JSON 响应、Origin 白名单 + Host 白名单）；
//! - 本 crate 的 `mcp_auth_middleware`（bearer → McpPrincipal，注入 extensions）；
//!
//! 两道白名单都源自 `APP_BASE_URL`：
//! - **Origin**：整串（含 scheme），rmcp 按 scheme+host 校验 `Origin` 头；
//! - **Host**：仅 host 部分，加入 rmcp 的 DNS-rebinding 防护默认表（localhost/127.0.0.1/::1）。
//!   生产反代 `proxy_set_header Host $host` 会把真实域名转发进来，必须加入否则 rmcp 一律 403。
//!
//! 未设置 `APP_BASE_URL` 时，rmcp 允许省略 Origin，Host 白名单仅包含本地地址。
//! 生产部署需要设置 `APP_BASE_URL`，见 README 的生产部署说明。
//! 协议版本头、体积上限（4MiB 默认）由 rmcp 内置，无需此处重复。

use axum::middleware::from_fn;
use axum::Router;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::mcp::auth::mcp_auth_middleware;
use crate::mcp::server::YggMcpServer;

/// 构建 `/mcp` 子路由（供 main.rs 的 `.merge(mcp_route)` 合并）。
///
/// 仅在 server feature 下有意义；WASM 构建不会调用本函数。
pub fn mcp_route() -> Router {
    // 读取 APP_BASE_URL：同时用于 Origin 白名单（整串含 scheme）与 Host 白名单（仅 host）。
    let base_url = std::env::var("APP_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // Origin 白名单：APP_BASE_URL 整串（含 scheme），rmcp 按 scheme+host[:port] 匹配 Origin 头。
    // 未设置则空——rmcp 对缺 Origin 头的请求放行（大多数原生 MCP 客户端不发 Origin）。
    let allowed_origins: Vec<String> = base_url.iter().cloned().collect();

    // Host 白名单（rmcp 的 DNS-rebinding 防护）。默认仅允许 localhost/127.0.0.1/::1：
    // 生产反代（nginx `proxy_set_header Host $host`）会把真实域名转发进来，若不把
    // APP_BASE_URL 的 host 也加入，所有 MCP 请求会被 rmcp 以 403 拒绝。保留默认值，
    // 确保本地直连（localhost）仍可用；只取 host（不含 port），rmcp 对无 port 条目匹配任意 port。
    let mut allowed_hosts: Vec<String> = vec!["localhost".into(), "127.0.0.1".into(), "::1".into()];
    if let Some(base) = &base_url {
        if let Ok(uri) = http::Uri::try_from(base.as_str()) {
            if let Some(authority) = uri.authority() {
                let host = authority.host().to_lowercase();
                if !allowed_hosts.contains(&host) {
                    allowed_hosts.push(host);
                }
            }
        }
    }

    let mut config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false) // 无状态（SEP-2567）
        .with_json_response(true) // 简单工具用 application/json 直回
        .with_allowed_hosts(allowed_hosts.iter().map(|s| s.as_str()));
    if !allowed_origins.is_empty() {
        config = config.with_allowed_origins(allowed_origins.iter().map(|s| s.as_str()));
    }

    let service = StreamableHttpService::new(
        || Ok::<_, std::io::Error>(YggMcpServer),
        LocalSessionManager::default().into(),
        config,
    );

    Router::new()
        .nest_service("/mcp", service)
        .layer(from_fn(mcp_auth_middleware))
}
