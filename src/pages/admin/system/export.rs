//! 导出 tab：按表或按查询流式导出 CSV / SQL。

use dioxus::prelude::*;

use crate::components::forms::{FormInput, FormTextarea, ToggleSwitch};
use crate::components::ui::{button_class, ButtonSize, ButtonVariant, Icon, IconName};

/// 数据导出 tab：按表或按查询导出 CSV / SQL，走 Axum 流式下载。
#[component]
pub(super) fn ExportTab() -> Element {
    let mode = use_signal(|| "table".to_string());
    let mut table_name = use_signal(String::new);
    let mut query = use_signal(String::new);
    let format = use_signal(|| "csv".to_string());
    let mut include_columns = use_signal(|| true);
    let ready = if mode() == "table" {
        !table_name().trim().is_empty()
    } else {
        !query().trim().is_empty()
    };
    // 触发下载：构造 GET /api/database/export?... 并打开。
    let do_export = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let source = if mode().as_str() == "table" {
                format!("table:{}", table_name.read().trim())
            } else {
                format!("query:{}", query.read())
            };
            let url = format!(
                "/api/database/export?source={}&format={}&include_columns={}",
                urlencode(&source),
                format(),
                include_columns(),
            );
            if let Some(window) = web_sys::window() {
                let _ = window.open_with_url(&url);
            }
        }
    };
    let segmented = |current: String,
                     items: &'static [(&'static str, &'static str)],
                     label: &'static str,
                     mut target: Signal<String>| {
        rsx! {
            div { class: "segmented settings-segmented", role: "radiogroup", aria_label: label,
                for (value, text) in items.iter().copied() {
                    button {
                        key: "{value}",
                        class: "segmented-item",
                        r#type: "button",
                        role: "radio",
                        aria_checked: "{current == value}",
                        onclick: move |_| target.set(value.to_string()),
                        "{text}"
                    }
                }
            }
        }
    };

    rsx! {
        section { class: "admin-panel sys-export",
            div { class: "settings-rows",
                div { class: "settings-row",
                    span { class: "settings-label", "方式" }
                    div { class: "settings-control",
                        {segmented(mode(), &[("table", "按表"), ("query", "按查询")], "导出方式", mode)}
                    }
                }
                if mode() == "table" {
                    div { class: "settings-row",
                        label { class: "settings-label", r#for: "export-table", "表名" }
                        div { class: "settings-control",
                            FormInput {
                                id: Some("export-table".to_string()),
                                class: "field settings-mono",
                                placeholder: "如 posts",
                                value: table_name(),
                                oninput: move |v: String| table_name.set(v),
                            }
                            p { class: "settings-hint", "只支持 public schema 下的用户表。" }
                        }
                    }
                } else {
                    div { class: "settings-row",
                        label { class: "settings-label", r#for: "export-query", "查询" }
                        div { class: "settings-control",
                            FormTextarea {
                                id: Some("export-query".to_string()),
                                class: "field field-textarea settings-mono",
                                rows: 4u32,
                                placeholder: "SELECT id, title FROM posts WHERE status = 'published'",
                                value: query(),
                                oninput: move |v: String| query.set(v),
                            }
                            p { class: "settings-hint", "只接受只读的 SELECT。" }
                        }
                    }
                }
                div { class: "settings-row",
                    span { class: "settings-label", "格式" }
                    div { class: "settings-control",
                        {segmented(format(), &[("csv", "CSV"), ("sql", "SQL")], "导出格式", format)}
                        p { class: "settings-hint", "SQL 格式只含 INSERT 语句，不含表结构。" }
                    }
                }
                div { class: "settings-row settings-switch-row",
                    span { class: "settings-label", "包含列名" }
                    p { class: "settings-hint", "CSV 写入表头，SQL 的 INSERT 写出列清单。" }
                    ToggleSwitch {
                        checked: include_columns(),
                        aria_label: "包含列名".to_string(),
                        ontoggle: move |_| include_columns.toggle(),
                    }
                }
            }
            div { class: "sys-export-foot",
                p { "流式导出，大表不会占满内存。" }
                button {
                    class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                    r#type: "button",
                    disabled: !ready,
                    onclick: do_export,
                    Icon { name: IconName::Download, size: 15 }
                    "导出并下载"
                }
            }
        }
    }
}

/// 简易 URL 编码（避免引入新依赖；仅编码导出参数里的特殊字符）。
/// 仅在 WASM 前端的导出按钮里用。
#[cfg(target_arch = "wasm32")]
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
