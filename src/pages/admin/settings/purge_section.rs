//! 孤儿素材自动清理分区（启用开关 + 保留天数）。
//!
//! 后台任务每天读取 DB 值执行清理：无文章引用且无存活评论引用的素材，
//! 超过保留天数后物理删除（文件 + DB 行 + 派生缓存）。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::settings::{get_asset_purge_settings, update_asset_purge_settings};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::{FormLabel, INPUT_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{Checkbox, LoadingButton, ADMIN_CARD_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::models::settings::AssetPurgeSettings;

/// 孤儿素材自动清理分区组件。
#[allow(non_snake_case)]
#[component]
pub fn AssetPurgeSection(toast: Callback<(String, bool)>) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let mut saved: Signal<AssetPurgeSettings> = use_signal(AssetPurgeSettings::default);
        let mut enabled_draft: Signal<bool> = use_signal(|| true);
        let mut days_draft: Signal<i32> = use_signal(|| 7);
        let loading = use_signal(|| true);
        let mut saving = use_signal(|| false);
        let mut just_saved = use_signal(|| false);

        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            {
                let mut saved = saved;
                let mut enabled_draft = enabled_draft;
                let mut days_draft = days_draft;
                let mut loading = loading;
                spawn(async move {
                    match get_asset_purge_settings().await {
                        Ok(s) => {
                            saved.set(s.clone());
                            enabled_draft.set(s.auto_purge_enabled);
                            days_draft.set(s.retention_days);
                        }
                        Err(e) => toast.call((format!("加载失败：{e}"), true)),
                    }
                    loading.set(false);
                });
            }
        });

        let dirty = use_memo(move || {
            enabled_draft() != saved().auto_purge_enabled || days_draft() != saved().retention_days
        });

        rsx! {
            div { class: "space-y-6",
                div { class: "{ADMIN_CARD_CLASS} p-6 md:p-8 flex flex-col gap-6",
                    div { class: "flex items-center gap-3",
                        span { class: "inline-flex items-center justify-center w-10 h-10 rounded-full bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]",
                            svg {
                                class: "w-5 h-5",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.8",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M21 8v13H3V8 M1 3h22v5H1z M10 12h4" }
                            }
                        }
                        div {
                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]",
                                "孤儿素材自动清理"
                            }
                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                "无文章/评论引用的图片超过保留天数后由后台任务物理删除（每天执行）。"
                            }
                        }
                    }

                    label { class: "flex items-center gap-3 cursor-pointer max-w-xl",
                        Checkbox {
                            checked: enabled_draft(),
                            onchange: move |checked: bool| {
                                enabled_draft.set(checked);
                                just_saved.set(false);
                            },
                        }
                        div { class: "flex flex-col",
                            span { class: "text-sm font-medium text-[var(--color-paper-primary)]",
                                "启用自动清理"
                            }
                            span { class: "text-xs text-[var(--color-paper-secondary)]",
                                "关闭后未引用图片永久保留，需在素材管理页手动清理。评论区允许匿名传图，建议保持开启。"
                            }
                        }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel {
                            label: "保留天数",
                            html_for: Some("asset-purge-days".to_string()),
                        }
                        input {
                            id: "asset-purge-days",
                            r#type: "number",
                            min: "1",
                            max: "365",
                            class: "{INPUT_CLASS}",
                            value: "{days_draft()}",
                            oninput: move |e: Event<FormData>| {
                                let v = e.value();
                                if let Ok(n) = v.parse::<i32>() {
                                    days_draft.set(n);
                                }
                                just_saved.set(false);
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]",
                            "超出保留天数的未引用图片被物理删除（1–365 天）。保护窗防止误删尚未保存的草稿引用。"
                        }
                    }

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
                            span { class: "text-xs text-[var(--color-paper-secondary)]",
                                "有未保存的更改"
                            }
                        } else {
                            span { class: "text-xs text-transparent select-none", "·" }
                        }
                        LoadingButton {
                            label: "保存设置".to_string(),
                            loading: saving(),
                            disabled: loading() || just_saved() || !dirty(),
                            onclick: move |_| {
                                let e = enabled_draft();
                                let d = days_draft();
                                saving.set(true);
                                spawn(async move {
                                    match update_asset_purge_settings(e, d).await {
                                        Ok(s) => {
                                            saved.set(s.clone());
                                            enabled_draft.set(s.auto_purge_enabled);
                                            days_draft.set(s.retention_days);
                                            just_saved.set(true);
                                            toast.call(("保存成功".to_string(), false));
                                        }
                                        Err(err) => toast.call((format!("保存失败：{err}"), true)),
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
        rsx! {
            div { class: "p-8 text-[var(--color-paper-secondary)]", "孤儿素材清理配置（前端渲染）" }
        }
    }
}
