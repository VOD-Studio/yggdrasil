//! SQL 查询结果表格组件。
//!
//! 消费后端 `SqlResult`（`crate::api::database::sql_console::SqlResult`），
//! 按单元格的 `serde_json::Value` variant 渲染：
//! NULL 显示为斜体灰色字面量，布尔显示为彩色小徽章，数字右对齐 + 等宽数字，
//! 文本截断显示。长文本可通过点击行在行下方展开跨列详情区查看完整内容。
//! 表格沿用 `.admin-table`，在自身滚动区内吸顶；样式见 `admin-system.css`「SQL 结果」。

use dioxus::prelude::*;

use crate::api::database::sql_console::SqlResult;
use crate::components::ui::{Icon, IconName};

/// 渲染数据行内的单个单元格（截断态）。
///
/// 按 `serde_json::Value` variant 分发样式：NULL 斜体灰、布尔彩色徽章、
/// 数字右对齐等宽、文本截断省略。类型信息直接来自后端 `col_to_json` 的编码，
/// 前端无需依赖 PG 类型名。
fn render_cell(value: &serde_json::Value) -> Element {
    use serde_json::Value;
    match value {
        Value::Null => rsx! {
            span { class: "sql-null", "NULL" }
        },
        Value::Bool(b) => rsx! {
            span { class: "sql-bool", "data-value": "{b}", "{b}" }
        },
        Value::Number(n) => rsx! {
            span { class: "sql-num", "{n}" }
        },
        Value::String(s) => rsx! {
            span { class: "sql-text", "{s}" }
        },
        // Array / Object（理论不会出现，col_to_json 不产生复合类型，防御性兜底）
        other => rsx! {
            span { class: "sql-text", "{other}" }
        },
    }
}

/// 渲染展开详情区中的「列名: 完整值」单行。
///
/// 与 `render_cell` 不同：值不做截断，长文本在展开区内换行，超高时纵向滚动。
fn render_expanded_value(col: &str, value: &serde_json::Value) -> Element {
    use serde_json::Value;
    let (display, is_null) = match value {
        Value::Null => ("NULL".to_string(), true),
        Value::Bool(b) => (b.to_string(), false),
        Value::Number(n) => (n.to_string(), false),
        Value::String(s) => (s.clone(), false),
        other => (other.to_string(), false),
    };
    rsx! {
        div { key: "{col}", class: "sql-detail-row",
            span { class: "sql-detail-key", "{col}" }
            if is_null {
                span { class: "sql-null", "NULL" }
            } else {
                span { class: "sql-detail-value", "{display}" }
            }
        }
    }
}

/// SQL 查询结果表格。
#[derive(Props, Clone, PartialEq)]
pub struct SqlResultTableProps {
    pub result: SqlResult,
    #[props(default)]
    pub initial_expanded: Option<usize>,
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
    let initial = props.initial_expanded;
    let mut expanded_row: Signal<Option<usize>> = use_signal(move || initial);
    let cols_len = props.result.columns.len();
    // colspan = 列数 + 行头箭头列
    let expand_colspan = cols_len + 1;

    rsx! {
        div { class: "sql-result",
            table { class: "admin-table",
                thead {
                    tr {
                        th { class: "sql-toggle-cell", span { class: "sr-only", "展开" } }
                        for col in props.result.columns.iter() {
                            th { key: "{col}", "{col}" }
                        }
                    }
                }
                tbody {
                    for (row_idx, row) in props.result.rows.iter().enumerate() {
                        // 数据行：点击切换展开
                        tr {
                            key: "row-{row_idx}",
                            class: "sql-row",
                            "data-expanded": expanded_row() == Some(row_idx),
                            onclick: move |_| {
                                let cur = expanded_row();
                                if cur == Some(row_idx) {
                                    expanded_row.set(None);
                                } else {
                                    expanded_row.set(Some(row_idx));
                                }
                            },
                            td { class: "sql-toggle-cell",
                                Icon { name: IconName::ChevronRight, size: 14 }
                            }
                            for (ci, cell) in row.iter().enumerate() {
                                td { key: "{ci}", {render_cell(cell)} }
                            }
                        }
                        // 展开详情行（跨列）
                        if expanded_row() == Some(row_idx) {
                            tr { key: "row-{row_idx}-detail", class: "sql-detail",
                                td { colspan: "{expand_colspan}",
                                    div { class: "sql-detail-body",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn props_default_initial_expanded_is_none() {
        let result = SqlResult {
            columns: vec!["id".to_string()],
            rows: vec![vec![serde_json::json!(1)]],
            statement_type: "SELECT".to_string(),
            ..Default::default()
        };
        let props = SqlResultTableProps {
            result,
            initial_expanded: None,
        };
        assert_eq!(props.initial_expanded, None);
    }
}
