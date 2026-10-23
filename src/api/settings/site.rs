// 与 posts 模块一致：Leptos `#[server]` 宏触发 deprecated/unit/too_many_arguments
// 提示，按项目惯例放行（限流/运行器等配置项天然参数多）。
#![allow(clippy::unused_unit, deprecated, clippy::too_many_arguments)]

use leptos::prelude::*;

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
#[server(GetSiteSettings, endpoint = "get_site_settings", input = leptos::server_fn::codec::Json)]
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
                "site_description" => settings.site_description = value,
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

/// 友链页「交换友链」展示的本站信息。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SiteCard {
    pub name: String,
    /// 站点地址（APP_BASE_URL）；未配置时为空，前台不显示地址与订阅。
    pub url: String,
    pub description: String,
}

impl SiteCard {
    /// RSS 订阅地址。
    pub fn feed_url(&self) -> String {
        format!("{}/feed.xml", self.url)
    }
}

/// 读取友链交换信息：站名、站点地址与简介。**公开接口**。
///
/// 地址只取「站点配置 → 安全」里的 APP_BASE_URL，不信任请求 Host。
#[server(GetSiteCard, endpoint = "get_site_card", input = leptos::server_fn::codec::Json)]
pub async fn get_site_card() -> Result<SiteCard, ServerFnError> {
    let settings = get_site_settings().await?;
    #[cfg(feature = "server")]
    let url = crate::api::settings::runtime_security_settings()
        .await
        .app_base_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    #[cfg(not(feature = "server"))]
    let url = String::new();
    Ok(SiteCard {
        name: "Yggdrasil".to_string(),
        url,
        description: settings.description().to_string(),
    })
}

/// 站点公开配置在 settings 表中的键，顺序与 [`site_values`] 一致。
#[cfg(feature = "server")]
const SITE_KEYS: [&str; 5] = [
    "site_github_url",
    "site_home_title",
    "site_home_subtitle",
    "site_home_intro",
    "site_description",
];

#[cfg(feature = "server")]
fn site_values(settings: &SiteSettings) -> [&str; 5] {
    [
        &settings.github_url,
        &settings.home_title,
        &settings.home_subtitle,
        &settings.home_intro,
        &settings.site_description,
    ]
}

/// 更新站点公开配置。
///
/// 仅 admin。写入前规范化（GitHub 链接 trim / 截断 / 补 https://，标语合并空白并截断）；
/// 写后失效 moka 站点配置缓存与全部公开页 SSR 缓存（页脚出现在每个前台页面，无法定向失效）。
#[server(UpdateSiteSettings, endpoint = "update_site_settings", input = leptos::server_fn::codec::Json)]
pub async fn update_site_settings(settings: SiteSettings) -> Result<SiteSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let settings = settings.normalized();

    #[cfg(feature = "server")]
    {
        let mut client = get_conn().await.map_err(AppError::db_conn)?;
        let tx = client.transaction().await.map_err(AppError::tx)?;
        for (key, value) in SITE_KEYS.iter().zip(site_values(&settings)) {
            super::upsert_setting(&*tx, key, value).await?;
        }
        tx.commit().await.map_err(AppError::tx)?;

        crate::cache::invalidate_site_settings();
        crate::ssr_cache::invalidate_ssr_all_public();
        tracing::info!("Site settings updated: github_url={}", settings.github_url);
    }

    Ok(settings)
}
