//! 数据导出 tab。

use dioxus::prelude::*;

use crate::components::forms::{FormInput, FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::ui::{Checkbox, BTN_PRIMARY_SM};

/// 数据导出 tab：按表/按查询导出 SQL/CSV，走 Axum 流式下载。
#[allow(non_snake_case)]
pub(super) fn ExportTab() -> Element {
    use crate::components::ui::ADMIN_CARD_CLASS;
    // 导出模式："table" / "query"
    let mut mode = use_signal(|| "table".to_string());
    let mut table_name = use_signal(String::new);
    let mut query = use_signal(String::new);
    let mut format = use_signal(|| "csv".to_string());
    let mut include_columns = use_signal(|| true);

    // 触发下载：构造 GET /api/database/export?... URL 并打开
    let do_export = move || {
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

    rsx! {
        div { class: "space-y-4",
            div { class: "{ADMIN_CARD_CLASS} p-4 space-y-4",
                // 模式选择
                div { class: "flex items-center gap-4",
                    label { class: "flex items-center gap-2 text-sm text-paper-primary",
                        input {
                            r#type: "radio",
                            name: "export-mode",
                            checked: mode() == "table",
                            onchange: move |_| mode.set("table".to_string()),
                        }
                        "按表导出"
                    }
                    label { class: "flex items-center gap-2 text-sm text-paper-primary",
                        input {
                            r#type: "radio",
                            name: "export-mode",
                            checked: mode() == "query",
                            onchange: move |_| mode.set("query".to_string()),
                        }
                        "按查询导出"
                    }
                }

                // 表名输入
                if mode().as_str() == "table" {
                    div {
                        label { class: "block text-sm text-paper-secondary mb-1", "表名" }
                        FormInput {
                            r#type: "text",
                            placeholder: "如 posts",
                            value: table_name(),
                            mono: true,
                            oninput: move |v: String| table_name.set(v),
                        }
                        p { class: "text-xs text-paper-secondary mt-1",
                            "仅支持 public schema 下的用户表，表名需为合法标识符"
                        }
                    }
                } else {
                    // 查询输入
                    div {
                        label { class: "block text-sm text-paper-secondary mb-1",
                            "SELECT 查询（只读）"
                        }
                        textarea {
                            class: "w-full px-3 py-2 text-sm border border-paper-border rounded bg-paper-theme text-paper-primary font-mono",
                            rows: "4",
                            placeholder: "SELECT id, title FROM posts WHERE published = true",
                            value: "{query}",
                            oninput: move |e| query.set(e.value()),
                        }
                    }
                }

                // 格式 + 选项
                div { class: "flex flex-wrap items-center gap-4",
                    div { class: "flex items-center gap-2",
                        span { class: "text-sm text-paper-secondary", "格式" }
                        FormSelect {
                            trigger_class: Some(FORM_SELECT_COMPACT_CLASS),
                            value: format(),
                            options: vec![
                                                            ("csv".to_string(), "CSV"),
                                                            ("sql".to_string(), "SQL (INSERT)"),
                                                        ],
                            onchange: move |v| format.set(v),
                        }
                    }
                    label { class: "flex items-center gap-1 text-sm text-paper-secondary",
                        Checkbox {
                            checked: include_columns(),
                            onchange: move |checked: bool| include_columns.set(checked),
                        }
                        "包含列名（CSV 表头 / INSERT 列清单）"
                    }
                }

                button { class: "{BTN_PRIMARY_SM}", onclick: move |_| do_export(), "导出并下载" }
            }
            p { class: "text-xs text-paper-secondary",
                "导出走流式响应，大表不会占满内存。SQL 格式仅含 INSERT 语句（不含 DDL/schema）。"
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
