//! 备份 tab：创建与导入备份、任务进度、备份列表（下载、恢复、删除）。
//! 定时备份的调度在「设置 › 自动备份」。
mod import;
mod row;
mod task;

use crate::components::ui::{button_class, ButtonSize, ButtonVariant, Icon, IconName};
use crate::router::Route;
use dioxus::prelude::*;
use import::ImportBackup;
use row::BackupRow;

/// 备份 tab。
#[component]
pub(super) fn BackupTab() -> Element {
    let state = task::use_backup_task();
    let loading = state.loading;
    let backups = state.backups.read().clone();
    let error = state.error.read().clone();
    let progress = state.progress.read().clone();
    let busy = state.busy();
    rsx! {
        div { class: "sys-backup-head",
            p {
                "优先用 pg_dump 生成含表结构的完整备份；包含素材时另打包配对的 tar.gz。定时备份在 "
                Link { to: Route::SiteSettingsPage {}, "设置 › 自动备份" }
                " 里调整。"
            }
            div { class: "sys-backup-actions",
                button {
                    class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                    r#type: "button",
                    disabled: loading() || busy,
                    onclick: move |_| state.refresh(),
                    Icon { name: IconName::RefreshCw, size: 14 }
                    "刷新列表"
                }
                ImportBackup { state }
                button {
                    class: button_class(ButtonVariant::Primary, ButtonSize::Sm),
                    r#type: "button",
                    disabled: busy,
                    onclick: move |_| state.start(None),
                    if busy { "处理中…" } else { "创建备份" }
                }
            }
        }

        if let Some(p) = progress {
            div { class: "admin-panel sys-progress", role: "status",
                span { class: "sys-progress-stage", "{p.stage}" }
                div { class: "sys-progress-bar", aria_hidden: "true",
                    div { style: "width: {p.percent}%" }
                }
                span { class: "sys-progress-pct", "{p.percent}%" }
                if let Some(detail) = p.detail {
                    p { class: "sys-progress-detail", "{detail}" }
                }
                if let Some(err) = p.error {
                    p { class: "sys-error", "错误：{err}" }
                }
            }
        }

        if let Some(err) = error {
            p { class: "sys-error", role: "alert", "{err}" }
        }

        if !backups.is_empty() {
            section { class: "admin-panel sys-section",
                div { class: "sys-table-scroll",
                    table { class: "admin-table",
                        thead {
                            tr {
                                th { "文件" }
                                th { "来源" }
                                th { "模式" }
                                th { class: "admin-table-num", "大小" }
                                th { class: "admin-table-actions", span { class: "sr-only", "操作" } }
                            }
                        }
                        tbody {
                            for b in backups.iter() {
                                BackupRow {
                                    key: "{b.filename}",
                                    info: b.clone(),
                                    busy,
                                    on_restore: move |filename: String| state.start(Some(filename)),
                                    on_delete: move |filename: String| state.delete(filename),
                                }
                            }
                        }
                    }
                }
            }
        } else if !loading() {
            div { class: "admin-panel",
                crate::components::empty_state::EmptyState {
                    icon: IconName::Archive,
                    title: "还没有备份",
                    description: "创建一份手动备份，或在设置里开启定时备份。",
                }
            }
        }
        p { class: "sys-footnote",
            "恢复只接受本系统生成的备份，且只恢复数据库；素材需要从配对的 tar.gz 手动还原。导入只接受本系统生成的 .sql 备份（签名校验，单个文件默认不超过 512 MB），以 auto_ 开头的文件会参与自动轮转。"
        }
    }
}
