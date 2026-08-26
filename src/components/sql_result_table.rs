//! SQL 查询结果表格组件。
//!
//! 消费后端 `SqlResult`（`crate::api::database::sql_console::SqlResult`），
//! 按单元格的 `serde_json::Value` variant 渲染：
//! NULL 显示为斜体灰色字面量，布尔显示为彩色小徽章，数字右对齐 + 等宽数字，
//! 文本截断显示。长文本可通过点击行在行下方展开跨列详情区查看完整内容。
//! 表头 sticky，列宽有界（`max-width`），避免长文本撑爆横向布局。

use dioxus::prelude::*;

use crate::api::database::sql_console::SqlResult;
use crate::components::ui::ADMIN_TABLE_CLASS;

/// 一行展开详情区容器的最大高度（超出纵向滚动，避免大 jsonb 撑爆页面）。
const EXPAND_MAX_HEIGHT_CLASS: &str = "max-h-80";
/// 文本单元格的列宽上限（Tailwind 任意值，约束长文本不无限拉伸）。
const TEXT_CELL_MAX_WIDTH_CLASS: &str = "max-w-[24rem]";

/// 渲染数据行内的单个单元格（截断态）。
///
/// 按 `serde_json::Value` variant 分发样式：NULL 斜体灰、布尔彩色徽章、
/// 数字右对齐等宽、文本截断省略。类型信息直接来自后端 `col_to_json` 的编码，
/// 前端无需依赖 PG 类型名。
fn render_cell(value: &serde_json::Value) -> Element {
    use serde_json::Value;
    match value {
        Value::Null => rsx! {
            span { class: "italic text-[var(--color-paper-tertiary)]", "NULL" }
        },
        Value::Bool(true) => rsx! {
            span {
                class: "inline-flex items-center px-1.5 py-0.5 rounded text-xs font-mono",
                style: "background-color: var(--color-paper-accent-soft); color: var(--color-paper-accent);",
                "true"
            }
        },
        Value::Bool(false) => rsx! {
            span {
                class: "inline-flex items-center px-1.5 py-0.5 rounded text-xs font-mono",
                style: "background-color: rgb(254 243 199); color: rgb(180 83 9);",
                "false"
            }
        },
        Value::Number(n) => rsx! {
            span { class: "block text-right tabular-nums text-[var(--color-paper-primary)]",
                "{n}"
            }
        },
        Value::String(s) => rsx! {
            span { class: "block font-mono text-xs text-[var(--color-paper-secondary)] truncate {TEXT_CELL_MAX_WIDTH_CLASS}",
                "{s}"
            }
        },
        // Array / Object（理论不会出现，col_to_json 不产生复合类型，防御性兜底）
        other => rsx! {
            span { class: "block font-mono text-xs text-[var(--color-paper-secondary)] truncate {TEXT_CELL_MAX_WIDTH_CLASS}",
                "{other}"
            }
        },
    }
}

/// 渲染展开详情区中的「列名: 完整值」单行。
///
/// 与 `render_cell` 不同：值不做截断，长文本换行由外层展开区容器
/// （`whitespace-pre-wrap break-all` + `max-h-80 overflow-y-auto`）承载。
fn render_expanded_value(col: &str, value: &serde_json::Value) -> Element {
    use serde_json::Value;
    let display = match value {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    rsx! {
        div { key: "{col}", class: "flex gap-2 py-0.5",
            span { class: "shrink-0 font-mono text-xs text-[var(--color-paper-tertiary)] min-w-[6rem]",
                "{col}"
            }
            span { class: "font-mono text-xs text-[var(--color-paper-secondary)]", "{display}" }
        }
    }
}

/// SQL 查询结果表格。
#[derive(Props, Clone, PartialEq)]
pub struct SqlResultTableProps {
    pub result: SqlResult,
}

/// 渲染 SQL 查询结果表格。
///
/// 一次只允许展开一行（`expanded_row` 信号记录行索引）。
/// `mut` 信号仅在 WASM 端被 `.set()`；server 构建下 `.set()` 调用在 cfg 门控块内被
/// strip，故加 `cfg_attr` 抑制 server 目标的 `unused_mut` 警告。
#[component]
#[allow(non_snake_case)]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
pub fn SqlResultTable(props: SqlResultTableProps) -> Element {
    let mut expanded_row: Signal<Option<usize>> = use_signal(|| None);
    let cols_len = props.result.columns.len();
    // colspan = 列数 + 行头箭头列
    let expand_colspan = cols_len + 1;

    rsx! {
        div { class: "{ADMIN_TABLE_CLASS}",
            div { class: "overflow-auto max-h-[70vh]",
                table { class: "w-full text-sm border-collapse",
                    thead {
                        tr { class: "border-b border-[var(--color-paper-border)] sticky top-0 bg-[var(--color-paper-entry)] z-10",
                            th { class: "w-8 px-2 py-2", "" }
                            for col in props.result.columns.iter() {
                                th {
                                    key: "{col}",
                                    class: "px-4 py-2 text-left font-medium whitespace-nowrap text-[var(--color-paper-secondary)]",
                                    "{col}"
                                }
                            }
                        }
                    }
                    tbody {
                        for (row_idx, row) in props.result.rows.iter().enumerate() {
                            // 数据行：点击切换展开
                            tr {
                                key: "row-{row_idx}",
                                class: "border-b border-[var(--color-paper-border)] last:border-0 hover:bg-[var(--color-paper-accent-soft)] transition-colors cursor-pointer",
                                onclick: move |_| {
                                    let cur = expanded_row();
                                    if cur == Some(row_idx) {
                                        expanded_row.set(None);
                                    } else {
                                        expanded_row.set(Some(row_idx));
                                    }
                                },
                                td { class: "px-2 py-2 text-center text-[var(--color-paper-tertiary)] text-xs select-none",
                                    if expanded_row() == Some(row_idx) {
                                        "▾"
                                    } else {
                                        "▸"
                                    }
                                }
                                for (ci, cell) in row.iter().enumerate() {
                                    td { key: "{ci}", class: "px-4 py-2 align-top", {render_cell(cell)} }
                                }
                            }
                            // 展开详情行（跨列）
                            if expanded_row() == Some(row_idx) {
                                tr {
                                    key: "row-{row_idx}-detail",
                                    td {
                                        colspan: "{expand_colspan}",
                                        class: "px-4 py-3 bg-[var(--color-paper-theme)]",
                                        div { class: "{EXPAND_MAX_HEIGHT_CLASS} overflow-y-auto space-y-0.5 whitespace-pre-wrap break-all",
                                            for (ci, col) in props.result.columns.iter().enumerate() {
                                                {render_expanded_value(col, row.get(ci).unwrap_or(&serde_json::Value::Null))}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
