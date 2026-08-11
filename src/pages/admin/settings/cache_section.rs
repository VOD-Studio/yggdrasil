//! 图片磁盘缓存配置分区。
//!
//! 即时生效层：清理任务每小时 tick 时从 runtime accessor 读取最新值。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::settings::{get_image_cache_settings, update_image_cache_settings};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::{FormLabel, INPUT_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{LoadingButton, ADMIN_CARD_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::models::settings::ImageCacheSettings;

/// 图片磁盘缓存配置分区组件。
#[allow(non_snake_case)]
#[component]
pub fn CacheSection(toast: Callback<(String, bool)>) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let mut saved: Signal<ImageCacheSettings> = use_signal(ImageCacheSettings::default);
        let mut mb_draft: Signal<u32> = use_signal(|| 1024);
        let mut hours_draft: Signal<u32> = use_signal(|| 168);
        let mut loading = use_signal(|| true);
        let mut saving = use_signal(|| false);
        let mut just_saved = use_signal(|| false);

        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            {
                let mut saved = saved;
                let mut mb_draft = mb_draft;
                let mut hours_draft = hours_draft;
                let mut loading = loading;
                spawn(async move {
                    match get_image_cache_settings().await {
                        Ok(s) => {
                            saved.set(s.clone());
                            mb_draft.set(s.disk_cache_max_mb);
                            hours_draft.set(s.disk_cache_max_age_hours);
                        }
                        Err(e) => toast.call((format!("加载失败：{e}"), true)),
                    }
                    loading.set(false);
                });
            }
        });

        let dirty = use_memo(move || {
            mb_draft() != saved().disk_cache_max_mb || hours_draft() != saved().disk_cache_max_age_hours
        });

        rsx! {
            div { class: "space-y-6",
                div { class: "{ADMIN_CARD_CLASS} p-6 md:p-8 flex flex-col gap-6",
                    div { class: "flex items-center gap-3",
                        span { class: "inline-flex items-center justify-center w-10 h-10 rounded-full bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]",
                            svg { class: "w-5 h-5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                                path { d: "M21 8v13H3V8 M1 3h22v5H1z M10 12h4" }
                            }
                        }
                        div {
                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]", "图片磁盘缓存" }
                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                "uploads/.cache/ 的容量与保留策略，清理任务每小时扫描一次。"
                            }
                        }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "最大容量（MB）", html_for: Some("cache-max-mb".to_string()) }
                        input {
                            id: "cache-max-mb",
                            r#type: "number",
                            min: "1",
                            class: "{INPUT_CLASS}",
                            value: "{mb_draft()}",
                            oninput: move |e: Event<FormData>| {
                                let v = e.value();
                                if let Ok(n) = v.parse::<u32>() { mb_draft.set(n); }
                                just_saved.set(false);
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]",
                            "超限时按修改时间删除最旧的缓存文件。默认 1024 MB。"
                        }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "最大保留时长（小时）", html_for: Some("cache-max-age".to_string()) }
                        input {
                            id: "cache-max-age",
                            r#type: "number",
                            min: "1",
                            class: "{INPUT_CLASS}",
                            value: "{hours_draft()}",
                            oninput: move |e: Event<FormData>| {
                                let v = e.value();
                                if let Ok(n) = v.parse::<u32>() { hours_draft.set(n); }
                                just_saved.set(false);
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]",
                            "超期的缓存文件优先删除。默认 168 小时（7 天）。"
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
                                let mb = mb_draft();
                                let hours = hours_draft();
                                saving.set(true);
                                spawn(async move {
                                    match update_image_cache_settings(mb, hours).await {
                                        Ok(s) => {
                                            saved.set(s.clone());
                                            mb_draft.set(s.disk_cache_max_mb);
                                            hours_draft.set(s.disk_cache_max_age_hours);
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
        rsx! { div { class: "p-8 text-[var(--color-paper-secondary)]", "缓存配置（前端渲染）" } }
    }
}
