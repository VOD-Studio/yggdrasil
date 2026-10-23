//! 分享图端点与分享元信息上下文（server-only）。
//!
//! - `GET /og/site.png`：站点图，标语读站点设置，底部为站点地址与已发布篇数；
//! - `GET /og/post/{slug}.png`：文章图，标题、摘要与「日期·时长·标签」。
//!
//! PNG 按展示内容缓存（moka，同一内容并发请求只渲染一次），渲染放进
//! `spawn_blocking` 并限制同时渲染数。文章不存在或渲染失败时回退为站点图；站点
//! 标语缺字时改用默认标语，保证总能返回一张图。地址里的 `?v=` 只用于让外部缓存
//! 换图，服务端不读取。
#![cfg(feature = "server")]

use std::hash::{Hash, Hasher};
use std::sync::LazyLock;
use std::time::Duration;

use axum::extract::Path;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use crate::models::settings::{SiteSettings, DEFAULT_HOME_SUBTITLE, DEFAULT_HOME_TITLE};
use crate::models::share::{site_image_path, PostShareCard, ShareContext, SEPARATOR};
use crate::og::{render_post, render_site, OgError, SiteShareCard};

/// 内容版本化的地址可以缓存久一些；回退图很快就该重试。
const CACHE_CONTROL: &str = "public, max-age=86400";
const FALLBACK_CACHE_CONTROL: &str = "public, max-age=300";

#[derive(Hash)]
enum Card {
    Post(PostShareCard),
    Site(SiteShareCard),
}

static IMAGES: LazyLock<moka::future::Cache<u64, Bytes>> = LazyLock::new(|| {
    moka::future::Cache::builder()
        .max_capacity(16 * 1024 * 1024)
        .weigher(|_, png: &Bytes| png.len() as u32)
        .time_to_live(Duration::from_secs(24 * 60 * 60))
        .build()
});

/// 渲染吃满一个核约几十毫秒，同时最多两张，避免挤占请求线程。
static RENDER_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

pub async fn site_image() -> Response {
    match site_png().await {
        Ok(png) => png_response(png, CACHE_CONTROL),
        Err(e) => {
            tracing::error!("站点分享图渲染失败：{e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn post_image(Path(file): Path<String>) -> Response {
    let Some(slug) = file.strip_suffix(".png") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match post_card(slug).await {
        Some(card) => match cached_render(Card::Post(card)).await {
            Ok(png) => return png_response(png, CACHE_CONTROL),
            Err(e) => tracing::warn!(slug, "文章分享图渲染失败，回退站点图：{e}"),
        },
        None => tracing::debug!(slug, "文章不存在，分享图回退站点图"),
    }
    match site_png().await {
        Ok(png) => png_response(png, FALLBACK_CACHE_CONTROL),
        Err(e) => {
            tracing::error!("站点分享图渲染失败：{e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn post_card(slug: &str) -> Option<PostShareCard> {
    let response = crate::api::posts::get_public_post_by_slug(slug.to_string())
        .await
        .ok()?;
    let post = response.post?.into_display_post();
    Some(PostShareCard::from_post(&post))
}

async fn site_png() -> Result<Bytes, OgError> {
    let settings = crate::api::settings::get_site_settings()
        .await
        .unwrap_or_default();
    let tagline = settings.home_tagline();
    let footer = site_footer().await;
    let card = SiteShareCard {
        title: tagline.title,
        subtitle: tagline.subtitle,
        footer: footer.clone(),
    };
    match cached_render(Card::Site(card)).await {
        Err(OgError::MissingGlyph(c)) => {
            tracing::warn!("站点标语含分享图字体没有的字 {c:?}，改用默认标语");
            cached_render(Card::Site(SiteShareCard {
                title: DEFAULT_HOME_TITLE.to_string(),
                subtitle: DEFAULT_HOME_SUBTITLE.to_string(),
                footer,
            }))
            .await
        }
        other => other,
    }
}

/// 「站点地址·N 篇文章」；地址只取 APP_BASE_URL 的主机部分，任一项取不到就省略。
async fn site_footer() -> String {
    let base = base_url().await;
    let host = base
        .split_once("://")
        .map_or(base.as_str(), |(_, rest)| rest)
        .to_string();
    // 与首页第一页同参数，命中同一条文章列表缓存。
    let total = crate::api::posts::list_published_posts(1, 10)
        .await
        .ok()
        .map(|r| format!("{} 篇文章", r.total));
    [Some(host).filter(|h| !h.is_empty()), total]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(SEPARATOR)
}

async fn cached_render(card: Card) -> Result<Bytes, OgError> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    card.hash(&mut hasher);
    IMAGES
        .try_get_with(hasher.finish(), async move {
            let _slot = RENDER_SLOTS.acquire().await.map_err(|_| OgError::Encode)?;
            tokio::task::spawn_blocking(move || match &card {
                Card::Post(card) => render_post(card),
                Card::Site(card) => render_site(card),
            })
            .await
            .map_err(|_| OgError::Encode)?
            .map(Bytes::from)
        })
        .await
        .map_err(|e| (*e).clone())
}

fn png_response(png: Bytes, cache_control: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/png")),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static(cache_control),
            ),
        ],
        png,
    )
        .into_response()
}

/// 站点绝对地址（无尾斜杠），只取 APP_BASE_URL；未配置时为空，页面退回站内相对地址。
async fn base_url() -> String {
    crate::api::settings::runtime_security_settings()
        .await
        .app_base_url
        .trim()
        .trim_end_matches('/')
        .to_string()
}

/// 前台页面服务端渲染时写分享元信息所需的站点信息，由中间件放进请求扩展。
pub async fn share_context() -> ShareContext {
    let settings: SiteSettings = crate::api::settings::get_site_settings()
        .await
        .unwrap_or_default();
    let tagline = settings.home_tagline();
    ShareContext {
        base_url: base_url().await,
        description: settings.description().to_string(),
        site_image: site_image_path(&tagline.title, &tagline.subtitle),
    }
}
