//! 备份页只组合任务状态、导入、列表与自动备份设置。
mod import;
mod row;
mod settings;
mod task;
use crate::components::ui::{LoadingButton, ADMIN_CARD_CLASS, ADMIN_TABLE_CLASS, BTN_OUTLINE};
use dioxus::prelude::*;
use import::ImportBackup;
use row::BackupRow;
use settings::BackupSettingsCard;

#[allow(non_snake_case)]
pub(super) fn BackupTab() -> Element {
    let state = task::use_backup_task();
    let backups = state.backups;
    let loading = state.loading;
    let error = state.error;
    let active_progress = state.progress;
    let current_backups = backups.read().clone();
    let current_error = error.read().clone();
    let current_progress = active_progress.read().clone();
    let is_busy = state.busy();

    rsx! {
        div { class: "space-y-4",
            // 自动备份设置卡片（顶部，与手动按钮/列表同屏）
            BackupSettingsCard {}

            // 操作栏
            div { class: "flex items-center gap-3",
                LoadingButton {
                    label: "创建备份".to_string(),
                    loading: is_busy,
                    variant: "sm",
                    onclick: move |_| state.start(None),
                }
                button {
                    class: "{BTN_OUTLINE}",
                    disabled: loading() || is_busy,
                    onclick: move |_| state.refresh(),
                    "刷新列表"
                }
                // 导入备份：隐藏 file input 包在 label 内，原生点击唤起文件选择
                //（asset_picker.rs 同款惯例，无需 JS 触发）。两步式——导入仅入库
                // backups/ 并刷新列表，恢复仍走列表行的既有管线（见 ADR 0001）。
                ImportBackup { state }
            }

            // 进度
            if let Some(p) = current_progress {
                div { class: "{ADMIN_CARD_CLASS} p-4",
                    div { class: "flex items-center justify-between mb-2",
                        span { class: "text-sm font-medium text-paper-primary", "{p.stage}" }
                        span { class: "text-sm text-paper-secondary", "{p.percent}%" }
                    }
                    div { class: "w-full bg-paper-entry rounded-full h-2 overflow-hidden",
                        div {
                            class: "bg-paper-accent h-full transition-all",
                            style: "width: {p.percent}%",
                        }
                    }
                    if let Some(detail) = p.detail {
                        p { class: "text-xs text-paper-secondary mt-2", "{detail}" }
                    }
                    if let Some(err) = p.error {
                        p { class: "text-xs text-red-600 dark:text-red-400 mt-2",
                            "错误：{err}"
                        }
                    }
                }
            }

            // 错误
            if let Some(err) = current_error {
                div { role: "alert", class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-3 text-sm text-red-700 dark:text-red-300",
                    "{err}"
                }
            }

            // 备份列表
            if !current_backups.is_empty() {
                div { class: "{ADMIN_TABLE_CLASS}",
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-sm",
                            thead {
                                tr { class: "border-b border-paper-border text-left text-paper-secondary",
                                    th { class: "px-4 py-2 font-medium", "文件名" }
                                    th { class: "px-4 py-2 font-medium", "来源" }
                                    th { class: "px-4 py-2 font-medium", "模式" }
                                    th { class: "px-4 py-2 font-medium text-right",
                                        "大小"
                                    }
                                    th { class: "px-4 py-2 font-medium text-right",
                                        "操作"
                                    }
                                }
                            }
                            tbody {
                                for b in current_backups.iter() {
                                    BackupRow {
                                        key: "{b.filename}",
                                        info: b.clone(),
                                        busy: is_busy,
                                        on_restore: move |filename: String| state.start(Some(filename)),
                                        on_delete: move |filename: String| state.delete(filename),
                                    }
                                }
                            }
                        }
                    }
                }
            } else if !loading() {
                div { class: "text-paper-secondary text-sm py-4", "暂无备份文件" }
            }

            p { class: "text-xs text-paper-secondary",
                "备份优先用 pg_dump（含 schema），不可用时回退纯 SQL（仅数据，且不可经 psql 恢复）。"
                "恢复仅接受本系统生成的备份，且只恢复数据库；uploads 素材需从配对 tar.gz 手动还原。"
                "导入仅接受本系统生成的 .sql 备份（签名校验，单文件默认上限 512MB）；"
                "以 auto_ 前缀导入的文件会参与自动轮转。"
            }
        }
    }
}
