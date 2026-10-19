//! 面包屑组件
//!
//! 文章标题上方的「文章 / 所在栏目」：「文章」回到首页，栏目取文章的第一个标签。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::router::Route;

/// 面包屑导航组件。
///
/// Props：
/// - `tag`：所在栏目（文章的第一个标签），没有时只显示「文章」
/// - `full_reload`：链接走整页加载而非客户端路由（默认 false）。仅当
///   渲染在 admin 布局内（`/admin/preview`）时传 true——跨 layout 分支的
///   客户端导航会触发 dioxus 0.7.10 的 suspense 卸载双重回收 bug（详见
///   `src/pages/admin/preview.rs` 模块文档），整页加载可完全规避。
/// - `sample`：图鉴展示非跳转的样例链接；正式文章默认 false。
#[component]
pub fn Breadcrumbs(
    #[props(default)] tag: Option<String>,
    #[props(default = false)] full_reload: bool,
    #[props(default = false)] sample: bool,
) -> Element {
    let home_to = super::nav_target(Route::Home {}, full_reload);
    rsx! {
        nav { class: "post-crumbs", aria_label: "面包屑",
            if sample {
                a { href: "#", onclick: move |event| event.prevent_default(), "文章" }
            } else {
                Link { to: home_to, "data-vt-return": (!full_reload).then_some("true"), "文章" }
            }
            if let Some(tag) = tag {
                span { class: "post-crumbs-sep", aria_hidden: "true", "/" }
                if sample {
                    a { href: "#", onclick: move |event| event.prevent_default(), "{tag}" }
                } else {
                    Link { to: super::nav_target(Route::TagDetail { tag: tag.clone() }, full_reload), "{tag}" }
                }
            }
        }
    }
}
