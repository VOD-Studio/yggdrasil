//! 文章上一篇/下一篇导航组件
//!
//! 在文章详情页底部提供相邻文章的快速跳转。

use dioxus::prelude::*;
use dioxus::router::components::Link;

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
///
/// 左右两侧分别渲染 Prev/Next 链接，若无相邻文章则占位空白。
#[component]
pub fn PostNavLinks(
    prev: Option<PostNav>,
    next: Option<PostNav>,
    #[props(default = false)] full_reload: bool,
) -> Element {
    // NavigationTarget::External 生成原生 <a href>（Link 不接管 preventDefault），
    // 浏览器整页加载；内部路由变体保持客户端导航。
    let prev_to = prev.as_ref().map(|p| {
        if full_reload {
            NavigationTarget::<Route>::External(format!("/post/{}", p.slug))
        } else {
            NavigationTarget::Internal(Route::PostDetail {
                slug: p.slug.clone(),
            })
        }
    });
    let next_to = next.as_ref().map(|p| {
        if full_reload {
            NavigationTarget::<Route>::External(format!("/post/{}", p.slug))
        } else {
            NavigationTarget::Internal(Route::PostDetail {
                slug: p.slug.clone(),
            })
        }
    });
    rsx! {
        nav { class: "paginav",
            if let (Some(prev_post), Some(prev_to)) = (prev, prev_to) {
                Link {
                    class: "prev",
                    to: prev_to,
                    span { class: "title", "« Prev" }
                    span { class: "post-title-nav", "{prev_post.title}" }
                }
            } else {
                span { class: "prev" }
            }

            if let (Some(next_post), Some(next_to)) = (next, next_to) {
                Link {
                    class: "next",
                    to: next_to,
                    span { class: "title", "Next »" }
                    span { class: "post-title-nav", "{next_post.title}" }
                }
            } else {
                span { class: "next" }
            }
        }
    }
}
