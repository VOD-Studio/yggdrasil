//! 分享元信息：`description`、Open Graph 与 Twitter 卡片。
//!
//! 只有服务端渲染的 HTML 会被社交平台读取。站点地址、简介与站点图地址由中间件
//! [`insert_share_context`] 放进请求扩展（[`ShareContext`]），服务端渲染时经
//! leptos_axum 提供的 `http::request::Parts` 同步读取，不另发请求；浏览器端取不到，
//! 用空值渲染同样数量的标签，保持与服务端一致的 head 元素顺序。
//!
//! 文章页的 `PostShareMeta` 在 `crate::components::post::share_meta`，复用这里的
//! [`use_share_context`] 与 [`ShareTags`]。

use leptos::prelude::*;
use leptos::text_prop::TextProp;
use leptos_meta::Meta;

/// 写分享元信息所需的站点信息。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShareContext {
    /// APP_BASE_URL，无尾斜杠；未配置时为空，地址退回站内相对路径。
    pub base_url: String,
    /// 站点简介，页面没有自己的描述时使用。
    pub description: String,
    /// 站点分享图的站内地址（带版本号）。
    pub site_image: String,
}

/// 读取当前请求的分享上下文；浏览器端与缺少中间件时返回空值。
pub fn use_share_context() -> ShareContext {
    #[cfg(feature = "ssr")]
    {
        use_context::<http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<ShareContext>().cloned())
            .unwrap_or_default()
    }
    #[cfg(not(feature = "ssr"))]
    {
        ShareContext::default()
    }
}

/// 除文章页外的前台页面：站点描述与站点分享图。
///
/// 前台布局常驻，`path` 随导航变化，故为响应式。
#[component]
pub fn SiteShareMeta(#[prop(into)] path: Signal<String>) -> impl IntoView {
    let ctx = use_share_context();
    let base_url = ctx.base_url.clone();
    view! {
        <ShareTags
            kind="website"
            title="Yggdrasil"
            description=ctx.description.clone()
            url=move || format!("{base_url}{}", path.get())
            image=format!("{}{}", ctx.base_url, ctx.site_image)
            image_alt="Yggdrasil"
        />
    }
}

/// 一组分享标签：description、Open Graph 与 Twitter 卡片（顺序与 Dioxus 版一致）。
#[component]
pub fn ShareTags(
    kind: &'static str,
    #[prop(into)] title: TextProp,
    #[prop(into)] description: TextProp,
    #[prop(into)] url: TextProp,
    #[prop(into)] image: TextProp,
    #[prop(into)] image_alt: TextProp,
) -> impl IntoView {
    view! {
        <Meta name="description" content=description.clone()/>
        <Meta property="og:site_name" content="Yggdrasil"/>
        <Meta property="og:locale" content="zh_CN"/>
        <Meta property="og:type" content=kind/>
        <Meta property="og:title" content=title/>
        <Meta property="og:description" content=description/>
        <Meta property="og:url" content=url/>
        <Meta property="og:image" content=image/>
        <Meta property="og:image:type" content="image/png"/>
        <Meta property="og:image:width" content="1200"/>
        <Meta property="og:image:height" content="630"/>
        <Meta property="og:image:alt" content=image_alt/>
        <Meta name="twitter:card" content="summary_large_image"/>
    }
}

// ============================================================================
// 服务端：构建 ShareContext 并放进请求扩展
// ============================================================================

/// 首页标题第一行的默认文案（与主 crate `models::settings` 一致）。
#[cfg(feature = "ssr")]
const DEFAULT_HOME_TITLE: &str = "慢慢读源码，";
/// 首页标题第二行的默认文案。
#[cfg(feature = "ssr")]
const DEFAULT_HOME_SUBTITLE: &str = "认真写长文。";
/// 站点简介的默认文案。
#[cfg(feature = "ssr")]
const DEFAULT_SITE_DESCRIPTION: &str = "Rust、Go 与前端工程的源码剖析和长文笔记。";

