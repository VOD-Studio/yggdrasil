//! 文章上一篇/下一篇导航组件
//!
//! 在文章详情页底部提供相邻文章的快速跳转。

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::components::ui::{Icon, IconName};
use crate::models::post::PostNav;
use crate::routes::Route;

/// 文章相邻导航组件。
///
/// Props：
/// - `prev`：上一篇文章摘要
/// - `next`：下一篇文章摘要
/// - `sample`：图鉴使用不可导航的样例锚点，默认 false。
///
/// 两张并排卡片，下一篇右对齐；窄容器中纵向排列。
/// 无相邻文章时不渲染对应链接，两侧都缺失时不渲染导航。
#[component]
pub fn PostNavLinks(
    #[prop(into)] prev: Option<PostNav>,
    #[prop(into)] next: Option<PostNav>,
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
        let href = if sample {
            "#".to_string()
        } else {
            Route::PostDetail { slug: post.slug }.to_string()
        };
        view! {
            <a
                class=class
                href=href
                on:click=move |event: MouseEvent| {
                    if sample {
                        event.prevent_default();
                    }
                }
            >
                <PostNavLabel prev=is_prev/>
                <span class="post-nav-title">{post.title}</span>
            </a>
        }
    };
    view! {
        <nav class="post-nav" aria-label="上一篇与下一篇">
            {prev.map(|post| card(post, true))}
            {next.map(|post| card(post, false))}
        </nav>
    }
    .into_any()
}

#[component]
fn PostNavLabel(prev: bool) -> impl IntoView {
    view! {
        <span class="post-nav-label">
            {if prev {
                view! {
                    <Icon name=IconName::ArrowLeft size=13/>
                    "上一篇"
                }
                    .into_any()
            } else {
                view! {
                    "下一篇"
                    <Icon name=IconName::ArrowRight size=13/>
                }
                    .into_any()
            }}
        </span>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptos::reactive::owner::Owner;

    fn nav(title: &str, slug: &str) -> PostNav {
        PostNav {
            title: title.into(),
            slug: slug.into(),
        }
    }

    #[test]
    fn renders_links_to_neighbours_only() {
        let html = Owner::new().with(|| {
            view! { <PostNavLinks prev=None next=Some(nav("下一篇标题", "next-one"))/> }.to_html()
        });
        assert!(html.contains(r#"href="/post/next-one""#), "{html}");
        assert!(html.contains("post-nav-card next"), "{html}");
        assert!(!html.contains("post-nav-card prev"), "{html}");
    }

    #[test]
    fn sample_mode_uses_inert_anchors_and_empty_nav_renders_nothing() {
        let sample = Owner::new().with(|| {
            view! { <PostNavLinks prev=Some(nav("上", "a")) next=None sample=true/> }.to_html()
        });
        assert!(sample.contains(r##"href="#""##), "{sample}");
        assert!(!sample.contains("/post/a"), "{sample}");
        let empty = Owner::new().with(|| view! { <PostNavLinks prev=None next=None/> }.to_html());
        assert!(!empty.contains("post-nav"), "{empty}");
    }
}
