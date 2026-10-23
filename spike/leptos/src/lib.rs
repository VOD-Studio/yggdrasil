// release 构建不擦除组件类型（cargo-leptos 默认只在 debug 用 erase_components），
// 深层嵌套视图的静态类型会超出默认 128 的查询深度。
#![recursion_limit = "256"]

//! Yggdrasil Leptos 0.8.22 迁移原型。
//!
//! 三条路由全部 lazy：`/`（首页）、`/post/:slug`（文章详情 + 评论）、
//! `/admin/comments`（后台评论审核）。前台布局、页头页脚与主题留在主包。

/// 发布标识（`scripts/release.sh` 通过 `SPIKE_BUILD_ID` 注入），用于滚动发布验证。
pub const BUILD_ID: &str = match option_env!("SPIKE_BUILD_ID") {
    Some(id) => id,
    None => "dev",
};

pub mod api;
pub mod app;
pub mod bridges;
pub mod components;
pub mod error;
pub mod models;
pub mod pages;
pub mod theme;
pub mod utils;

#[cfg(feature = "ssr")]
pub mod server;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_lazy(app::App);
}
