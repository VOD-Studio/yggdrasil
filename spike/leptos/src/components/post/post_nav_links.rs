//! 文章上一篇/下一篇导航组件
//!
//! 在文章详情页底部提供相邻文章的快速跳转。

use leptos::prelude::*;

use super::{full_reload_rel, post_href};
use crate::components::ui::{Icon, IconName};
use crate::models::post::PostNav;

/// 文章相邻导航组件。
///
/// Props：
/// - `prev`：上一篇文章摘要
/// - `next`：下一篇文章摘要
/// - `full_reload`：链接走整页加载而非客户端路由（默认 false），原版仅 admin
///   预览布局使用。
/// - `sample`：图鉴使用不可导航的样例锚点，默认 false。
///
/// 两张并排卡片，下一篇右对齐；窄容器中纵向排列。
/// 无相邻文章时不渲染对应链接，两侧都缺失时不渲染导航。
#[component]
pub fn PostNavLinks(
    prev: Option<PostNav>,
    next: Option<PostNav>,
    #[prop(optional)] full_reload: bool,
    #[prop(optional)] sample: bool,
) -> impl IntoView {
    if prev.is_none() && next.is_none() {
        return ().into_any();
    }

    let card = move |post: PostNav, is_prev: bool| {
        let class = if is_prev {
            "post-nav-card prev"
        } else {
            "post-nav-card next"
        };
        if sample {
            view! {
                <a class=class href="#" on:click=|event| event.prevent_default()>
                    <PostNavLabel prev=is_prev/>
                    <span class="post-nav-title">{post.title}</span>
                </a>
            }
            .into_any()
        } else {
            let href = post_href(&post.slug);
            view! {
                <a class=class href=href rel=full_reload_rel(full_reload)>
                    <PostNavLabel prev=is_prev/>
                    <span class="post-nav-title">{post.title}</span>
                </a>
            }
            .into_any()
        }
    };

    view! {
        <nav class="post-nav" aria-label="上一篇与下一篇">
            {prev.map(|p| card(p, true))}
            {next.map(|p| card(p, false))}
        </nav>
    }
    .into_any()
}

#[component]
fn PostNavLabel(prev: bool) -> impl IntoView {
    if prev {
        view! {
            <span class="post-nav-label">
                <Icon name=IconName::ArrowLeft size=13/>
                "上一篇"
            </span>
        }
        .into_any()
    } else {
        view! {
            <span class="post-nav-label">
                "下一篇"
                <Icon name=IconName::ArrowRight size=13/>
            </span>
        }
        .into_any()
    }
}
