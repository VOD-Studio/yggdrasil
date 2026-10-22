//! 文章目录组件
//!
//! 同一个 `nav.toc-sidebar` 在两种宽度下换形态（CSS 媒体查询，断点 1100px）：
//! - 宽屏且位于 `.post-layout` 内：正文右侧吸顶目录，当前小节以 Clay 圆点标示，
//!   列表两端滚动渐隐，底部显示「已读 N%」与进度条。
//! - 窄屏或其他容器：底部悬浮条「目录 · 当前小节 · 百分比」，点开后目录浮起
//!   （760px 以下为底部抽屉），选中小节后收起。手机上页头下方另有 2px 阅读进度线。
//!
//! 两形态共用服务端生成的 `toc_html`（嵌套 `<ul><li><a href="#id">`），只保留一份
//! 列表，scroll-spy 只需维护一处激活态。scroll-spy、当前小节名与阅读进度由
//! yggdrasil-core 的 `__initTocSidebar` 维护；锚点点击由 `initAnchorClick` 平滑滚动。

use dioxus::prelude::*;

use crate::components::ui::{Icon, IconName};
#[cfg(target_arch = "wasm32")]
use crate::utils::js::invoke_optional_global;

/// 正式页面目录的默认 id；图鉴预览传入各自的局部 id。
const DEFAULT_NAV_ID: &str = "post-toc";

/// 文章目录（Table of Contents）组件。
///
/// Props：
/// - `toc_html`：服务端生成的目录 HTML 字符串
/// - `title`：目录标题，默认「目录」
/// - `nav_id`、`content_id`、`scroll_id`：可选的图鉴局部作用域；
///   正式文章使用默认的页面目录和窗口滚动。
#[component]
pub fn PostToc(
    toc_html: String,
    #[props(default = "目录")] title: &'static str,
    #[props(default)] nav_id: Option<&'static str>,
    #[props(default)] content_id: Option<&'static str>,
    #[props(default)] scroll_id: Option<&'static str>,
) -> Element {
    let mut open = use_signal(|| false);
    let id = nav_id.unwrap_or(DEFAULT_NAV_ID);
    let pill_id = format!("{id}-pill");

    // 挂载后初始化 scroll-spy（yggdrasil-core.js 由 Dioxus.toml 全局注入）。
    // 调用方以 slug 为 key 的单元素列表包裹本组件，切文章时 remount → effect 重跑；
    // __initTocSidebar 内部幂等（先 dispose 上一次的 observer 与激活态）。
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let window =
            web_sys::window().expect("post_toc use_effect 仅在 WASM 浏览器上下文执行：无 window");
        if let (Some(nav_id), Some(content_id), Some(scroll_id)) = (nav_id, content_id, scroll_id) {
            invoke_optional_global(
                &window,
                "__initTocSidebar",
                &[
                    format!("#{nav_id}").into(),
                    format!("#{content_id}").into(),
                    format!("#{scroll_id}").into(),
                ],
            );
        } else {
            invoke_optional_global(&window, "__initTocSidebar", &[]);
        }
    });

    #[cfg(target_arch = "wasm32")]
    use_drop(move || {
        if let Some(window) = web_sys::window() {
            if let Some(nav_id) = nav_id {
                invoke_optional_global(
                    &window,
                    "__disposeTocSidebar",
                    &[format!("#{nav_id}").into()],
                );
            } else {
                invoke_optional_global(&window, "__disposeTocSidebar", &[]);
            }
        }
    });

    // 浮层渲染出来后把焦点移进目录；主动关闭（Esc、遮罩、关闭键）时还给悬浮条。
    use_effect(move || {
        if open() {
            focus_by_id(id);
        }
    });
    let pill_focus_id = pill_id.clone();
    let close = use_callback(move |restore: bool| {
        if !open() {
            return;
        }
        open.set(false);
        if restore {
            focus_by_id(&pill_focus_id);
        }
    });

    rsx! {
        // P5 §8.2：toc scroll-spy 增强在 yggdrasil-enhance.js，按需注入。SSR 直达烤进
        // head 带 defer，先于 hydration；SPA 首跳由增强调用队列兜底，按 src 去重。
        crate::components::enhance_assets::EnhanceScript {}

        div {
            class: if open() { "post-toc is-open" } else { "post-toc" },
            "data-toc-root": "true",
            onkeydown: move |event| {
                if open() && event.key() == Key::Escape {
                    event.prevent_default();
                    close.call(true);
                }
            },
            div { class: "toc-progress-line", aria_hidden: "true" }
            button {
                id: "{pill_id}",
                class: "toc-pill",
                r#type: "button",
                aria_expanded: "{open}",
                aria_controls: "{id}",
                onclick: move |_| open.set(true),
                svg { class: "toc-pill-ring", view_box: "0 0 22 22", "aria-hidden": "true",
                    circle { cx: "11", cy: "11", r: "8.5" }
                    circle { class: "toc-pill-ring-value", cx: "11", cy: "11", r: "8.5", "pathLength": "100" }
                }
                span { class: "toc-pill-label",
                    span { class: "toc-pill-title", "{title}" }
                    span { class: "toc-pill-current", "data-toc-current": "true" }
                }
                span { class: "toc-pill-percent tabular-nums", "data-toc-percent": "true" }
            }
            div { class: "toc-scrim", aria_hidden: "true", onclick: move |_| close.call(true) }
            nav { id, class: "toc-sidebar", aria_label: "{title}", tabindex: "-1",
                div { class: "toc-sidebar-head",
                    span { class: "toc-sidebar-title", "{title}" }
                    button {
                        class: "toc-sidebar-close btn btn-xs btn-icon btn-ghost",
                        r#type: "button",
                        aria_label: "关闭目录",
                        onclick: move |_| close.call(true),
                        Icon { name: IconName::X, size: 16 }
                    }
                }
                // 选中小节后收起浮层。锚点点击在 window 捕获阶段被全局处理拦下并停止传播，
                // 收不到 click，这里改听 pointerup 与 Enter；浮层的 visibility 延迟 150ms 才隐藏，
                // 随后的 click 仍落在链接上。
                div {
                    class: "toc-sidebar-body scroll-fade-y",
                    onpointerup: move |_| close.call(false),
                    onkeydown: move |event| {
                        if event.key() == Key::Enter {
                            close.call(false);
                        }
                    },
                    dangerous_inner_html: "{toc_html}",
                }
                div { class: "toc-sidebar-progress",
                    div { class: "toc-sidebar-progress-text",
                        span { "已读" }
                        span { class: "tabular-nums", "data-toc-percent": "true" }
                    }
                    div { class: "toc-sidebar-progress-track", aria_hidden: "true",
                        div { class: "toc-sidebar-progress-bar" }
                    }
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn focus_by_id(id: &str) {
    use wasm_bindgen::JsCast;
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.focus();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn focus_by_id(_id: &str) {}
