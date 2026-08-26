//! 面包屑组件
//!
//! 在文章详情页展示从首页到当前文章标题的导航路径。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::router::Route;

/// 面包屑导航组件。
///
/// Props：
/// - `title`：当前文章标题
/// - `full_reload`：Home 链接走整页加载而非客户端路由（默认 false）。仅当
///   渲染在 admin 布局内（`/admin/preview`）时传 true——跨 layout 分支的
///   客户端导航会触发 dioxus 0.7.10 的 suspense 卸载双重回收 bug（详见
///   `src/pages/admin/preview.rs` 模块文档），整页加载可完全规避。
///
/// 渲染 `Home > 当前标题` 的面包屑路径。
#[component]
pub fn Breadcrumbs(title: String, #[props(default = false)] full_reload: bool) -> Element {
    let home_to = super::nav_target(Route::Home {}, full_reload);
    rsx! {
        nav {
            class: "breadcrumbs",
            role: "navigation",
            aria_label: "Breadcrumb",
            Link { to: home_to, "Home" }
            svg {
                xmlns: "http://www.w3.org/2000/svg",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                class: "feather feather-chevron-right",
                width: "16",
                height: "16",
                polyline { points: "9 18 15 12 9 6" }
            }
            span { "{title}" }
        }
    }
}
