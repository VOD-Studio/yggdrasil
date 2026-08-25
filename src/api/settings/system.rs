// 与 posts 模块一致：Dioxus `#[server]` 宏触发 deprecated/unit/too_many_arguments
// 提示，按项目惯例放行（限流/运行器等配置项天然参数多）。
#![allow(clippy::unused_unit, deprecated, clippy::too_many_arguments)]

use dioxus::prelude::*;

#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
use crate::models::settings::SystemInfo;

// ============================================================================
// 系统启动配置（只读展示）
// ============================================================================

/// 读取系统启动配置的只读快照（面板展示用）。
///
/// 这些值在进程启动时读取，不可运行时修改。密钥类仅展示是否已设置。
#[server(GetSystemInfo, "/api")]
pub async fn get_system_info() -> Result<SystemInfo, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        // 脱敏 DATABASE_URL：隐藏密码，仅保留 scheme://host:port/dbname。
        let database_url_masked = {
            let url = std::env::var("DATABASE_URL").unwrap_or_default();
            // 简单脱敏：如果含 @（即含密码），截取 @ 之后的部分 + scheme 前缀。
            if let Some(at_pos) = url.find('@') {
                let scheme_end = url.find("://").unwrap_or(0);
                let after = &url[at_pos + 1..];
                format!("{}://{}", &url[..scheme_end], after)
            } else {
                url
            }
        };

        Ok(SystemInfo {
            database_url_masked,
            rust_log: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            db_pool_size: std::env::var("DB_POOL_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(20),
            statement_timeout_secs: std::env::var("STATEMENT_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            ssr_cache_secs: crate::utils::server::parse_ssr_cache_secs(),
            compression_algorithms: std::env::var("COMPRESSION_ALGORITHMS").unwrap_or_default(),
            expose_version_headers: crate::utils::server::parse_env_bool(
                "EXPOSE_VERSION_HEADERS",
                true,
            ),
            docker_socket_path: std::env::var("DOCKER_SOCKET_PATH")
                .unwrap_or_else(|_| "/var/run/docker.sock".to_string()),
            mcp_token_enc_key_set: std::env::var("MCP_TOKEN_ENC_KEY")
                .map(|v| !v.is_empty())
                .unwrap_or(false),
            migrate_startup_timeout_secs: std::env::var("MIGRATE_STARTUP_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            sysinfo_sample_secs: std::env::var("SYSINFO_SAMPLE_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.5),
        })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(SystemInfo {
            database_url_masked: String::new(),
            rust_log: String::new(),
            db_pool_size: 0,
            statement_timeout_secs: 0,
            ssr_cache_secs: 0,
            compression_algorithms: String::new(),
            expose_version_headers: false,
            docker_socket_path: String::new(),
            mcp_token_enc_key_set: false,
            migrate_startup_timeout_secs: 0,
            sysinfo_sample_secs: 0.0,
        })
    }
}
