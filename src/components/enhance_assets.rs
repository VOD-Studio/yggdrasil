//! P5 §8.2 资源边界：增强脚本/样式的按需注入。
//!
//! `shell()` 的 `<head>` 全局只保留启动必需的 `yggdrasil-core.js`（阻塞，先于 hydration，
//! 导航协调器契约不变）与首屏样式；文章、图鉴、changelog 的增强代码
//! （`yggdrasil-enhance.js`）与灯箱（`lightbox.js/css`）改由需要的组件各自声明：
//!
//! - SSR 直达：`leptos_meta` 把标签烤进 head 且带 `defer`，先于 WASM hydration 执行，
//!   `invoke_optional_global` 直接命中；
//! - SPA 首次导航：客户端注入为 async，effect 可能先于脚本执行——迟到的调用由
//!   core 建立的 `__pendingEnhanceCalls` 队列兜底，脚本尾部 flush 补上。
//!
//! 与 Dioxus `document::Script` 不同，`leptos_meta::Script` **不按 `src` 去重**：同页多个
//! 组件声明同一脚本会插入多个同 src 标签，组件卸载再挂载（站内导航）还会重新执行。
//! 两个脚本因此都是幂等 IIFE——入口检测到 `window.__init*` 已安装就保留首次实例，
//! 只补一次 `__flushEnhanceCalls`（源码见 `libs/yggdrasil-core/src/enhance.ts`、
//! `libs/lightbox/src/index.ts`）。地址的版本号按文件内容生成，见 [`crate::static_assets`]。
//!
//! 组件为 `pub(crate)`：纯 head 注入的内部管线，无可视预览，不进图鉴目录
//! （目录测试只扫描 `pub fn` 组件；同类先例见 theme.rs 的 ThemePreload）。

// 消费者（post/*、comments、write/cover、图鉴）在 Phase 2B/3/4 移植，在此之前本模块无调用方；
// 它们全部落地后删除本 allow。
#![allow(dead_code)]

use leptos::prelude::*;
use leptos_meta::{Script, Stylesheet};

use crate::static_assets::{self, ENHANCE_JS, LIGHTBOX_CSS, LIGHTBOX_JS};

/// 声明本页需要增强脚本。挂载 PostContent / 目录 / changelog / 图鉴等增强组件的
/// 页面调用；重复声明无害（脚本是幂等 IIFE，见模块文档）。
#[component]
pub(crate) fn EnhanceScript() -> impl IntoView {
    view! { <Script src=static_assets::url(ENHANCE_JS) defer="true"/> }
}

/// 声明本页需要灯箱脚本与样式。点击放大在增强脚本就绪后仍可用
/// （配置自启动 + 显式调用双保险），卸载清理由 `__closeLightboxFor` 承担。
#[component]
pub(crate) fn LightboxAssets() -> impl IntoView {
    view! {
        <Stylesheet href=static_assets::url(LIGHTBOX_CSS)/>
        <Script src=static_assets::url(LIGHTBOX_JS) defer="true"/>
    }
}
