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
use crate::models::settings::SiteSettings;

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
