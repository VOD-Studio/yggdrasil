//! 页脚组件
//!
//! 单行：© · RSS · 更新日志 · 写作指南 · 构建说明 · GitHub（站点设置配置时显示）
//! · 回到顶部。样式见 styles/components/shell.css。

use leptos::prelude::*;

use crate::api::settings::get_site_settings;
use crate::components::ui::{Icon, IconName};
use crate::hooks::ssr_resource::ssr_resource;
use crate::routes::Route;

/// 页脚组件：读取站点设置中的 GitHub 链接。
///
/// Props：
/// - `max_width`：内部内容的宽度类，需与页头、正文一致以保证左右边缘对齐
#[component]
pub fn Footer(#[prop(default = "max-w-[880px]")] max_width: &'static str) -> impl IntoView {
    // 读取站点公开配置（页脚 GitHub 链接）。公开接口，SSR 时解析并随页面序列化，
    // 水合直接复用；服务端有 moka 缓存兜底，解析极快。资源只在 `<Suspense>` 内读取，
    // 且 Suspense 只包住 GitHub 图标，页脚其余部分不等待数据。
    let settings_res = ssr_resource(|| (), |_| get_site_settings());

    view! {
        <footer data-vt-shell="frontend-footer" class="site-footer">
            <div class=format!("site-footer-inner {max_width}")>
                <span class="tabular-nums">"© 2026 Yggdrasil"</span>
                <div class="site-footer-links">
                    // rel="external"：让路由协调器放行，整页加载 RSS。
                    <a href="/feed.xml" rel="external">
                        "RSS"
                    </a>
                    <a href=Route::Changelog.to_string()>"更新日志"</a>
                    <a href=Route::WritingGuide.to_string()>"写作指南"</a>
                    <span>"由 Rust 与 Leptos 构建"</span>
                    <Suspense>
                        {move || Suspend::new(async move {
                            let github_url = settings_res
                                .await
                                .ok()
                                .map(|s| s.github_url)
                                .filter(|u| !u.is_empty());
                            github_url.map(|url| view! {
                                <a
                                    href=url
                                    target="_blank"
                                    rel="external noopener noreferrer"
                                    aria-label="GitHub"
                                    title="GitHub"
                                >
                                    <Icon name=IconName::Github size=16/>
                                </a>
                            })
                        })}
                    </Suspense>
                    <button type="button" on:click=move |_| scroll_to_top()>
                        "回到顶部"
                        <Icon name=IconName::ArrowUp size=13/>
                    </button>
                </div>
            </div>
        </footer>
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
