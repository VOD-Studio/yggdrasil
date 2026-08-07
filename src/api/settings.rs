//! 回收站与站点配置接口。
//!
//! - 回收站配置：读取与更新自动清理设置，需要 admin 权限。
//! - 站点公开配置：页脚 GitHub 链接等，`get_site_settings` 公开读取（前台页脚 SSR），
//!   `update_site_settings` 仅 admin。配置持久化到 settings 键值表。
//! Dioxus server function，注册在 `/api` 路径下。

// 与 posts 模块一致：Dioxus `#[server]` 宏触发 deprecated/unit 提示，按项目惯例放行。
#![allow(clippy::unused_unit, deprecated)]

use dioxus::prelude::*;

#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;
use crate::models::settings::{SiteSettings, TrashSettings};

/// 读取回收站配置。
///
/// settings 表缺失键时回退到默认值，保证向后兼容。
#[server(GetTrashSettings, "/api")]
pub async fn get_trash_settings() -> Result<TrashSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        let enabled: bool = client
            .query_opt(
                "SELECT value FROM settings WHERE key = 'trash_auto_purge_enabled'",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .and_then(|r| r.get::<_, String>("value").parse().ok())
            .unwrap_or(crate::models::settings::DEFAULT_AUTO_PURGE_ENABLED);

        let days: i32 = client
            .query_opt(
                "SELECT value FROM settings WHERE key = 'trash_retention_days'",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .and_then(|r| r.get::<_, String>("value").parse().ok())
            .unwrap_or(crate::models::settings::DEFAULT_RETENTION_DAYS);

        Ok(TrashSettings {
            auto_purge_enabled: enabled,
            retention_days: TrashSettings::clamp_retention(days),
        })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(TrashSettings::default())
    }
}

/// 更新回收站配置。
///
/// retention_days 会被 clamp 到合法范围后写入。
#[server(UpdateTrashSettings, "/api")]
pub async fn update_trash_settings(
    auto_purge_enabled: bool,
    retention_days: i32,
) -> Result<TrashSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let retention_days = TrashSettings::clamp_retention(retention_days);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        // UPSERT 两个键。
        client
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('trash_auto_purge_enabled', $1, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                &[&auto_purge_enabled.to_string()],
            )
            .await
            .map_err(AppError::query)?;

        client
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('trash_retention_days', $1, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                &[&retention_days.to_string()],
            )
            .await
            .map_err(AppError::query)?;

        tracing::info!(
            "Trash settings updated: auto_purge={}, retention_days={}",
            auto_purge_enabled,
            retention_days
        );
    }

    Ok(TrashSettings {
        auto_purge_enabled,
        retention_days,
    })
}

// ============================================================================
// 站点公开配置
// ============================================================================

/// 读取站点公开配置。
///
/// **公开接口**（无需登录）：供前台页脚 SSR 读取 GitHub 链接等。先查 moka 缓存，
/// 未命中查 settings 表，缺失键回退默认值（空串=不展示图标）。
#[server(GetSiteSettings, "/api")]
pub async fn get_site_settings() -> Result<SiteSettings, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::models::settings::DEFAULT_SITE_GITHUB_URL;

        if let Some(cached) = crate::cache::get_site_settings().await {
            return Ok(cached);
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;

        let github_url: String = client
            .query_opt(
                "SELECT value FROM settings WHERE key = 'site_github_url'",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .map(|r| r.get::<_, String>("value"))
            .unwrap_or_else(|| DEFAULT_SITE_GITHUB_URL.to_string());

        let settings = SiteSettings { github_url };
        crate::cache::set_site_settings(settings.clone()).await;
        Ok(settings)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(SiteSettings::default())
    }
}

/// 更新站点公开配置。
///
/// 仅 admin。写入前规范化（trim / 截断 / 补 https://）；写后失效 moka 站点配置缓存
/// 与全部公开页 SSR 缓存（页脚出现在每个前台页面，无法定向失效）。
#[server(UpdateSiteSettings, "/api")]
pub async fn update_site_settings(github_url: String) -> Result<SiteSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let github_url = SiteSettings::normalize_github_url(&github_url);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        client
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('site_github_url', $1, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                &[&github_url],
            )
            .await
            .map_err(AppError::query)?;

        crate::cache::invalidate_site_settings();
        crate::ssr_cache::invalidate_ssr_all_public();
        tracing::info!("Site settings updated: github_url={}", github_url);
    }

    Ok(SiteSettings { github_url })
}
