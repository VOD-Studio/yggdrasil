// 与 posts 模块一致：Leptos `#[server]` 宏触发 deprecated/unit 提示，按项目惯例放行。
#![allow(clippy::unused_unit, deprecated)]

use crate::api::error::ApiError;
use leptos::prelude::*;

#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;
use crate::models::settings::SsrCacheSettings;

// ============================================================================
// SSR 页面缓存配置（需重启生效）
// ============================================================================

/// 启动时用 `SSR_CACHE_MAX_PAGE_BYTES` 环境变量播种 SSR 页面缓存配置。
#[cfg(feature = "server")]
pub(crate) async fn seed_ssr_cache_settings_from_env(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    let mut seeds: Vec<(&'static str, String)> = Vec::new();

    if let Ok(v) = std::env::var("SSR_CACHE_MAX_PAGE_BYTES") {
        match v.trim().parse::<u64>() {
            Ok(n) => seeds.push((
                "ssr_cache_max_page_bytes",
                SsrCacheSettings::clamp_max_page_bytes(n).to_string(),
            )),
            Err(_) => tracing::warn!("SSR_CACHE_MAX_PAGE_BYTES={v:?} 非法（期望正整数），跳过"),
        }
    }

    super::insert_env_seeds(client, seeds).await
}

/// 从 settings 表读取 SSR 页面缓存配置（缺键回退默认值）。
#[cfg(feature = "server")]
pub(crate) async fn load_ssr_cache_settings(
    client: &tokio_postgres::Client,
) -> Result<SsrCacheSettings, AppError> {
    let values = super::load_setting_values(client, &["ssr_cache_max_page_bytes"]).await?;
    let max_page_bytes = values
        .get("ssr_cache_max_page_bytes")
        .and_then(|v| v.parse().ok())
        .map(SsrCacheSettings::clamp_max_page_bytes)
        .unwrap_or(crate::models::settings::DEFAULT_SSR_CACHE_MAX_PAGE_BYTES);

    Ok(SsrCacheSettings { max_page_bytes })
}

/// 读取 SSR 页面缓存配置（面板用）。
#[server(GetSsrCacheSettings, endpoint = "get_ssr_cache_settings", input = leptos::server_fn::codec::Json)]
pub async fn get_ssr_cache_settings() -> Result<SsrCacheSettings, ApiError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let s = load_ssr_cache_settings(&client)
            .await
            .map_err(<ApiError>::from)?;
        Ok(s)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(SsrCacheSettings::default())
    }
}

/// 更新 SSR 页面缓存配置。
///
/// 字段会被 clamp 后写入 DB。HTML 缓存首次使用时读取，修改后需**重启进程**生效。
#[server(UpdateSsrCacheSettings, endpoint = "update_ssr_cache_settings", input = leptos::server_fn::codec::Json)]
pub async fn update_ssr_cache_settings(max_page_bytes: u64) -> Result<SsrCacheSettings, ApiError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let max_page_bytes = SsrCacheSettings::clamp_max_page_bytes(max_page_bytes);
        let client = get_conn().await.map_err(AppError::db_conn)?;
        super::upsert_setting(
            &**client,
            "ssr_cache_max_page_bytes",
            &max_page_bytes.to_string(),
        )
        .await?;

        tracing::info!("SSR cache settings updated (需重启生效): max_page_bytes={max_page_bytes}");
        Ok(SsrCacheSettings { max_page_bytes })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(SsrCacheSettings { max_page_bytes })
    }
}
