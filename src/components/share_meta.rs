//! 分享元信息：`description`、Open Graph 与 Twitter 卡片。
//!
//! 只有服务端渲染的 HTML 会被社交平台读取。站点地址、简介与站点图地址由中间件放进
//! 请求扩展（[`crate::models::share::ShareContext`]），服务端渲染时同步读取，
//! 不另发请求；浏览器端取不到，
//! 用空值渲染同样数量的标签，保持与服务端一致的 head 元素顺序（水合按顺序对齐）。
//!
//! 组件只写 `<head>`、没有可见内容，不进组件图鉴，故为 `pub(crate)`。

// `PostShareMeta` 的调用方 pages/post_detail 在 Phase 3 移植，在此之前无调用方
// （props 结构体的字段会报 never read，item 级 allow 盖不住宏生成的结构体）；落地后删除本 allow。
#![allow(dead_code)]

use leptos::prelude::*;
use leptos::text_prop::TextProp;
use leptos_meta::Meta;

use crate::models::post::Post;
use crate::models::share::{PostShareCard, ShareContext};

/// 读取当前请求的分享上下文；浏览器端与缺少中间件时返回空值。
fn use_share_context() -> ShareContext {
    #[cfg(feature = "server")]
    {
        crate::request_ctx::request_extension::<ShareContext>().unwrap_or_default()
    }
    #[cfg(not(feature = "server"))]
    {
        ShareContext::default()
    }
}

/// 除文章页外的前台页面：站点描述与站点分享图。
///
/// 前台布局常驻，`path` 随导航变化，故为响应式（传当前显示页面的路径加查询串）。
#[component]
pub(crate) fn SiteShareMeta(#[prop(into)] path: Signal<String>) -> impl IntoView {
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

/// 文章页：标题、摘要、文章分享图与发布时间、标签。
#[component]
pub(crate) fn PostShareMeta(post: Post) -> impl IntoView {
    let ctx = use_share_context();
    let card = PostShareCard::from_post(&post);
    let description = if card.summary.is_empty() {
        ctx.description.clone()
    } else {
        card.summary.clone()
    };
    let published = post.published_at.unwrap_or(post.created_at).to_rfc3339();
    let url = format!("{}/post/{}", ctx.base_url, post.slug);
    let image = format!("{}{}", ctx.base_url, card.image_path(&post.slug));
    let tags = post
        .tags
        .iter()
        .map(|tag| view! { <Meta property="article:tag" content=tag.clone()/> })
        .collect_view();

    view! {
        <ShareTags
            kind="article"
            title=card.title.clone()
            description=description
            url=url
            image=image
            image_alt=card.title.clone()
        />
        <Meta property="article:published_time" content=published/>
        {tags}
    }
}

/// 一组分享标签：description、Open Graph 与 Twitter 卡片（顺序固定，水合按顺序对齐）。
#[component]
fn ShareTags(
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
