//! SQL 查询结果表格组件。
//!
//! 消费后端 `SqlResult`（`crate::api::database::sql_console::SqlResult`），
//! 按单元格的 `serde_json::Value` variant 渲染：
//! NULL 显示为斜体灰色字面量，布尔显示为彩色小徽章，数字右对齐 + 等宽数字，
//! 文本截断显示。长文本可通过点击行在行下方展开跨列详情区查看完整内容。
//! 表格沿用 `.admin-table`，在自身滚动区内吸顶；样式见 `admin-system.css`「SQL 结果」。

use leptos::prelude::*;

use crate::api::database::sql_console::SqlResult;
use crate::components::ui::{Icon, IconName};

/// 渲染数据行内的单个单元格（截断态）。
///
/// 按 `serde_json::Value` variant 分发样式：NULL 斜体灰、布尔彩色徽章、
/// 数字右对齐等宽、文本截断省略。类型信息直接来自后端 `col_to_json` 的编码，
/// 前端无需依赖 PG 类型名。
fn render_cell(value: &serde_json::Value) -> AnyView {
    use serde_json::Value;
    match value {
        Value::Null => view! { <span class="sql-null">"NULL"</span> }.into_any(),
        Value::Bool(b) => {
            let text = b.to_string();
            let value = text.clone();
            view! { <span class="sql-bool" data-value=value>{text}</span> }.into_any()
        }
        Value::Number(n) => view! { <span class="sql-num">{n.to_string()}</span> }.into_any(),
        Value::String(s) => view! { <span class="sql-text">{s.clone()}</span> }.into_any(),
        // Array / Object（理论不会出现，col_to_json 不产生复合类型，防御性兜底）
        other => view! { <span class="sql-text">{other.to_string()}</span> }.into_any(),
    }
}

/// 渲染展开详情区中的「列名: 完整值」单行。
///
/// 与 `render_cell` 不同：值不做截断，长文本在展开区内换行，超高时纵向滚动。
fn render_expanded_value(col: &str, value: &serde_json::Value) -> impl IntoView {
    use serde_json::Value;
    let (display, is_null) = match value {
        Value::Null => ("NULL".to_string(), true),
        Value::Bool(b) => (b.to_string(), false),
        Value::Number(n) => (n.to_string(), false),
        Value::String(s) => (s.clone(), false),
        other => (other.to_string(), false),
    };
    view! {
        <div class="sql-detail-row">
            <span class="sql-detail-key">{col.to_string()}</span>
            {if is_null {
                view! { <span class="sql-null">"NULL"</span> }.into_any()
            } else {
                view! { <span class="sql-detail-value">{display}</span> }.into_any()
            }}
        </div>
    }
}

/// 渲染 SQL 查询结果表格。
///
/// Props：
/// - `result`：后端返回的结果集（列名 + 每格 JSON 值）。
/// - `initial_expanded`：初始展开的行索引，缺省全部收起（图鉴预览用）。
///
/// 一次只允许展开一行（`expanded_row` 信号记录行索引）；点击已展开的行收起。
#[component]
pub fn SqlResultTable(
    result: SqlResult,
    #[prop(optional, into)] initial_expanded: MaybeProp<usize>,
) -> impl IntoView {
    let expanded_row = RwSignal::new(initial_expanded.get_untracked());
    // colspan = 列数 + 行头箭头列
    let expand_colspan = (result.columns.len() + 1).to_string();
    let SqlResult { columns, rows, .. } = result;

    let head_cells = columns
        .iter()
        .map(|col| view! { <th>{col.clone()}</th> })
        .collect_view();

    let body_rows = rows
        .into_iter()
        .enumerate()
        .map(|(row_idx, row)| {
            let cells = row
                .iter()
                .map(|cell| view! { <td>{render_cell(cell)}</td> })
                .collect_view();
            let columns = columns.clone();
            let expand_colspan = expand_colspan.clone();
            view! {
                // 数据行：点击切换展开
                <tr
                    class="sql-row"
                    data-expanded=move || {
                        if expanded_row.get() == Some(row_idx) { "true" } else { "false" }
                    }
                    on:click=move |_| {
                        let cur = expanded_row.get_untracked();
                        expanded_row.set(if cur == Some(row_idx) { None } else { Some(row_idx) });
                    }
                >
                    <td class="sql-toggle-cell">
                        <Icon name=IconName::ChevronRight size=14/>
                    </td>
                    {cells}
                </tr>
                // 展开详情行（跨列）
                {move || {
                    (expanded_row.get() == Some(row_idx))
                        .then(|| {
                            let details = columns
                                .iter()
                                .enumerate()
                                .map(|(ci, col)| {
                                    render_expanded_value(
                                        col,
                                        row.get(ci).unwrap_or(&serde_json::Value::Null),
                                    )
                                })
                                .collect_view();
                            view! {
                                <tr class="sql-detail">
                                    <td colspan=expand_colspan.clone()>
                                        <div class="sql-detail-body">{details}</div>
                                    </td>
                                </tr>
                            }
                        })
                }}
            }
        })
        .collect_view();

    view! {
        <div class="sql-result">
            <table class="admin-table">
                <thead>
                    <tr>
                        <th class="sql-toggle-cell">
                            <span class="sr-only">"展开"</span>
                        </th>
                        {head_cells}
                    </tr>
                </thead>
                <tbody>{body_rows}</tbody>
            </table>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SqlResult {
        SqlResult {
            columns: vec!["id".to_string(), "name".to_string(), "ok".to_string()],
            rows: vec![
                vec![
                    serde_json::json!(1),
                    serde_json::json!("alpha"),
                    serde_json::json!(true),
                ],
                vec![
                    serde_json::json!(2),
                    serde_json::Value::Null,
                    serde_json::json!(false),
                ],
            ],
            statement_type: "SELECT".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn renders_cells_by_value_variant_and_collapsed_rows() {
        let html = Owner::new().with(|| view! { <SqlResultTable result=sample()/> }.to_html());
        assert!(html.contains("class=\"sql-num\""), "{html}");
        assert!(html.contains("class=\"sql-null\""), "{html}");
        assert!(html.contains("data-value=\"true\""), "{html}");
        assert!(html.contains("data-expanded=\"false\""), "{html}");
        assert!(!html.contains("sql-detail"), "{html}");
    }

    #[test]
    fn initial_expanded_row_renders_spanning_detail() {
        let html = Owner::new()
            .with(|| view! { <SqlResultTable result=sample() initial_expanded=1/> }.to_html());
        assert!(html.contains("data-expanded=\"true\""), "{html}");
        // 3 列 + 行头箭头列
        assert!(html.contains("colspan=\"4\""), "{html}");
        assert!(html.contains("sql-detail-key"), "{html}");
    }
}
