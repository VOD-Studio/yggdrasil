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
/// **公开接口**（无需登录）：供前台页脚与首页 SSR 读取 GitHub 链接和首页标语。
/// 先查 moka 缓存，未命中查 settings 表，缺失键回退空串（不展示图标 / 使用默认标语）。
#[server(GetSiteSettings, "/api")]
pub async fn get_site_settings() -> Result<SiteSettings, ServerFnError> {
    #[cfg(feature = "server")]
    {
        if let Some(cached) = crate::cache::get_site_settings().await {
            return Ok(cached);
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;

        let rows = client
            .query(
                "SELECT key, value FROM settings WHERE key = ANY($1)",
                &[&SITE_KEYS.as_slice()],
            )
            .await
            .map_err(AppError::query)?;
        let mut settings = SiteSettings::default();
        for row in rows {
            let value: String = row.get("value");
            match row.get::<_, &str>("key") {
                "site_github_url" => settings.github_url = value,
                "site_home_title" => settings.home_title = value,
                "site_home_subtitle" => settings.home_subtitle = value,
                "site_home_intro" => settings.home_intro = value,
                _ => {}
            }
        }
        crate::cache::set_site_settings(settings.clone()).await;
        Ok(settings)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(SiteSettings::default())
    }
}

/// 站点公开配置在 settings 表中的键，顺序与 [`site_values`] 一致。
#[cfg(feature = "server")]
const SITE_KEYS: [&str; 4] = [
    "site_github_url",
    "site_home_title",
    "site_home_subtitle",
    "site_home_intro",
];

#[cfg(feature = "server")]
fn site_values(settings: &SiteSettings) -> [&str; 4] {
    [
        &settings.github_url,
        &settings.home_title,
        &settings.home_subtitle,
        &settings.home_intro,
    ]
}

/// 更新站点公开配置。
///
/// 仅 admin。写入前规范化（GitHub 链接 trim / 截断 / 补 https://，标语合并空白并截断）；
/// 写后失效 moka 站点配置缓存与全部公开页 SSR 缓存（页脚出现在每个前台页面，无法定向失效）。
#[server(UpdateSiteSettings, "/api")]
pub async fn update_site_settings(settings: SiteSettings) -> Result<SiteSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let settings = settings.normalized();

    #[cfg(feature = "server")]
    {
        let mut client = get_conn().await.map_err(AppError::db_conn)?;
        let tx = client.transaction().await.map_err(AppError::tx)?;
        for (key, value) in SITE_KEYS.iter().zip(site_values(&settings)) {
            tx.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ($1, $2, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                &[key, &value],
            )
            .await
            .map_err(AppError::query)?;
        }
        tx.commit().await.map_err(AppError::tx)?;

        crate::cache::invalidate_site_settings();
        crate::ssr_cache::invalidate_ssr_all_public();
        tracing::info!("Site settings updated: github_url={}", settings.github_url);
    }

    Ok(settings)
}
