//! 站点公开配置分区（页脚 GitHub 链接）。
//!
//! 从原 `settings.rs` 提取的 GitHub 链接卡片。写入后服务端失效 moka 缓存与
//! 全部公开页 SSR 缓存，前台下次访问立即生效。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::settings::{get_site_settings, update_site_settings};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::{FormInput, FormLabel};
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{LoadingButton, ADMIN_CARD_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::models::settings::SiteSettings;

/// 传递给各分区的 toast 回调类型：(消息, 是否错误)。
type ToastCb = Callback<(String, bool)>;

/// 站点公开配置分区组件。
#[allow(non_snake_case)]
#[component]
pub fn SiteSection(toast: ToastCb) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let mut saved: Signal<SiteSettings> = use_signal(SiteSettings::default);
        let mut github_draft: Signal<String> = use_signal(String::new);
        let mut loading = use_signal(|| true);
        let mut saving = use_signal(|| false);
        let mut just_saved = use_signal(|| false);

        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            {
                let mut saved = saved;
                let mut github_draft = github_draft;
                let mut loading = loading;
                spawn(async move {
                    match get_site_settings().await {
                        Ok(s) => {
                            saved.set(s.clone());
                            github_draft.set(s.github_url);
                        }
                        Err(e) => toast.call((format!("加载失败：{e}"), true)),
                    }
                    loading.set(false);
                });
            }
        });

        let dirty = use_memo(move || github_draft().trim() != saved().github_url);

        rsx! {
            div { class: "space-y-6",
                div { class: "{ADMIN_CARD_CLASS} p-6 md:p-8 flex flex-col gap-6",
                    div { class: "flex items-center gap-3",
                        span { class: "inline-flex items-center justify-center w-10 h-10 rounded-full bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]",
                            svg {
                                xmlns: "http://www.w3.org/2000/svg", width: "22", height: "22",
                                view_box: "0 0 24 24", fill: "currentColor",
                                path {
                                    fill_rule: "evenodd", clip_rule: "evenodd",
                                    d: "M12.026 2c-5.509 0-9.974 4.465-9.974 9.974 0 4.406 2.857 8.145 6.821 9.465.499.09.679-.217.679-.481 0-.237-.008-.865-.011-1.696-2.775.602-3.361-1.338-3.361-1.338-.452-1.152-1.107-1.459-1.107-1.459-.905-.619.069-.605.069-.605 1.002.07 1.527 1.028 1.527 1.028.89 1.524 2.336 1.084 2.902.829.091-.645.351-1.085.635-1.334-2.214-.251-4.542-1.107-4.542-4.93 0-1.087.389-1.979 1.024-2.675-.101-.253-.446-1.268.099-2.64 0 0 .837-.269 2.742 1.021a9.582 9.582 0 0 1 2.496-.336 9.554 9.554 0 0 1 2.496.336c1.906-1.291 2.742-1.021 2.742-1.021.545 1.372.203 2.387.099 2.64.64.696 1.024 1.587 1.024 2.675 0 3.833-2.33 4.675-4.552 4.922.355.308.675.916.675 1.846 0 1.334-.012 2.41-.012 2.737 0 .267.178.577.687.479C19.146 20.115 22 16.379 22 11.974 22 6.465 17.535 2 12.026 2z"
                                }
                            }
                        }
                        div {
                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]", "页脚 GitHub 链接" }
                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                "配置后页脚右侧展示 GitHub 图标并跳转此链接；留空则不展示。"
                            }
                        }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "GitHub 链接", html_for: Some("site-github-url".to_string()) }
                        FormInput {
                            id: Some("site-github-url".to_string()),
                            r#type: "url",
                            placeholder: "github.com/your/repo",
                            value: github_draft(),
                            oninput: move |v: String| {
                                github_draft.set(v);
                                just_saved.set(false);
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]", "可省略 https:// 前缀，保存时自动补全。" }
                    }

                    if !loading() {
                        div { class: "text-sm text-[var(--color-paper-secondary)] flex items-center gap-1.5 flex-wrap",
                            "当前："
                            if saved().github_url.is_empty() {
                                span { class: "italic", "未配置（不展示图标）" }
                            } else {
                                a {
                                    class: "text-[var(--color-paper-accent)] hover:underline break-all",
                                    href: "{saved().github_url}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    "{saved().github_url}"
                                }
                            }
                        }
                    }

                    div { class: "flex items-center justify-between gap-4 pt-1",
                        if just_saved() {
                            span { class: "inline-flex items-center gap-1.5 text-xs text-[var(--color-paper-accent)]",
                                svg { class: "w-3.5 h-3.5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.5",
                                    path { stroke_linecap: "round", stroke_linejoin: "round", d: "M5 13l4 4L19 7" }
                                }
                                "已保存"
                            }
                        } else if dirty() {
                            span { class: "text-xs text-[var(--color-paper-secondary)]", "有未保存的更改" }
                        } else {
                            span { class: "text-xs text-transparent select-none", "·" }
                        }
                        LoadingButton {
                            label: "保存设置".to_string(),
                            loading: saving(),
                            disabled: loading() || just_saved() || !dirty(),
                            onclick: move |_| {
                                let url = github_draft().clone();
                                saving.set(true);
                                spawn(async move {
                                    match update_site_settings(url).await {
                                        Ok(s) => {
                                            saved.set(s.clone());
                                            github_draft.set(s.github_url);
                                            just_saved.set(true);
                                            toast.call(("保存成功".to_string(), false));
                                        }
                                        Err(e) => toast.call((format!("保存失败：{e}"), true)),
                                    }
                                    saving.set(false);
                                });
                            },
                        }
                    }
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        rsx! { div { class: "p-8 text-[var(--color-paper-secondary)]", "站点配置（前端渲染）" } }
    }
}
