//! P5 §8.2 资源边界：增强脚本/样式的按需注入（移植主 crate `components/enhance_assets.rs`）。
//!
//! 外壳全局只保留启动必需的 `yggdrasil-core.js` 与首屏样式；文章的增强代码
//! （`yggdrasil-enhance.js`）与灯箱（`lightbox.js/css`）由需要的组件各自声明：
//!
//! - SSR 直达：`leptos_meta` 把标签烤进 head 且带 `defer`，先于 WASM hydration 执行，
//!   `invoke_optional_global` 直接命中；
//! - SPA 首次导航：客户端动态插入的脚本异步执行，effect 可能先于脚本——迟到的调用由
//!   core 建立的 `__pendingEnhanceCalls` 队列兜底，脚本尾部 flush 补上。
//!
//! 与 Dioxus `document::Script` 不同，`leptos_meta` 不按 `src` 去重：同页多处声明
//! 会插入多个同 src 标签。两个脚本都是幂等 IIFE（只重新挂全局函数，灯箱绑定有
//! `data-lb-bound` 守卫），重复执行无副作用；文章页的增强脚本只由 `PostContent`
//! 声明（原版 `PostToc` 也声明一次，靠去重合并）。

use leptos::prelude::*;
use leptos_meta::{Script, Stylesheet};

/// 增强脚本（post-content/mermaid/toc/changelog/masonry）的固定 URL。
pub const ENHANCE_SCRIPT: &str = "/yggdrasil-core/yggdrasil-enhance.js?v=1";

/// 灯箱脚本 URL。flush 钩子加入后内容变化，版本随之提升。
pub const LIGHTBOX_SCRIPT: &str = "/lightbox/lightbox.js?v=3";

/// 灯箱样式 URL。
pub const LIGHTBOX_STYLESHEET: &str = "/lightbox/lightbox.css?v=2";

/// 声明本页需要增强脚本。
#[component]
pub fn EnhanceScript() -> impl IntoView {
    view! { <Script src=ENHANCE_SCRIPT defer="true"/> }
}

/// 声明本页需要灯箱脚本与样式。点击放大在增强脚本就绪后仍可用
/// （配置自启动 + 显式调用双保险），卸载清理由 `__closeLightboxFor` 承担。
#[component]
pub fn LightboxAssets() -> impl IntoView {
    view! {
        <Stylesheet href=LIGHTBOX_STYLESHEET/>
        <Script src=LIGHTBOX_SCRIPT defer="true"/>
    }
}
