//! cleanup behavior of the log page.
#[cfg(target_arch = "wasm32")]
use crate::api::logs::get_log_settings;
use crate::api::logs::update_log_settings;
use crate::components::forms::FormInput;
use crate::components::ui::{CollapsibleSettingsCard, LoadingButton};
use crate::models::log::LogSettings;
use dioxus::prelude::*;

/// 清理策略子组件：保留天数 + 最大行数，复用共享可折叠设置卡片。
///
/// 模式镜像 posts_trash.rs `AutoPurgeSettings`：草稿信号 + `use_memo` dirty +
/// LoadingButton 保存 + just_saved 反馈，加载/保存均在组件内部完成。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn CleanupPolicy() -> Element {
    let mut saved: Signal<LogSettings> = use_signal(LogSettings::default);
    let mut draft_days: Signal<String> = use_signal(|| "7".to_string());
    let mut draft_rows: Signal<String> = use_signal(|| "100000".to_string());
    let mut saving: Signal<bool> = use_signal(|| false);
    // 保存成功后的短暂反馈标记（用户再次编辑时清除）。
    let mut just_saved: Signal<bool> = use_signal(|| false);

    // 挂载即拉取一次服务端配置（缺键时服务端回退默认值）。
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            if let Ok(s) = get_log_settings().await {
                draft_days.set(s.retention_days.to_string());
                draft_rows.set(s.max_rows.to_string());
                saved.set(s);
            }
        });
    });

    // 草稿相对已保存配置是否存在差异（use_memo：依赖不变不重算 parse）。
    let dirty = use_memo(move || {
        let days_diff = draft_days()
            .trim()
            .parse::<i32>()
            .ok()
            .map(|d| d != saved().retention_days)
            .unwrap_or(true);
        let rows_diff = draft_rows()
            .trim()
            .parse::<i32>()
            .ok()
            .map(|r| r != saved().max_rows)
            .unwrap_or(true);
        days_diff || rows_diff
    });

    rsx! {
        CollapsibleSettingsCard {
            title: "清理策略".to_string(),
            summary: format!("保留 {} 天 · 上限 {} 行", saved().retention_days, saved().max_rows),
            enabled: true,
            on_toggle: move |_| just_saved.set(false),
            div { class: "border-t border-paper-border p-5 space-y-6",
                // 保留天数行
                div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-3",
                    div { class: "min-w-0",
                        div { class: "text-sm font-medium text-paper-primary", "保留天数" }
                        div { class: "text-xs text-paper-secondary mt-1",
                            "超过保留期的日志将被后台任务自动清理（1–90 天）"
                        }
                    }
                    div { class: "flex items-center gap-2 flex-shrink-0",
                        div { class: "w-28",
                            FormInput {
                                r#type: "number",
                                placeholder: "7",
                                value: draft_days(),
                                oninput: move |v: String| {
                                    draft_days.set(v);
                                    just_saved.set(false);
                                },
                            }
                        }
                        span { class: "text-xs text-paper-secondary", "天" }
                    }
                }

                // 最大行数行
                div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-3",
                    div { class: "min-w-0",
                        div { class: "text-sm font-medium text-paper-primary", "最大行数" }
                        div { class: "text-xs text-paper-secondary mt-1",
                            "日志表行数上限，超出后按最旧优先裁剪（1000–1000000）"
                        }
                    }
                    div { class: "flex items-center gap-2 flex-shrink-0",
                        div { class: "w-28",
                            FormInput {
                                r#type: "number",
                                placeholder: "100000",
                                value: draft_rows(),
                                oninput: move |v: String| {
                                    draft_rows.set(v);
                                    just_saved.set(false);
                                },
                            }
                        }
                        span { class: "text-xs text-paper-secondary", "行" }
                    }
                }

                // 底部操作行：草稿状态提示 + 保存按钮
                div { class: "flex items-center justify-between gap-4 pt-1",
                    if just_saved() {
                        span { class: "inline-flex items-center gap-1.5 text-xs text-paper-accent",
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
                        span { class: "text-xs text-paper-secondary", "有未保存的更改" }
                    } else {
                        span { class: "text-xs text-transparent select-none", "·" }
                    }
                    LoadingButton {
                        label: "保存设置".to_string(),
                        loading: saving(),
                        disabled: just_saved() || !dirty(),
                        variant: "sm",
                        onclick: move |_| {
                            // 与服务端 clamp 一致的客户端预收敛（服务端仍会兜底）。
                            let days = draft_days().trim().parse::<i32>().unwrap_or(7).clamp(1, 90);
                            let max_rows = draft_rows()
                                .trim()
                                .parse::<i32>()
                                .unwrap_or(100_000)
                                .clamp(1000, 1_000_000);
                            saving.set(true);
                            spawn(async move {
                                if let Ok(s) = update_log_settings(days, max_rows).await {
                                    // 先拷出字段再 set（LogSettings 非 Copy，set 会 move）。
                                    let saved_days = s.retention_days;
                                    let saved_rows = s.max_rows;
                                    saved.set(s);
                                    draft_days.set(saved_days.to_string());
                                    draft_rows.set(saved_rows.to_string());
                                    just_saved.set(true);
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