/// 分享上下文缓存时长：替代主 crate 里站点配置与安全配置的 moka 缓存，
/// 避免每次页面渲染都查库。
#[cfg(feature = "ssr")]
const SHARE_CONTEXT_TTL: std::time::Duration = std::time::Duration::from_secs(60);

#[cfg(feature = "ssr")]
static SHARE_CONTEXT_CACHE: std::sync::Mutex<Option<(std::time::Instant, ShareContext)>> =
    std::sync::Mutex::new(None);

/// 站点分享图地址：文案变化时版本号随之变化，社交平台重新抓取。
#[cfg(feature = "ssr")]
fn site_image_path(title: &str, subtitle: &str) -> String {
    format!("/og/site.png?v={}", version_of(&[title, subtitle]))
}

/// FNV-1a 64 位，取低 32 位作 8 位十六进制版本号。
#[cfg(feature = "ssr")]
fn version_of(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(0)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{:08x}", hash as u32)
}

/// 站点绝对地址（无尾斜杠）：取 settings 表的 `security_app_base_url`，
/// 缺失时回退环境变量 APP_BASE_URL（主 crate 启动时用它播种该键）；都没有时为空。
#[cfg(feature = "ssr")]
async fn base_url() -> String {
    let from_db = match crate::server::db::get_conn().await {
        Ok(conn) => conn
            .query_opt(
                "SELECT value FROM settings WHERE key = 'security_app_base_url'",
                &[],
            )
            .await
            .ok()
            .flatten()
            .map(|row| row.get::<_, String>("value")),
        Err(_) => None,
    };
    from_db
        .filter(|v| !v.trim().is_empty())
        .or_else(|| std::env::var("APP_BASE_URL").ok())
        .unwrap_or_default()
        .trim()
        .trim_end_matches('/')
        .to_string()
}

/// 前台页面服务端渲染时写分享元信息所需的站点信息（对应主 crate `api::og::share_context`）。
#[cfg(feature = "ssr")]
pub async fn share_context() -> ShareContext {
    if let Ok(guard) = SHARE_CONTEXT_CACHE.lock() {
        if let Some((at, ctx)) = guard.as_ref() {
            if at.elapsed() < SHARE_CONTEXT_TTL {
                return ctx.clone();
            }
        }
    }

    let settings = crate::api::home::get_site_settings()
        .await
        .unwrap_or_default();
    let (title, subtitle) = if settings.home_title.is_empty() {
        (DEFAULT_HOME_TITLE, DEFAULT_HOME_SUBTITLE)
    } else {
        (
            settings.home_title.as_str(),
            settings.home_subtitle.as_str(),
        )
    };
    let description = if settings.site_description.is_empty() {
        DEFAULT_SITE_DESCRIPTION.to_string()
    } else {
        settings.site_description.clone()
    };
    let ctx = ShareContext {
        base_url: base_url().await,
        description,
        site_image: site_image_path(title, subtitle),
    };

    if let Ok(mut guard) = SHARE_CONTEXT_CACHE.lock() {
        *guard = Some((std::time::Instant::now(), ctx.clone()));
    }
    ctx
}

/// axum 中间件：为页面请求构建 [`ShareContext`] 并放进请求扩展。
///
/// 只处理 GET 页面请求；server fn（`/api/`）、WASM 包（`/pkg/`）与带扩展名的静态
/// 文件不需要分享信息，直接放行。
#[cfg(feature = "ssr")]
pub async fn insert_share_context(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = req.uri().path();
    let is_page = *req.method() == http::Method::GET
        && !path.starts_with("/api/")
        && !path.starts_with("/pkg/")
        && !path.rsplit('/').next().unwrap_or_default().contains('.');
    if is_page {
        let ctx = share_context().await;
        req.extensions_mut().insert(ctx);
    }
    next.run(req).await
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    #[test]
    fn site_image_path_has_eight_hex_version() {
        assert_eq!(
            site_image_path("慢慢读源码，", "认真写长文。").len(),
            "/og/site.png?v=".len() + 8
        );
    }
}
