//! P5 §8.2 资源边界：增强脚本/样式的按需注入。
//!
//! `shell()` 的 `<head>` 全局只保留启动必需的 `yggdrasil-core.js`（阻塞，先于 hydration，
//! 导航协调器契约不变）与首屏样式；文章、图鉴、changelog 的增强代码
//! （`yggdrasil-enhance.js`）与灯箱（`lightbox.js/css`）改由需要的组件各自声明：
//!
//! - SSR 直达：`leptos_meta` 把标签烤进 head 且带 `defer`，先于 WASM hydration 执行，
//!   `bridges::globals` 的调用直接命中；
//! - SPA 首次导航：客户端注入为 async，effect 可能先于脚本执行——迟到的调用由
//!   core 建立的 `__pendingEnhanceCalls` 队列兜底，脚本尾部 flush 补上。
//!
//! `leptos_meta::Script` **不按 `src` 去重**：同页多个
//! 组件声明同一脚本会插入多个同 src 标签，组件卸载再挂载（站内导航）还会重新执行。
//! 两个脚本因此都是幂等 IIFE——入口检测到 `window.__init*` 已安装就保留首次实例，
//! 只补一次 `__flushEnhanceCalls`（源码见 `libs/yggdrasil-core/src/enhance.ts`、
//! `libs/lightbox/src/index.ts`）。地址的版本号按文件内容生成，见 [`crate::static_assets`]。
//!
//! 组件为 `pub(crate)`：纯 head 注入的内部管线，无可视预览，不进图鉴目录
//! （目录测试只扫描 `pub fn` 组件；同类先例见 theme.rs 的 ThemePreload）。

use leptos::prelude::*;
use leptos_meta::{Script, Stylesheet};

use crate::static_assets::{self, ENHANCE_JS, HIGHLIGHT_CSS, KATEX_CSS, LIGHTBOX_CSS, LIGHTBOX_JS};

const HIGHLIGHT_MARKER: &str = "<!--ygg-highlight-->";
const KATEX_MARKER: &str = "<!--ygg-katex-->";

#[derive(Clone, Copy, Default)]
pub(crate) struct MarkdownStyles {
    pub(crate) highlight: bool,
    pub(crate) katex: bool,
}

impl MarkdownStyles {
    pub(crate) fn for_html(html: &str) -> Self {
        Self {
            highlight: html.contains("<pre>")
                || html.contains("<pre ")
                || html.contains(HIGHLIGHT_MARKER),
            katex: html.contains(r#"class="katex"#) || html.contains(KATEX_MARKER),
        }
    }

    /// 正文 HTML 已由 SSR 输出，水合数据只记录它需要的样式。
    pub(crate) fn append_markers(self, html: &mut String) {
        if self.highlight {
            html.push_str(HIGHLIGHT_MARKER);
        }
        if self.katex {
            html.push_str(KATEX_MARKER);
        }
    }

    pub(crate) fn links(self) -> impl IntoView {
        view! {
            {self.highlight.then(highlight_styles)}
            {self.katex.then(|| view! { <Stylesheet href=static_assets::url(KATEX_CSS)/> })}
        }
    }
}

pub(crate) fn highlight_styles() -> impl IntoView {
    view! { <Stylesheet href=static_assets::url(HIGHLIGHT_CSS)/> }
}

/// 声明本页需要增强脚本。挂载 PostContent / 目录 / changelog / 图鉴等增强组件的
/// 页面调用；重复声明无害（脚本是幂等 IIFE，见模块文档）。
#[component]
pub(crate) fn EnhanceScript() -> impl IntoView {
    view! { <Script src=static_assets::url(ENHANCE_JS) defer="true"/> }
}

/// 声明本页需要灯箱脚本与样式。点击放大在增强脚本就绪后仍可用
/// （未加载时调用入队，脚本尾部补执行），卸载清理由 `__closeLightboxFor` 承担。
#[component]
pub(crate) fn LightboxAssets() -> impl IntoView {
    view! {
        <Stylesheet href=static_assets::url(LIGHTBOX_CSS)/>
        <Script src=static_assets::url(LIGHTBOX_JS) defer="true"/>
    }
}
