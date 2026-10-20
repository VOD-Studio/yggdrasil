//! 分享元信息：`description`、Open Graph 与 Twitter 卡片。
//!
//! 只有服务端渲染的 HTML 会被社交平台读取。站点地址、简介与站点图地址由中间件放进
//! 请求扩展（[`ShareContext`]），服务端渲染时同步读取，不另发请求；浏览器端取不到，
//! 用空值渲染同样数量的标签，保持与服务端一致的 head 元素顺序（水合按顺序对齐）。
//!
//! 组件只写 `<head>`、没有可见内容，不进组件图鉴，故为 `pub(crate)`。

use dioxus::prelude::*;

use crate::models::post::Post;
use crate::models::share::PostShareCard;

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

fn use_share_context() -> ShareContext {
    use_hook(|| {
        #[cfg(feature = "server")]
        {
            dioxus::fullstack::FullstackContext::current()
                .and_then(|ctx| ctx.extension::<ShareContext>())
                .unwrap_or_default()
        }
        #[cfg(not(feature = "server"))]
        {
            ShareContext::default()
        }
    })
}

/// 除文章页外的前台页面：站点描述与站点分享图。
#[component]
pub(crate) fn SiteShareMeta(path: String) -> Element {
    let ctx = use_share_context();
    rsx! {
        ShareTags {
            kind: "website",
            title: "Yggdrasil",
            description: ctx.description.clone(),
            url: format!("{}{path}", ctx.base_url),
            image: format!("{}{}", ctx.base_url, ctx.site_image),
            image_alt: "Yggdrasil",
        }
    }
}

/// 文章页：标题、摘要、文章分享图与发布时间、标签。
#[component]
pub(crate) fn PostShareMeta(post: Post) -> Element {
    let ctx = use_share_context();
    let card = PostShareCard::from_post(&post);
    let description = if card.summary.is_empty() {
        ctx.description.clone()
    } else {
        card.summary.clone()
    };
    let published = post.published_at.unwrap_or(post.created_at).to_rfc3339();

    rsx! {
        ShareTags {
            kind: "article",
            title: card.title.clone(),
            description,
            url: format!("{}/post/{}", ctx.base_url, post.slug),
            image: format!("{}{}", ctx.base_url, card.image_path(&post.slug)),
            image_alt: card.title.clone(),
        }
        document::Meta { property: "article:published_time", content: published }
        for tag in post.tags.iter() {
            document::Meta { key: "{tag}", property: "article:tag", content: tag.clone() }
        }
    }
}

#[component]
fn ShareTags(
    kind: &'static str,
    title: String,
    description: String,
    url: String,
    image: String,
    image_alt: String,
) -> Element {
    rsx! {
        document::Meta { name: "description", content: description.clone() }
        document::Meta { property: "og:site_name", content: "Yggdrasil" }
        document::Meta { property: "og:locale", content: "zh_CN" }
        document::Meta { property: "og:type", content: kind }
        document::Meta { property: "og:title", content: title }
        document::Meta { property: "og:description", content: description }
        document::Meta { property: "og:url", content: url }
        document::Meta { property: "og:image", content: image }
        document::Meta { property: "og:image:type", content: "image/png" }
        document::Meta { property: "og:image:width", content: "1200" }
        document::Meta { property: "og:image:height", content: "630" }
        document::Meta { property: "og:image:alt", content: image_alt }
        document::Meta { name: "twitter:card", content: "summary_large_image" }
    }
}
