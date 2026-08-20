// 与 posts 模块一致：Dioxus `#[server]` 宏触发 deprecated/unit/too_many_arguments
// 提示，按项目惯例放行（限流/运行器等配置项天然参数多）。
#![allow(clippy::unused_unit, deprecated, clippy::too_many_arguments)]

use dioxus::prelude::*;

#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;
use crate::models::settings::SecuritySettings;
// 仅 server 构建的函数体引用（WASM 端 server fn 体被宏剥离）。
#[cfg(feature = "server")]
use crate::cache::invalidate_security_settings;

// ============================================================================
// 安全配置（即时生效）
// ============================================================================

/// 启动时用 `APP_BASE_URL` / `COOKIE_SECURE` / `TRUSTED_PROXY_COUNT` /
/// `MAX_SESSIONS_PER_USER` 环境变量播种安全配置。
///
/// 语义与 [`seed_backup_settings_from_env`][crate::api::settings::seed_backup_settings_from_env] 一致：仅当对应 settings 键**不存在**时
/// 插入（首次部署），之后以「站点配置 → 安全」面板写入的 DB 值为准，重启不被
/// env 覆盖。单个变量非法只告警跳过，不影响其他变量与启动。
#[cfg(feature = "server")]
pub(crate) async fn seed_security_settings_from_env(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    let mut seeds: Vec<(&'static str, String)> = Vec::new();

    if let Ok(v) = std::env::var("APP_BASE_URL") {
        let url = crate::models::settings::SecuritySettings::normalize_app_base_url(&v);
        if !url.is_empty() {
            seeds.push(("security_app_base_url", url));
        }
    }
    if let Ok(v) = std::env::var("COOKIE_SECURE") {
        match v.trim().parse::<bool>() {
            Ok(b) => seeds.push(("security_cookie_secure", b.to_string())),
            Err(_) => tracing::warn!("COOKIE_SECURE={v:?} 非法（期望 true/false），跳过"),
        }
    }
    if let Ok(v) = std::env::var("TRUSTED_PROXY_COUNT") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "security_trusted_proxy_count",
                crate::models::settings::SecuritySettings::clamp_trusted_proxy_count(n).to_string(),
            )),
            Err(_) => tracing::warn!("TRUSTED_PROXY_COUNT={v:?} 非法（期望非负整数），跳过"),
        }
    }
    if let Ok(v) = std::env::var("MAX_SESSIONS_PER_USER") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "security_max_sessions_per_user",
                crate::models::settings::SecuritySettings::clamp_max_sessions(n).to_string(),
            )),
            Err(_) => tracing::warn!("MAX_SESSIONS_PER_USER={v:?} 非法（期望正整数），跳过"),
        }
    }

    for (key, value) in seeds {
        client
            .execute(
                "INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO NOTHING",
                &[&key, &value],
            )
            .await
            .map_err(AppError::query)?;
        tracing::info!("安全配置已从环境变量播种: {key}={value}（仅键缺失时生效）");
    }
    Ok(())
}

/// 从 settings 表读取安全配置（缺键回退默认值）。
///
/// 供面板 `get_security_settings` 与运行时读取点（CSRF / cookie / 真实 IP /
/// 登录会话上限）共用：先查 moka 缓存，未命中查 DB 并回填。
#[cfg(feature = "server")]
pub(crate) async fn load_security_settings(
    client: &tokio_postgres::Client,
) -> Result<crate::models::settings::SecuritySettings, AppError> {
    async fn read_key(
        client: &tokio_postgres::Client,
        key: &str,
    ) -> Result<Option<String>, AppError> {
        let row = client
            .query_opt("SELECT value FROM settings WHERE key = $1", &[&key])
            .await
            .map_err(AppError::query)?;
        Ok(row.map(|r| r.get::<_, String>("value")))
    }

    let app_base_url = read_key(client, "security_app_base_url")
        .await?
        .map(|v| crate::models::settings::SecuritySettings::normalize_app_base_url(&v))
        .unwrap_or_default();
    let cookie_secure = read_key(client, "security_cookie_secure")
        .await?
        .and_then(|v| v.parse().ok())
        .unwrap_or(crate::models::settings::DEFAULT_COOKIE_SECURE);
    let trusted_proxy_count = read_key(client, "security_trusted_proxy_count")
        .await?
        .and_then(|v| v.parse().ok())
        .map(crate::models::settings::SecuritySettings::clamp_trusted_proxy_count)
        .unwrap_or(crate::models::settings::DEFAULT_TRUSTED_PROXY_COUNT);
    let max_sessions_per_user = read_key(client, "security_max_sessions_per_user")
        .await?
        .and_then(|v| v.parse().ok())
        .map(crate::models::settings::SecuritySettings::clamp_max_sessions)
        .unwrap_or(crate::models::settings::DEFAULT_MAX_SESSIONS_PER_USER);

    Ok(crate::models::settings::SecuritySettings {
        app_base_url,
        cookie_secure,
        trusted_proxy_count,
        max_sessions_per_user,
    })
}

