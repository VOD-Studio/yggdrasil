//! 图片处理配置分区（WebP 编码 + 尺寸上限 + 尺寸缓存 TTL）。
//!
//! Tier B 可编辑+重启生效。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::settings::{get_image_limit_settings, get_webp_settings, update_image_limit_settings, update_webp_settings};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::FormLabel;
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{LoadingButton, ADMIN_CARD_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::models::settings::{ImageLimitSettings, WebpSettings};

/// 图片处理配置分区组件。
#[allow(non_snake_case)]
#[component]
pub fn ImageSection(toast: Callback<(String, bool)>) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        // WebP 编码
        let mut webp_saved: Signal<WebpSettings> = use_signal(WebpSettings::default);
        let mut webp_draft: Signal<WebpSettings> = use_signal(WebpSettings::default);
        let mut webp_loading = use_signal(|| true);
        let mut webp_saving = use_signal(|| false);
        let mut webp_just_saved = use_signal(|| false);

        // 图片限制
        let mut img_saved: Signal<ImageLimitSettings> = use_signal(ImageLimitSettings::default);
        let mut img_draft: Signal<ImageLimitSettings> = use_signal(ImageLimitSettings::default);
        let mut img_loading = use_signal(|| true);
        let mut img_saving = use_signal(|| false);
        let mut img_just_saved = use_signal(|| false);

        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            {
                let mut webp_saved = webp_saved;
                let mut webp_draft = webp_draft;
                let mut webp_loading = webp_loading;
                let mut img_saved = img_saved;
                let mut img_draft = img_draft;
                let mut img_loading = img_loading;
                spawn(async move {
                    match get_webp_settings().await {
                        Ok(s) => { webp_saved.set(s.clone()); webp_draft.set(s); }
                        Err(e) => toast.call((format!("WebP 加载失败：{e}"), true)),
                    }
                    match get_image_limit_settings().await {
                        Ok(s) => { img_saved.set(s.clone()); img_draft.set(s); }
                        Err(e) => toast.call((format!("图片限制加载失败：{e}"), true)),
                    }
                    webp_loading.set(false);
                    img_loading.set(false);
                });
            }
        });

        let webp_dirty = use_memo(move || webp_draft() != webp_saved());
        let img_dirty = use_memo(move || img_draft() != img_saved());

        rsx! {
            div { class: "space-y-6",
                // WebP 编码卡片
                div { class: "{ADMIN_CARD_CLASS} p-6 md:p-8 flex flex-col gap-6",
                    div { class: "flex items-center gap-3",
                        span { class: "inline-flex items-center justify-center w-10 h-10 rounded-full bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]",
                            svg { class: "w-5 h-5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                                path { d: "M21 15l-5-5L5 21 M18 21H3V3h18v9z" }
                                circle { cx: "7", cy: "7", r: "1.5" }
                            }
                        }
                        div {
                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]", "WebP 编码" }
                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                span { class: "text-amber-600 dark:text-amber-400 font-medium", "⚠ 修改后需重启服务生效。" }
                            }
                        }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "质量系数（0.0–100.0）", html_for: Some("webp-quality".to_string()) }
                        input {
                            id: "webp-quality", r#type: "number", min: "0", max: "100", step: "1",
                            class: "w-full px-4 py-2 border border-[var(--color-paper-border)] rounded-2xl bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] focus:outline-none focus:border-[var(--color-paper-accent)] focus:ring-1 focus:ring-[var(--color-paper-accent)]/30 transition-colors",
                            value: "{webp_draft().quality}",
                            oninput: move |v: String| {
                                if let Ok(n) = v.parse::<f32>() { let mut d = webp_draft(); d.quality = n; webp_draft.set(d); webp_just_saved.set(false); }
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]", "0=最小体积，100=最佳质量。默认 85。" }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "编码方法（0–6）", html_for: Some("webp-method".to_string()) }
                        input {
                            id: "webp-method", r#type: "number", min: "0", max: "6",
                            class: "w-full px-4 py-2 border border-[var(--color-paper-border)] rounded-2xl bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] focus:outline-none focus:border-[var(--color-paper-accent)] focus:ring-1 focus:ring-[var(--color-paper-accent)]/30 transition-colors",
                            value: "{webp_draft().method}",
                            oninput: move |v: String| {
                                if let Ok(n) = v.parse::<u32>() { let mut d = webp_draft(); d.method = n; webp_draft.set(d); webp_just_saved.set(false); }
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]", "数值越大压缩率越高但越慢。默认 2。" }
                    }

                    div { class: "flex items-center justify-between gap-4 pt-1",
                        if webp_just_saved() {
                            span { class: "inline-flex items-center gap-1.5 text-xs text-[var(--color-paper-accent)]",
                                svg { class: "w-3.5 h-3.5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.5",
                                    path { stroke_linecap: "round", stroke_linejoin: "round", d: "M5 13l4 4L19 7" }
                                }
                                "已保存（重启后生效）"
                            }
                        } else if webp_dirty() {
                            span { class: "text-xs text-[var(--color-paper-secondary)]", "有未保存的更改" }
                        } else {
                            span { class: "text-xs text-transparent select-none", "·" }
                        }
                        LoadingButton {
                            label: "保存设置".to_string(),
                            loading: webp_saving(),
                            disabled: webp_loading() || webp_just_saved() || !webp_dirty(),
                            onclick: move |_| {
                                let d = webp_draft();
                                webp_saving.set(true);
                                spawn(async move {
                                    match update_webp_settings(d.quality, d.method).await {
                                        Ok(s) => { webp_saved.set(s.clone()); webp_draft.set(s); webp_just_saved.set(true); toast.call(("保存成功".to_string(), false)); }
                                        Err(e) => toast.call((format!("保存失败：{e}"), true)),
                                    }
                                    webp_saving.set(false);
                                });
                            },
                        }
                    }
                }

                // 图片尺寸限制卡片
                div { class: "{ADMIN_CARD_CLASS} p-6 md:p-8 flex flex-col gap-6",
                    div {
                        h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]", "图片尺寸限制" }
                        p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                            span { class: "text-amber-600 dark:text-amber-400 font-medium", "⚠ 修改后需重启服务生效。" }
                        }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "单边最大尺寸（像素）", html_for: Some("img-dim".to_string()) }
                        input {
                            id: "img-dim", r#type: "number", min: "512",
                            class: "w-full px-4 py-2 border border-[var(--color-paper-border)] rounded-2xl bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] focus:outline-none focus:border-[var(--color-paper-accent)] focus:ring-1 focus:ring-[var(--color-paper-accent)]/30 transition-colors",
                            value: "{img_draft().max_dimension}",
                            oninput: move |v: String| {
                                if let Ok(n) = v.parse::<u32>() { let mut d = img_draft(); d.max_dimension = n; img_draft.set(d); img_just_saved.set(false); }
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]", "上传与动态处理的宽/高上限。默认 8192，下限 512。" }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "总像素上限", html_for: Some("img-px".to_string()) }
                        input {
                            id: "img-px", r#type: "number", min: "1000000",
                            class: "w-full px-4 py-2 border border-[var(--color-paper-border)] rounded-2xl bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] focus:outline-none focus:border-[var(--color-paper-accent)] focus:ring-1 focus:ring-[var(--color-paper-accent)]/30 transition-colors",
                            value: "{img_draft().max_pixels}",
                            oninput: move |v: String| {
                                if let Ok(n) = v.parse::<u64>() { let mut d = img_draft(); d.max_pixels = n; img_draft.set(d); img_just_saved.set(false); }
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]", "决定单图解码内存（pixels×4+1MB）。默认 50M（约 7000×7000）。" }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "尺寸缓存 TTL（秒）", html_for: Some("img-ttl".to_string()) }
                        input {
                            id: "img-ttl", r#type: "number", min: "1",
                            class: "w-full px-4 py-2 border border-[var(--color-paper-border)] rounded-2xl bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] focus:outline-none focus:border-[var(--color-paper-accent)] focus:ring-1 focus:ring-[var(--color-paper-accent)]/30 transition-colors",
                            value: "{img_draft().dimensions_cache_ttl_secs}",
                            oninput: move |v: String| {
                                if let Ok(n) = v.parse::<u64>() { let mut d = img_draft(); d.dimensions_cache_ttl_secs = n; img_draft.set(d); img_just_saved.set(false); }
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]", "图片宽高缓存的 TTL，用于生成 aspect-ratio。默认 86400（24h）。" }
                    }

                    div { class: "flex items-center justify-between gap-4 pt-1",
                        if img_just_saved() {
                            span { class: "inline-flex items-center gap-1.5 text-xs text-[var(--color-paper-accent)]",
                                svg { class: "w-3.5 h-3.5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.5",
                                    path { stroke_linecap: "round", stroke_linejoin: "round", d: "M5 13l4 4L19 7" }
                                }
                                "已保存（重启后生效）"
                            }
                        } else if img_dirty() {
                            span { class: "text-xs text-[var(--color-paper-secondary)]", "有未保存的更改" }
                        } else {
                            span { class: "text-xs text-transparent select-none", "·" }
                        }
                        LoadingButton {
                            label: "保存设置".to_string(),
                            loading: img_saving(),
                            disabled: img_loading() || img_just_saved() || !img_dirty(),
                            onclick: move |_| {
                                let d = img_draft();
                                img_saving.set(true);
                                spawn(async move {
                                    match update_image_limit_settings(d.max_dimension, d.max_pixels, d.dimensions_cache_ttl_secs).await {
                                        Ok(s) => { img_saved.set(s.clone()); img_draft.set(s); img_just_saved.set(true); toast.call(("保存成功".to_string(), false)); }
                                        Err(e) => toast.call((format!("保存失败：{e}"), true)),
                                    }
                                    img_saving.set(false);
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
        rsx! { div { class: "p-8 text-[var(--color-paper-secondary)]", "图片配置（前端渲染）" } }
    }
}
