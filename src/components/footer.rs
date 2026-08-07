//! 页脚组件
//!
//! 提供站点版权信息，并在用户向下滚动超过一屏后显示"回到顶部"悬浮按钮。
//! 回到顶部的滚动监听与平滑滚动逻辑仅在 WASM 前端生效。

use crate::api::settings::get_site_settings;
#[cfg(target_arch = "wasm32")]
use crate::hooks::event_listener::use_event_listener;
use dioxus::prelude::*;

/// 页脚与回到顶部按钮组件。
///
/// Props：无。
/// 关键行为：
/// - 监听窗口滚动，超过一屏时显示回到顶部按钮
/// - 点击按钮平滑滚动到顶部，并清理 URL 中的 `#`
/// - 滚动监听与平滑滚动仅在 `target_arch = "wasm32"` 下执行
#[component]
#[allow(unused_mut)]
pub fn Footer() -> Element {
    let mut visible = use_signal(|| false);

    // 读取站点公开配置（页脚 GitHub 链接）。公开接口，SSR 与 WASM 均可调用；
    // 服务端有 moka 缓存兜底，解析极快。`?` 在 pending 时向上抛 Suspense，
    // 页脚复用前台布局的渲染流程——版权与回到顶部按钮随之短暂延后（通常仅首屏一次）。
    let settings_res = use_server_future(get_site_settings)?;
    let github_url: Option<String> = settings_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|s| s.github_url.clone())
        .filter(|u| !u.is_empty());

    // 根据 window 当前滚动位置同步 visible（注册监听后立即调用一次，避免首屏漏判）。
    // 滚动事件回调里也复用同一份判断逻辑。
    let mut sync_visible = move || {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(w) = web_sys::window() {
                let threshold = w
                    .inner_height()
                    .ok()
                    .and_then(|h| h.as_f64())
                    .unwrap_or(0.0);
                let scroll_y = w.scroll_y().unwrap_or(0.0);
                visible.set(scroll_y > threshold);
            }
        }
    };

    // 注册 scroll 监听：注册 / 卸载清理由 use_event_listener 负责。
    // 仅 WASM 端调用（server 端 use_event_listener 是 noop，但 acquire 闭包内的
    // web_sys 在非 wasm 下不可解析，故整块 cfg；hook 数量在 server build 中不影响，
    // 因为 server 端该组件只跑一次 SSR）。
    #[cfg(target_arch = "wasm32")]
    use_event_listener(
        web_sys::window,
        "scroll",
        // 滚动事件触发时复用同样的阈值判断。
        sync_visible,
    );

    // 挂载时根据当前滚动位置初始化一次按钮可见性。
    use_effect(move || {
        sync_visible();
    });

    // 根据 visible 动态切换按钮显示/隐藏样式
    let btn_class = use_memo(move || {
        let base = "fixed bottom-16 right-8 z-50 w-10 h-10 rounded-full bg-paper-entry border border-paper-border shadow-sm flex items-center justify-center cursor-pointer transition-all duration-300 text-paper-secondary hover:text-paper-accent";
        if visible() {
            format!("{} opacity-100 translate-y-0", base)
        } else {
            format!("{} opacity-0 translate-y-2 pointer-events-none", base)
        }
    });

    rsx! {
        footer { class: "w-full border-t border-paper-border mt-auto",
            div { class: "max-w-4xl mx-auto px-6 py-5 flex items-center justify-between text-sm text-paper-secondary",
                span { "© 2026 Yggdrasil" }
                if let Some(url) = github_url.as_ref() {
                    a {
                        class: "inline-flex items-center justify-center w-8 h-8 rounded-full text-paper-secondary hover:text-paper-primary hover:bg-paper-entry transition-colors",
                        href: "{url}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        aria_label: "GitHub",
                        title: "GitHub",
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "20",
                            height: "20",
                            view_box: "0 0 24 24",
                            fill: "currentColor",
                            path {
                                fill_rule: "evenodd",
                                clip_rule: "evenodd",
                                d: "M12.026 2c-5.509 0-9.974 4.465-9.974 9.974 0 4.406 2.857 8.145 6.821 9.465.499.09.679-.217.679-.481 0-.237-.008-.865-.011-1.696-2.775.602-3.361-1.338-3.361-1.338-.452-1.152-1.107-1.459-1.107-1.459-.905-.619.069-.605.069-.605 1.002.07 1.527 1.028 1.527 1.028.89 1.524 2.336 1.084 2.902.829.091-.645.351-1.085.635-1.334-2.214-.251-4.542-1.107-4.542-4.93 0-1.087.389-1.979 1.024-2.675-.101-.253-.446-1.268.099-2.64 0 0 .837-.269 2.742 1.021a9.582 9.582 0 0 1 2.496-.336 9.554 9.554 0 0 1 2.496.336c1.906-1.291 2.742-1.021 2.742-1.021.545 1.372.203 2.387.099 2.64.64.696 1.024 1.587 1.024 2.675 0 3.833-2.33 4.675-4.552 4.922.355.308.675.916.675 1.846 0 1.334-.012 2.41-.012 2.737 0 .267.178.577.687.479C19.146 20.115 22 16.379 22 11.974 22 6.465 17.535 2 12.026 2z"
                            }
                        }
                    }
                }
            }
        }
        a {
            class: "{btn_class}",
            href: "#top",
            aria_label: "go to top",
            title: "Go to Top (Alt + G)",
            accesskey: "g",
            onclick: move |evt| {
                evt.prevent_default();
                scroll_to_top();
            },
            svg {
                xmlns: "http://www.w3.org/2000/svg",
                height: "24px",
                view_box: "0 -960 960 960",
                width: "24px",
                fill: "currentColor",
                path { d: "m296-224-56-56 240-240 240 240-56 56-184-183-184 183Zm0-240-56-56 240-240 240 240-56 56-184-183-184 183Z" }
            }
        }
    }
}

/// 平滑滚动到页面顶部，并清理 history 中的 `#` 哈希。
///
/// 仅在 `target_arch = "wasm32"` 下执行实际滚动，SSR 环境中为空操作。
fn scroll_to_top() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            let options = web_sys::ScrollToOptions::new();
            options.set_top(0.0);
            options.set_behavior(web_sys::ScrollBehavior::Smooth);
            window.scroll_to_with_scroll_to_options(&options);

            if let Ok(history) = window.history() {
                let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(" "));
            }
        }
    }
}
