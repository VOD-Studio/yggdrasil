//! 文章目录组件
//!
//! 同一个 `nav.toc-sidebar` 在两种宽度下换形态（CSS 媒体查询，断点 1100px）：
//! - 宽屏且位于 `.post-layout` 内：正文右侧吸顶目录，当前小节以 Clay 圆点标示，
//!   列表两端滚动渐隐，底部显示「已读 N%」与进度条。
//! - 窄屏或其他容器：底部悬浮条「目录 · 当前小节 · 百分比」，点开后目录浮起
//!   （767px 以下为底部抽屉），选中小节后收起。手机上页头下方另有 2px 阅读进度线。
//!
//! 两形态共用服务端生成的 `toc_html`（嵌套 `<ul><li><a href="#id">`），只保留一份
//! 列表，scroll-spy 只需维护一处激活态。scroll-spy、当前小节名与阅读进度由
//! yggdrasil-core 的 `__initTocSidebar` 维护；锚点点击由 `initAnchorClick` 平滑滚动。

use leptos::ev::{KeyboardEvent, PointerEvent};
use leptos::prelude::*;

use crate::components::enhance_assets::EnhanceScript;
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
/// - `nav_id`、`content_id`、`scroll_id`：可选的图鉴局部作用域（省略即不传，
///   不要传 `Some(..)`）；正式文章使用默认的页面目录和窗口滚动。
///
/// 组件函数体只运行一次：切文章时由调用方按 slug 重建本组件，
/// 重建会重跑初始化（`__initTocSidebar` 幂等，先 dispose 上一次的 observer 与激活态）。
#[component]
pub fn PostToc(
    #[prop(into)] toc_html: String,
    #[prop(default = "目录")] title: &'static str,
    #[prop(optional)] nav_id: Option<&'static str>,
    #[prop(optional)] content_id: Option<&'static str>,
    #[prop(optional)] scroll_id: Option<&'static str>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let id = nav_id.unwrap_or(DEFAULT_NAV_ID);
    let pill_id = format!("{id}-pill");

    // 挂载后初始化 scroll-spy（yggdrasil-core.js 由 shell 全局注入）。
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        let Some(window) = web_sys::window() else {
            return;
        };
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
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (content_id, scroll_id);

    #[cfg(target_arch = "wasm32")]
    on_cleanup(move || {
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

    // 浮层渲染出来后把焦点移进目录（等一帧：class 变更落到 DOM 后再聚焦）。
    Effect::new(move |_| {
        if open.get() {
            request_animation_frame(move || focus_by_id(id));
        }
    });
    let pill_focus_id = pill_id.clone();
    // 主动关闭（Esc、遮罩、关闭键）时还给悬浮条（restore = true）。
    let close = Callback::new(move |restore: bool| {
        if !open.get_untracked() {
            return;
        }
        open.set(false);
        if restore {
            let pill_id = pill_focus_id.clone();
            request_animation_frame(move || focus_by_id(&pill_id));
        }
    });

    view! {
        // P5 §8.2：toc scroll-spy 增强在 yggdrasil-enhance.js，按需注入。SSR 直达烤进
        // head 带 defer，先于 hydration；SPA 首跳由增强调用队列兜底。
        <EnhanceScript/>

        <div
            class=move || if open.get() { "post-toc is-open" } else { "post-toc" }
            data-toc-root="true"
            on:keydown=move |event: KeyboardEvent| {
                if open.get_untracked() && event.key() == "Escape" {
                    event.prevent_default();
                    close.run(true);
                }
            }
        >
            <div class="toc-progress-line" aria-hidden="true"></div>
            <button
                id=pill_id
                class="toc-pill"
                type="button"
                aria-expanded=move || open.get().to_string()
                aria-controls=id
                on:click=move |_| open.set(true)
            >
                <svg class="toc-pill-ring" viewBox="0 0 22 22" aria-hidden="true">
                    <circle cx="11" cy="11" r="8.5"></circle>
                    <circle class="toc-pill-ring-value" cx="11" cy="11" r="8.5" pathLength="100"></circle>
                </svg>
                <span class="toc-pill-label">
                    <span class="toc-pill-title">{title}</span>
                    <span class="toc-pill-current" data-toc-current="true"></span>
                </span>
                <span class="toc-pill-percent tabular-nums" data-toc-percent="true"></span>
            </button>
            <div class="toc-scrim" aria-hidden="true" on:click=move |_| close.run(true)></div>
            <nav id=id class="toc-sidebar" aria-label=title tabindex="-1">
                <div class="toc-sidebar-head">
                    <span class="toc-sidebar-title">{title}</span>
                    <button
                        class="toc-sidebar-close btn btn-xs btn-icon btn-ghost"
                        type="button"
                        aria-label="关闭目录"
                        on:click=move |_| close.run(true)
                    >
                        <Icon name=IconName::X size=16/>
                    </button>
                </div>
                // 选中小节后收起浮层。锚点点击在捕获阶段被全局处理拦下并停止传播，
                // 收不到 click，这里改听 pointerup 与 Enter；浮层的 visibility 延迟 150ms 才隐藏，
                // 随后的 click 仍落在链接上。
                <div
                    class="toc-sidebar-body scroll-fade-y"
                    on:pointerup=move |_: PointerEvent| close.run(false)
                    on:keydown=move |event: KeyboardEvent| {
                        if event.key() == "Enter" {
                            close.run(false);
                        }
                    }
                    inner_html=toc_html
                ></div>
                <div class="toc-sidebar-progress">
                    <div class="toc-sidebar-progress-text">
                        <span>"已读"</span>
                        <span class="tabular-nums" data-toc-percent="true"></span>
                    </div>
                    <div class="toc-sidebar-progress-track" aria-hidden="true">
                        <div class="toc-sidebar-progress-bar"></div>
                    </div>
                </div>
            </nav>
        </div>
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
