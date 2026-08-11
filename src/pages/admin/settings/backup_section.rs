//! 自动备份调度分区（调度开关、执行时间、保留份数、是否含 uploads）。
//!
//! 从 /admin/system 的备份设置卡片整合到设置页面。备份操作（创建/恢复/删除）
//! 仍在 /admin/system 的备份 tab。写入后唤醒调度任务立即重排。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::settings::{get_backup_settings, update_backup_settings};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::{FormLabel, TimePicker, INPUT_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{Checkbox, LoadingButton, ADMIN_CARD_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::models::settings::{BackupSettings, BackupSettingsView};
#[cfg(target_arch = "wasm32")]
use crate::utils::time::{local_hhmm_to_utc, utc_hhmm_to_local};

/// 自动备份调度分区组件。
#[allow(non_snake_case)]
#[component]
pub fn BackupSection(toast: Callback<(String, bool)>) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let mut saved: Signal<BackupSettings> = use_signal(BackupSettings::default);
        let mut auto_draft: Signal<bool> = use_signal(|| false);
        let mut time_draft: Signal<String> = use_signal(|| "04:00".to_string());
        let mut retention_draft: Signal<i32> = use_signal(|| 30);
        let mut include_draft: Signal<bool> = use_signal(|| true);
        let mut loading = use_signal(|| true);
        let mut saving = use_signal(|| false);
        let mut just_saved = use_signal(|| false);

        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            {
                let mut saved = saved;
                let mut auto_draft = auto_draft;
                let mut time_draft = time_draft;
                let mut retention_draft = retention_draft;
                let mut include_draft = include_draft;
                let mut loading = loading;
                spawn(async move {
                    match get_backup_settings().await {
                        Ok(v) => {
                            let s = v.settings;
                            saved.set(s.clone());
                            auto_draft.set(s.auto_enabled);
                            time_draft.set(utc_hhmm_to_local(&s.time_utc));
                            retention_draft.set(s.retention_count);
                            include_draft.set(s.include_uploads);
                        }
                        Err(e) => toast.call((format!("加载失败：{e}"), true)),
                    }
                    loading.set(false);
                });
            }
        });

        let dirty = use_memo(move || {
            let s = saved();
            auto_draft() != s.auto_enabled
                || time_draft() != utc_hhmm_to_local(&s.time_utc)
                || retention_draft() != s.retention_count
                || include_draft() != s.include_uploads
        });

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
                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]", "自动备份" }
                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                "定时自动备份调度策略。备份操作（创建/恢复/删除）在「系统」页面。"
                            }
                        }
                    }

                    // 启用开关
                    label { class: "flex items-center gap-3 cursor-pointer max-w-xl",
                        Checkbox {
                            checked: auto_draft(),
                            onchange: move |checked: bool| {
                                auto_draft.set(checked);
                                just_saved.set(false);
                            },
                        }
                        div { class: "flex flex-col",
                            span { class: "text-sm font-medium text-[var(--color-paper-primary)]", "启用每天定时备份" }
                            span { class: "text-xs text-[var(--color-paper-secondary)]", "调度任务按面板写入的 DB 值运行，保存后立即重排。" }
                        }
                    }

                    // 执行时间（本地时间显示/编辑，存 UTC）
                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "执行时间", html_for: Some("backup-time".to_string()) }
                        p { class: "text-xs text-[var(--color-paper-secondary)]", "按浏览器本地时间显示与编辑，服务端以 UTC 存储" }
                        div { class: "flex items-center gap-3",
                            TimePicker {
                                id: Some("backup-time".to_string()),
                                value: time_draft(),
                                onchange: move |v: String| {
                                    time_draft.set(v);
                                    just_saved.set(false);
                                },
                            }
                            span { class: "text-xs text-[var(--color-paper-secondary)]", "本地时间" }
                        }
                    }

                    // 保留份数
                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "自动备份保留份数", html_for: Some("backup-retention".to_string()) }
                        input {
                            id: "backup-retention",
                            r#type: "number",
                            min: "1", max: "365",
                            class: "{INPUT_CLASS}",
                            value: "{retention_draft()}",
                            oninput: move |e: Event<FormData>| {
                                let v = e.value();
                                if let Ok(n) = v.parse::<i32>() { retention_draft.set(n); }
                                just_saved.set(false);
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]", "超出后最旧的备份连配对 uploads 包一起删除（1–365）。手动备份永不自动删除。" }
                    }

                    // 含 uploads
                    label { class: "flex items-center gap-3 cursor-pointer max-w-xl",
                        Checkbox {
                            checked: include_draft(),
                            onchange: move |checked: bool| {
                                include_draft.set(checked);
                                just_saved.set(false);
                            },
                        }
                        div { class: "flex flex-col",
                            span { class: "text-sm font-medium text-[var(--color-paper-primary)]", "备份包含 uploads 素材" }
                            span { class: "text-xs text-[var(--color-paper-secondary)]", "打包 tar.gz（排除可重建的 .cache）。" }
                        }
                    }

                    // 操作行
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
                                let a = auto_draft();
                                let t = local_hhmm_to_utc(&time_draft());
                                let r = retention_draft();
                                let i = include_draft();
                                saving.set(true);
                                spawn(async move {
                                    match update_backup_settings(a, t, r, i).await {
                                        Ok(v) => {
                                            let s = v.settings;
                                            saved.set(s.clone());
                                            auto_draft.set(s.auto_enabled);
                                            time_draft.set(utc_hhmm_to_local(&s.time_utc));
                                            retention_draft.set(s.retention_count);
                                            include_draft.set(s.include_uploads);
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
        rsx! { div { class: "p-8 text-[var(--color-paper-secondary)]", "备份配置（前端渲染）" } }
    }
}
