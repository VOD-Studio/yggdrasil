//! 素材上传并发数分区（从原 settings.rs 提取）。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::settings::{get_upload_settings, update_upload_settings};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::{FormLabel, FormSelect};
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{LoadingButton, ADMIN_CARD_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::models::settings::UploadSettings;

/// 素材上传并发配置分区组件。
#[allow(non_snake_case)]
#[component]
pub fn UploadSection(toast: Callback<(String, bool)>) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let mut saved: Signal<UploadSettings> = use_signal(UploadSettings::default);
        let mut draft: Signal<i32> = use_signal(|| crate::models::settings::DEFAULT_UPLOAD_CONCURRENCY);
        let mut loading = use_signal(|| true);
        let mut saving = use_signal(|| false);
        let mut just_saved = use_signal(|| false);

        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            {
                let mut saved = saved;
                let mut draft = draft;
                let mut loading = loading;
                spawn(async move {
                    match get_upload_settings().await {
                        Ok(s) => {
                            saved.set(s.clone());
                            draft.set(s.concurrency);
                        }
                        Err(e) => toast.call((format!("加载失败：{e}"), true)),
                    }
                    loading.set(false);
                });
            }
        });

        let dirty = use_memo(move || draft() != saved().concurrency);

        rsx! {
            div { class: "space-y-6",
                div { class: "{ADMIN_CARD_CLASS} p-6 md:p-8 flex flex-col gap-6",
                    div { class: "flex items-center gap-3",
                        span { class: "inline-flex items-center justify-center w-10 h-10 rounded-full bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]",
                            svg { class: "w-5 h-5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                                path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                                polyline { points: "17 8 12 3 7 8" }
                                line { x1: "12", y1: "3", x2: "12", y2: "15" }
                            }
                        }
                        div {
                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]", "素材上传并发数" }
                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                "素材管理页上传弹窗同时发起的上传任务数，调高可加速批量上传。"
                            }
                        }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "并发数", html_for: Some("upload-concurrency".to_string()) }
                        FormSelect {
                            id: Some("upload-concurrency".to_string()),
                            value: draft(),
                            options: vec![
                                (1, "1（顺序上传）"), (2, "2"), (3, "3（默认）"),
                                (4, "4"), (5, "5"), (6, "6"), (7, "7"), (8, "8"),
                            ],
                            onchange: move |v: i32| {
                                draft.set(v);
                                just_saved.set(false);
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]",
                            "弹窗按并发数自动放大张间间隔，聚合速率始终与上传限流对齐，不会触发 429。"
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
                                let n = draft();
                                saving.set(true);
                                spawn(async move {
                                    match update_upload_settings(n).await {
                                        Ok(s) => {
                                            saved.set(s.clone());
                                            draft.set(s.concurrency);
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
        rsx! { div { class: "p-8 text-[var(--color-paper-secondary)]", "上传配置（前端渲染）" } }
    }
}
