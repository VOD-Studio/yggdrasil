//! 页脚组件
//!
//! 单行：© · RSS · 更新日志 · 写作指南 · 构建说明 · GitHub（站点设置配置时显示）
//! · 回到顶部。样式见 styles/components/shell.css。
use crate::api::settings::get_site_settings;
use crate::components::ui::{Icon, IconName};
use crate::router::Route;
use dioxus::prelude::*;
use dioxus::router::components::Link;

/// 页脚组件：读取站点设置中的 GitHub 链接。
#[component]
pub fn Footer(#[props(default = "max-w-[880px]")] max_width: &'static str) -> Element {
    // 读取站点公开配置（页脚 GitHub 链接）。公开接口，SSR 与 WASM 均可调用；
    // 服务端有 moka 缓存兜底，解析极快。`?` 在 pending 时向上抛 Suspense。
    let settings_res = use_server_future(get_site_settings)?;
    let github_url: Option<String> = settings_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|s| s.github_url.clone())
        .filter(|u| !u.is_empty());
    rsx! {
        footer { "data-vt-shell": "frontend-footer", class: "site-footer",
            div { class: "site-footer-inner {max_width}",
                span { class: "tabular-nums", "© 2026 Yggdrasil" }
                div { class: "site-footer-links",
                    a { href: "/feed.xml", "RSS" }
                    Link { to: Route::Changelog {}, "更新日志" }
                    Link { to: Route::WritingGuide {}, "写作指南" }
                    span { "由 Rust 与 Dioxus 构建" }
                    if let Some(url) = github_url.as_ref() {
                        a {
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            aria_label: "GitHub",
                            title: "GitHub",
                            Icon { name: IconName::Github, size: 16 }
                        }
                    }
                    button { r#type: "button", onclick: move |_| scroll_to_top(),
                        "回到顶部"
                        Icon { name: IconName::ArrowUp, size: 13 }
                    }
                }
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