/// 运行时读取安全配置：先查 moka 缓存，未命中查 DB 并回填，全失败回退默认值。
///
/// 供运行时读取点（CSRF / cookie / 真实 IP / 登录会话上限）调用，
/// 替代原来的 `std::env::var` 每请求读取。
#[cfg(feature = "server")]
pub(crate) async fn runtime_security_settings() -> crate::models::settings::SecuritySettings {
    if let Some(s) = crate::cache::get_security_settings().await {
        return s;
    }
    let fallback = crate::models::settings::SecuritySettings::default();
    // 无 DATABASE_URL 时（如单元测试环境）直接回退默认值，
    // 避免触发 DB_POOL LazyLock 的防御性 panic。
    if std::env::var("DATABASE_URL").is_err() {
        return fallback;
    }
    match get_conn().await {
        Ok(client) => match load_security_settings(&client).await {
            Ok(s) => {
                crate::cache::set_security_settings(s.clone()).await;
                s
            }
            Err(e) => {
                tracing::warn!("读取安全配置失败，回退默认值：{e:?}");
                fallback
            }
        },
        Err(e) => {
            tracing::warn!("获取连接读取安全配置失败，回退默认值：{e:?}");
            fallback
        }
    }
}

/// 读取安全配置（面板用）。
#[server(GetSecuritySettings, "/api")]
pub async fn get_security_settings() -> Result<SecuritySettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let s = load_security_settings(&client)
            .await
            .map_err(ServerFnError::from)?;
        crate::cache::set_security_settings(s.clone()).await;
        Ok(s)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(SecuritySettings::default())
    }
}

/// 更新安全配置。
///
/// 所有字段会被规范化/clamp 后写入。写入后失效 moka 缓存，数秒内全链路生效。
#[server(UpdateSecuritySettings, "/api")]
pub async fn update_security_settings(
    app_base_url: String,
    cookie_secure: bool,
    trusted_proxy_count: u32,
    max_sessions_per_user: u32,
) -> Result<SecuritySettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let app_base_url = SecuritySettings::normalize_app_base_url(&app_base_url);
    let trusted_proxy_count = SecuritySettings::clamp_trusted_proxy_count(trusted_proxy_count);
    let max_sessions_per_user = SecuritySettings::clamp_max_sessions(max_sessions_per_user);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        for (key, value) in [
            ("security_app_base_url", app_base_url.clone()),
            ("security_cookie_secure", cookie_secure.to_string()),
            (
                "security_trusted_proxy_count",
                trusted_proxy_count.to_string(),
            ),
            (
                "security_max_sessions_per_user",
                max_sessions_per_user.to_string(),
            ),
        ] {
            client
                .execute(
                    "INSERT INTO settings (key, value, updated_at) VALUES ($1, $2, NOW())
                     ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                    &[&key, &value],
                )
                .await
                .map_err(AppError::query)?;
        }

        invalidate_security_settings();
        tracing::info!(
            "Security settings updated: app_base_url={:?}, cookie_secure={}, proxy_count={}, max_sessions={}",
            app_base_url,
            cookie_secure,
            trusted_proxy_count,
            max_sessions_per_user
        );
    }

    Ok(SecuritySettings {
        app_base_url,
        cookie_secure,
        trusted_proxy_count,
        max_sessions_per_user,
    })
}
