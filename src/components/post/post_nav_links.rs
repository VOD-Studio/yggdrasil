//! 文章上一篇/下一篇导航组件
//!
//! 在文章详情页底部提供相邻文章的快速跳转。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::components::ui::{Icon, IconName};
use crate::models::post::PostNav;
use crate::router::Route;

/// 文章相邻导航组件。
///
/// Props：
/// - `prev`：上一篇文章摘要
/// - `next`：下一篇文章摘要
/// - `full_reload`：链接走整页加载而非客户端路由（默认 false）。仅当本组件
///   渲染在 admin 布局内（如 `/admin/preview`）时传 true：跨 layout 分支的
///   客户端导航会触发 dioxus 0.7.10 的 suspense 卸载双重回收 bug（详见
///   `src/pages/admin/preview.rs` 模块文档），整页加载可完全规避。
/// - `sample`：图鉴使用不可导航的样例锚点，默认 false。
///
/// 两张并排卡片，下一篇右对齐；窄容器中纵向排列。
/// 无相邻文章时不渲染对应链接，两侧都缺失时不渲染导航。
#[component]
pub fn PostNavLinks(
    prev: Option<PostNav>,
    next: Option<PostNav>,
    #[props(default = false)] full_reload: bool,
    #[props(default = false)] sample: bool,
) -> Element {
    if prev.is_none() && next.is_none() {
        return rsx! {};
    }

    // NavigationTarget::External 生成原生 <a href>（Link 不接管 preventDefault），
    // 浏览器整页加载；内部路由变体保持客户端导航。
    let prev_to = prev.as_ref().map(|p| {
        super::nav_target(
            Route::PostDetail {
                slug: p.slug.clone(),
            },
            full_reload,
        )
    });
    let next_to = next.as_ref().map(|p| {
        super::nav_target(
            Route::PostDetail {
                slug: p.slug.clone(),
            },
            full_reload,
        )
    });
    rsx! {
        nav { class: "post-nav", aria_label: "上一篇与下一篇",
            if let (Some(prev_post), Some(prev_to)) = (prev, prev_to) {
                if sample {
                    a { class: "post-nav-card prev", href: "#", onclick: move |event| event.prevent_default(),
                        PostNavLabel { prev: true }
                        span { class: "post-nav-title", "{prev_post.title}" }
                    }
                } else {
                    Link { class: "post-nav-card prev", to: prev_to,
                        PostNavLabel { prev: true }
                        span { class: "post-nav-title", "{prev_post.title}" }
                    }
                }
            }
            if let (Some(next_post), Some(next_to)) = (next, next_to) {
                if sample {
                    a { class: "post-nav-card next", href: "#", onclick: move |event| event.prevent_default(),
                        PostNavLabel { prev: false }
                        span { class: "post-nav-title", "{next_post.title}" }
                    }
                } else {
                    Link { class: "post-nav-card next", to: next_to,
                        PostNavLabel { prev: false }
                        span { class: "post-nav-title", "{next_post.title}" }
                    }
                }
            }
        }
    }
}

#[component]
fn PostNavLabel(prev: bool) -> Element {
    rsx! {
        span { class: "post-nav-label",
            if prev {
                Icon { name: IconName::ArrowLeft, size: 13 }
                "上一篇"
            } else {
                "下一篇"
                Icon { name: IconName::ArrowRight, size: 13 }
            }
        }
    }
}
