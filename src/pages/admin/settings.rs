//! 后台「站点配置」页面。
//!
//! 管理员在此维护站点级配置：前台公开配置（页脚 GitHub 链接）与后台行为参数
//! （素材上传并发数）。数据经 Dioxus server functions（`src/api/settings.rs`）
//! 读写；GitHub 链接写入成功后服务端失效 moka 缓存与全部公开页 SSR 缓存，前台
//! 下次访问立即生效；上传并发数由上传弹窗在打开页面时拉取生效。
//!
//! 仅 WASM 前端交互（照 mcp.rs / friends.rs 的 `#[cfg(target_arch = "wasm32")]`
//! 门控模式）。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::settings::{
    get_site_settings, get_upload_settings, update_site_settings, update_upload_settings,
};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::{FormInput, FormLabel, FormSelect};
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{LoadingButton, ADMIN_CARD_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::models::settings::{SiteSettings, UploadSettings};

/// 管理后台站点配置页面。
///
/// 加载服务端当前配置回填表单，编辑后保存即失效前台缓存。
#[component]
pub fn SiteSettingsPage() -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        // 服务端当前配置（加载后只读展示于「当前值」）。
        let mut saved: Signal<SiteSettings> = use_signal(SiteSettings::default);
        // 表单草稿。
        let mut github_draft: Signal<String> = use_signal(String::new);
        // 操作状态。
        let mut loading: Signal<bool> = use_signal(|| true);
        let mut saving: Signal<bool> = use_signal(|| false);
        let mut just_saved: Signal<bool> = use_signal(|| false);
        let mut toast: Signal<Option<(String, bool)>> = use_signal(|| None);

        // 素材上传并发配置（独立加载/保存循环，与 GitHub 链接互不影响）。
        let mut upload_saved: Signal<UploadSettings> = use_signal(UploadSettings::default);
        let mut upload_draft: Signal<i32> =
            use_signal(|| crate::models::settings::DEFAULT_UPLOAD_CONCURRENCY);
        let mut upload_loading: Signal<bool> = use_signal(|| true);
        let mut upload_saving: Signal<bool> = use_signal(|| false);
        let mut upload_just_saved: Signal<bool> = use_signal(|| false);

        // 首次挂载加载服务端配置。
        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            spawn(async move {
                match get_site_settings().await {
                    Ok(s) => {
                        github_draft.set(s.github_url.clone());
                        saved.set(s);
                    }
                    Err(e) => toast.set(Some((format!("加载失败：{e}"), true))),
                }
                loading.set(false);
                match get_upload_settings().await {
                    Ok(s) => {
                        upload_draft.set(s.concurrency);
                        upload_saved.set(s);
                    }
                    Err(e) => toast.set(Some((format!("加载失败：{e}"), true))),
                }
                upload_loading.set(false);
            });
        });

        // 草稿相对已保存配置是否存在差异：控制保存按钮可用性与「未保存」提示。
        let dirty = use_memo(move || github_draft().trim() != saved().github_url);
        let upload_dirty = use_memo(move || upload_draft() != upload_saved().concurrency);

        rsx! {
            div { class: "w-full max-w-7xl mx-auto space-y-8",
                // 页头
                div { class: "flex flex-col md:flex-row md:items-end justify-between gap-6 pb-8 border-b border-[var(--color-paper-border)]/50",
                    div {
                        h1 { class: "text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                            "站点配置"
                        }
                        p { class: "text-base text-[var(--color-paper-secondary)] mt-2",
                            "管理站点公开配置与后台行为参数。"
                        }
                    }
                }

                // 操作提示条
                if let Some((msg, is_err)) = toast() {
                    div {
                        class: if is_err { "text-sm rounded-lg px-3 py-2 bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300" } else { "text-sm rounded-lg px-3 py-2 bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300" },
                        "{msg}"
                    }
                }

                // GitHub 链接卡片
                div { class: "{ADMIN_CARD_CLASS} p-8 flex flex-col gap-6",
                    div { class: "flex items-center gap-3",
                        // GitHub 图标（与页脚一致）
                        span { class: "inline-flex items-center justify-center w-10 h-10 rounded-full bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]",
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "22",
                                height: "22",
                                view_box: "0 0 24 24",
                                fill: "currentColor",
                                path {
                                    fill_rule: "evenodd",
                                    clip_rule: "evenodd",
                                    d: "M12.026 2c-5.509 0-9.974 4.465-9.974 9.974 0 4.406 2.857 8.145 6.821 9.465.499.09.679-.217.679-.481 0-.237-.008-.865-.011-1.696-2.775.602-3.361-1.338-3.361-1.338-.452-1.152-1.107-1.459-1.107-1.459-.905-.619.069-.605.069-.605 1.002.07 1.527 1.028 1.527 1.028.89 1.524 2.336 1.084 2.902.829.091-.645.351-1.085.635-1.334-2.214-.251-4.542-1.107-4.542-4.93 0-1.087.389-1.979 1.024-2.675-.101-.253-.446-1.268.099-2.64 0 0 .837-.269 2.742 1.021a9.582 9.582 0 0 1 2.496-.336 9.554 9.554 0 0 1 2.496.336c1.906-1.291 2.742-1.021 2.742-1.021.545 1.372.203 2.387.099 2.64.64.696 1.024 1.587 1.024 2.675 0 3.833-2.33 4.675-4.552 4.922.355.308.675.916.675 1.846 0 1.334-.012 2.41-.012 2.737 0 .267.178.577.687.479C19.146 20.115 22 16.379 22 11.974 22 6.465 17.535 2 12.026 2z"
                                }
                            }
                        }
                        div {
                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]",
                                "页脚 GitHub 链接"
                            }
                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                "配置后，页脚右侧展示 GitHub 图标并跳转此链接；留空则不展示。"
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
                        p { class: "text-xs text-[var(--color-paper-secondary)]",
                            "可省略 https:// 前缀，保存时自动补全。"
                        }
                    }

                    // 当前值预览
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

                    // 底部操作行
                    div { class: "flex items-center justify-between gap-4 pt-1",
                        if just_saved() {
                            span { class: "inline-flex items-center gap-1.5 text-xs text-[var(--color-paper-accent)]",
                                svg {
                                    class: "w-3.5 h-3.5",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2.5",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M5 13l4 4L19 7",
                                    }
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
                                            toast.set(Some(("保存成功".to_string(), false)));
                                        }
                                        Err(e) => {
                                            toast.set(Some((format!("保存失败：{e}"), true)));
                                        }
                                    }
                                    saving.set(false);
                                });
                            },
                        }
                    }
                }

                // 素材上传并发卡片
                div { class: "{ADMIN_CARD_CLASS} p-8 flex flex-col gap-6",
                    div { class: "flex items-center gap-3",
                        // 上传图标（与素材上传弹窗一致的 Feather 线框风格）
                        span { class: "inline-flex items-center justify-center w-10 h-10 rounded-full bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]",
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "22",
                                height: "22",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.8",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                                polyline { points: "17 8 12 3 7 8" }
                                line {
                                    x1: "12",
                                    y1: "3",
                                    x2: "12",
                                    y2: "15",
                                }
                            }
                        }
                        div {
                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]",
                                "素材上传并发数"
                            }
                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                "素材管理页上传弹窗同时发起的上传任务数，调高可加速批量上传。"
                            }
                        }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "并发数", html_for: Some("upload-concurrency".to_string()) }
                        FormSelect {
                            id: Some("upload-concurrency".to_string()),
                            value: upload_draft(),
                            options: vec![
                                (1, "1（顺序上传）"),
                                (2, "2"),
                                (3, "3（默认）"),
                                (4, "4"),
                                (5, "5"),
                                (6, "6"),
                                (7, "7"),
                                (8, "8"),
                            ],
                            onchange: move |v: i32| {
                                upload_draft.set(v);
                                upload_just_saved.set(false);
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]",
                            "弹窗按并发数自动放大张间间隔，聚合速率始终与上传限流对齐，不会触发 429。首次部署可用 UPLOAD_CONCURRENCY 环境变量播种初始值。"
                        }
                    }

                    // 当前值预览
                    if !upload_loading() {
                        div { class: "text-sm text-[var(--color-paper-secondary)] flex items-center gap-1.5 flex-wrap",
                            "当前："
                            span { class: "font-mono", "{upload_saved().concurrency} 路并发" }
                        }
                    }

                    // 底部操作行
                    div { class: "flex items-center justify-between gap-4 pt-1",
                        if upload_just_saved() {
                            span { class: "inline-flex items-center gap-1.5 text-xs text-[var(--color-paper-accent)]",
                                svg {
                                    class: "w-3.5 h-3.5",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2.5",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M5 13l4 4L19 7",
                                    }
                                }
                                "已保存"
                            }
                        } else if upload_dirty() {
                            span { class: "text-xs text-[var(--color-paper-secondary)]", "有未保存的更改" }
                        } else {
                            span { class: "text-xs text-transparent select-none", "·" }
                        }
                        LoadingButton {
                            label: "保存设置".to_string(),
                            loading: upload_saving(),
                            disabled: upload_loading() || upload_just_saved() || !upload_dirty(),
                            onclick: move |_| {
                                let n = upload_draft();
                                upload_saving.set(true);
                                spawn(async move {
                                    match update_upload_settings(n).await {
                                        Ok(s) => {
                                            upload_saved.set(s.clone());
                                            upload_draft.set(s.concurrency);
                                            upload_just_saved.set(true);
                                            toast.set(Some(("保存成功".to_string(), false)));
                                        }
                                        Err(e) => {
                                            toast.set(Some((format!("保存失败：{e}"), true)));
                                        }
                                    }
                                    upload_saving.set(false);
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
        // server 构建下页面无前端交互；路由实际只在 WASM 渲染。
        rsx! {
            p { class: "text-paper-secondary", "此页面仅在浏览器中可用。" }
        }
    }
}
