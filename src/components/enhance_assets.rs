//! P5 §8.2 资源边界：增强脚本/样式的按需注入。
//!
//! `Dioxus.toml` 全局只保留启动必需的 `yggdrasil-core.js`（阻塞，先于 hydration，
//! history 适配器契约不变）与首屏样式；文章、图鉴、changelog 的增强代码
//! （`yggdrasil-enhance.js`）与灯箱（`lightbox.js/css`）改由需要的组件各自声明：
//!
//! - SSR 直达：`document::Script` 烤进 head 且带 `defer`，先于 WASM hydration 执行，
//!   `invoke_optional_global` 直接命中；
//! - SPA 首次导航：客户端注入为 async，effect 可能先于脚本执行——迟到的调用由
//!   core 建立的 `__pendingEnhanceCalls` 队列兜底，脚本尾部 flush 补上。
//!
//! `document::Script` 按 `src` 去重，同页多组件声明同一脚本只会注入一次。
//!
//! 组件为 `pub(crate)`：纯 head 注入的内部管线，无可视预览，不进图鉴目录
//! （目录测试只扫描 `pub fn` 组件；同类先例见 theme.rs 的 ThemePreload）。

use dioxus::prelude::*;

/// 增强脚本（post-content/mermaid/toc/changelog/masonry）的固定 URL。
pub(crate) const ENHANCE_SCRIPT: &str = "/yggdrasil-core/yggdrasil-enhance.js?v=1";

/// 灯箱脚本 URL。flush 钩子加入后内容变化，版本随之提升。
pub(crate) const LIGHTBOX_SCRIPT: &str = "/lightbox/lightbox.js?v=3";

/// 灯箱样式 URL。
pub(crate) const LIGHTBOX_STYLESHEET: &str = "/lightbox/lightbox.css?v=2";

/// 声明本页需要增强脚本。挂载 PostContent / 目录 / changelog / 图鉴等增强组件的
/// 页面调用；重复声明无害（按 src 去重）。
#[component]
pub(crate) fn EnhanceScript() -> Element {
    rsx! {
        document::Script { src: ENHANCE_SCRIPT, defer: true }
    }
}

/// 声明本页需要灯箱脚本与样式。点击放大在增强脚本就绪后仍可用
/// （配置自启动 + 显式调用双保险），卸载清理由 `__closeLightboxFor` 承担。
#[component]
pub(crate) fn LightboxAssets() -> Element {
    rsx! {
        document::Stylesheet { href: LIGHTBOX_STYLESHEET }
        document::Script { src: LIGHTBOX_SCRIPT, defer: true }
    }
}
