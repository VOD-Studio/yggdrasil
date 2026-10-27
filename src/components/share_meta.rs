//! 分享元信息：`description`、Open Graph 与 Twitter 卡片。
//!
//! 只有服务端渲染的 HTML 会被社交平台读取，但客户端导航后挂载的标签也要与直达页面一致。
//! 站点信息由中间件放进请求扩展（[`crate::models::share::ShareContext`]），
//! 再由 [`provide_share_context`](crate::components::share_meta::provide_share_context) 在 App 根部经 `SharedValue` 序列化，浏览器水合时读回。
//! 标签顺序固定，以便水合按顺序对齐。
//!
//! 组件只写 `<head>`、没有可见内容，不进组件图鉴，故为 `pub(crate)`。

use leptos::prelude::*;
use leptos::text_prop::TextProp;
use leptos_meta::Meta;

use crate::models::post::Post;
use crate::models::share::{PostShareCard, ShareContext};

/// 应用根组件调用一次：服务端从请求扩展读取分享上下文并序列化进页面，浏览器水合时读回同一份值。
/// 必须在水合期间创建（App 根部）：水合结束后才创建的 `SharedValue` 会在浏览器端重新求值，只能得到空值。
pub(crate) fn provide_share_context() {
    provide_context(SharedValue::new(share_context_from_request).into_inner());
}

fn share_context_from_request() -> ShareContext {
    #[cfg(feature = "server")]
    {
        crate::request_ctx::request_extension::<ShareContext>().unwrap_or_default()
    }
    #[cfg(not(feature = "server"))]
    {
        ShareContext::default()
    }
}

/// 读取分享上下文；没有上下文时（组件单测）返回空值。
fn use_share_context() -> ShareContext {
    use_context::<ShareContext>().unwrap_or_default()
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
