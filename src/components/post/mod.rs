//! 文章详情组件模块
//!
//! 提供文章详情页所需的各个子组件：封面、内容、页眉、元信息、页脚、面包屑、
//! 上一篇/下一篇导航与目录。

use dioxus::prelude::*;

use crate::router::Route;

/// 根据 `full_reload` 决定路由式导航目标。
///
/// `true`：整页加载（`NavigationTarget::External`，路径由 `route.to_string()`
/// 生成——`Route` 的 `Routable` 派生自带 `Display`，与 `#[route(..)]` 匹配规则
/// 天然一致，比手写 `format!("/xxx/{}", ..)` 更不容易漏 URL 编码或与路由表脱节）。
/// 绕开跨 layout 分支客户端导航触发的 dioxus 0.7.10 suspense 卸载双重回收 bug
/// （详见 `src/pages/admin/preview.rs` 模块文档）。
/// `false`：客户端路由（`NavigationTarget::Internal`）。
///
/// 供 `breadcrumbs`/`post_footer`/`post_nav_links` 共用，替代此前三处独立手写
/// 的同构 `if full_reload {...} else {...}` 分支（含三份几乎相同的说明注释）。
pub(super) fn nav_target(route: Route, full_reload: bool) -> NavigationTarget<Route> {
    if full_reload {
        NavigationTarget::External(route.to_string())
    } else {
        NavigationTarget::Internal(route)
    }
}

/// 面包屑导航组件。
pub mod breadcrumbs;
/// 文章内容组件。
pub mod post_content;
/// 文章封面组件。
pub mod post_cover;
/// 文章页脚组件。
pub mod post_footer;
/// 文章页眉组件。
pub mod post_header;
/// 文章元信息组件。
pub mod post_meta;
/// 上一篇/下一篇导航组件。
pub mod post_nav_links;
/// 文章目录组件。
pub mod post_toc;
