//! 文章目录组件
//!
//! 双形态（CSS 媒体查询切换，断点 1200px）：
//! - <1200px（移动端/窄屏）：正文顶部 `<details class="toc">` 折叠块，Alt+C 聚焦。
//! - ≥1200px（桌面端）：右缘悬浮侧边目录——收起为一列层级刻度，scroll-spy 高亮
//!   当前阅读节（`__initTocSidebar`，IntersectionObserver 探测带），鼠标悬浮动画
//!   展开为卡片面板，点 pin 按钮锁定展开。
//!
//! 两形态共用同一份服务端生成的 `toc_html`（嵌套 `<ul><li><a href="#id">`）。
//! 锚点点击由 yggdrasil-core 的 `initAnchorClick` 全局 capture 拦截平滑滚动，
//! 侧边目录链接自动受益。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::utils::js::invoke_optional_global;

/// 文章目录（Table of Contents）组件。
///
/// Props：
/// - `toc_html`：服务端生成的目录 HTML 字符串
/// - `title`：目录标题，文章详情用默认的 "Table of Contents"，
///   更新日志页传入「版本索引」
///
/// 通过 `dangerous_inner_html` 注入目录结构，快捷键 `Alt + C` 可聚焦（移动端形态）。
#[component]
pub fn PostToc(
    toc_html: String,
    #[props(default = "Table of Contents")] title: &'static str,
) -> Element {
    // pin 状态：是否锁定展开（仅本次挂载有效，切文章 remount 后重置）。
    let mut pinned = use_signal(|| false);

    // 挂载后初始化 scroll-spy（yggdrasil-core.js 由 Dioxus.toml 全局注入）。
    // 调用方以 slug 为 key 的单元素列表包裹本组件，切文章时 remount → effect 重跑；
    // __initTocSidebar 内部幂等（先 dispose 上一次的 observer 与激活态）。
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let window =
            web_sys::window().expect("post_toc use_effect 仅在 WASM 浏览器上下文执行：无 window");
        invoke_optional_global(&window, "__initTocSidebar", &[]);
    });

    rsx! {
        // 移动端 / <1200px：顶部折叠块（CSS 在 ≥1200px 隐藏它）。
        details { class: "toc",
            summary { accesskey: "c", title: "(Alt + C)",
                span { class: "title", "{title}" }
            }
            div { class: "inner", dangerous_inner_html: "{toc_html}" }
        }

        // 桌面端 / ≥1200px：右缘悬浮目录（CSS 在 <1200px 隐藏它）。
        // 展开动画纯 CSS（:hover + .pinned），SSR/未 hydration 时已可用；
        // scroll-spy 由 __initTocSidebar 增强。
        nav {
            class: if pinned() { "toc-sidebar pinned" } else { "toc-sidebar" },
            aria_label: "{title}",
            div { class: "toc-sidebar-panel",
                div { class: "toc-sidebar-head",
                    span { class: "toc-sidebar-title", "{title}" }
                    button {
                        class: "toc-sidebar-pin",
                        aria_label: if pinned() { "取消固定目录" } else { "固定目录" },
                        aria_pressed: "{pinned}",
                        onclick: move |_| pinned.set(!pinned()),
                        // lucide "pin" 图标，stroke=currentColor，CSS 控制尺寸 14px
                        svg {
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M12 17v5" }
                            path { d: "M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 8 15.24V17h8v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 12 10.76V6h1a2 2 0 0 0 0-4h-2a2 2 0 0 0 0 4h1z" }
                        }
                    }
                }
                div {
                    class: "toc-sidebar-body",
                    dangerous_inner_html: "{toc_html}",
                }
            }
        }
    }
}
