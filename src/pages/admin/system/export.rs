//! 导出 tab：按表或按查询流式导出 CSV / SQL。

use leptos::prelude::*;

use crate::components::forms::{FormInput, FormTextarea, ToggleSwitch};
use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, Icon, IconName, SegmentedControl, SegmentedItem,
    SegmentedKind, SegmentedSize,
};

/// 数据导出 tab：按表或按查询导出 CSV / SQL，走 Axum 流式下载。
#[component]
pub(super) fn ExportTab() -> impl IntoView {
    let mode = RwSignal::new("table".to_string());
    let table_name = RwSignal::new(String::new());
    let query = RwSignal::new(String::new());
    let file_format = RwSignal::new("csv".to_string());
    let include_columns = RwSignal::new(true);
    let is_table_mode = move || mode.with(|m| m == "table");
    let ready = move || {
        if is_table_mode() {
            !table_name.with(|t| t.trim().is_empty())
        } else {
            !query.with(|q| q.trim().is_empty())
        }
    };
    // 触发下载：构造 GET /api/database/export?... 并打开。
    let do_export = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let source = if is_table_mode() {
                format!("table:{}", table_name.get_untracked().trim())
            } else {
                format!("query:{}", query.get_untracked())
            };
            let url = format!(
                "/api/database/export?source={}&format={}&include_columns={}",
                super::urlencode(&source),
                file_format.get_untracked(),
                include_columns.get_untracked(),
            );
            if let Some(window) = web_sys::window() {
                let _ = window.open_with_url(&url);
            }
        }
    };

    view! {
        <section class="admin-panel sys-export">
            <div class="settings-rows">
                <div class="settings-row">
                    <span class="settings-label">"方式"</span>
                    <div class="settings-control">
                        <SegmentedControl
                            items=vec![
                                SegmentedItem::new("table".to_string(), "按表"),
                                SegmentedItem::new("query".to_string(), "按查询"),
                            ]
                            value=mode
                            on_change=move |v: String| mode.set(v)
                            aria_label="导出方式"
                            kind=SegmentedKind::Radio
                            size=SegmentedSize::Sm
                        />
                    </div>
                </div>
                {move || {
                    if is_table_mode() {
                        view! {
                            <div class="settings-row">
                                <label class="settings-label" for="export-table">
                                    "表名"
                                </label>
                                <div class="settings-control">
                                    <FormInput
                                        id="export-table"
                                        class="field settings-mono"
                                        placeholder="如 posts"
                                        value=table_name
                                        oninput=move |v: String| table_name.set(v)
                                    />
                                    <p class="settings-hint">"只支持 public schema 下的用户表。"</p>
                                </div>
                            </div>
                        }
                            .into_any()
                    } else {
                        view! {
                            <div class="settings-row">
                                <label class="settings-label" for="export-query">
                                    "查询"
                                </label>
                                <div class="settings-control">
                                    <FormTextarea
                                        id="export-query"
                                        class="field field-textarea settings-mono"
                                        rows=4u32
                                        placeholder="SELECT id, title FROM posts WHERE status = 'published'"
                                        value=query
                                        oninput=move |v: String| query.set(v)
                                    />
                                    <p class="settings-hint">"只接受只读的 SELECT。"</p>
                                </div>
                            </div>
                        }
                            .into_any()
                    }
                }}
                <div class="settings-row">
                    <span class="settings-label">"格式"</span>
                    <div class="settings-control">
                        <SegmentedControl
                            items=vec![
                                SegmentedItem::new("csv".to_string(), "CSV"),
                                SegmentedItem::new("sql".to_string(), "SQL"),
                            ]
                            value=file_format
                            on_change=move |v: String| file_format.set(v)
                            aria_label="导出格式"
                            kind=SegmentedKind::Radio
                            size=SegmentedSize::Sm
                        />
                        <p class="settings-hint">"SQL 格式只含 INSERT 语句，不含表结构。"</p>
                    </div>
                </div>
                <div class="settings-row settings-switch-row">
                    <span class="settings-label">"包含列名"</span>
                    <p class="settings-hint">"CSV 写入表头，SQL 的 INSERT 写出列清单。"</p>
                    <ToggleSwitch
                        checked=include_columns
                        aria_label="包含列名"
                        ontoggle=move |_| include_columns.update(|v| *v = !*v)
                    />
                </div>
            </div>
            <div class="sys-export-foot">
                <p>"流式导出，大表不会占满内存。"</p>
                <button
                    class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                    type="button"
                    disabled=move || !ready()
                    on:click=do_export
                >
                    <Icon name=IconName::Download size=15/>
                    "导出并下载"
                </button>
            </div>
        </section>
    }
}
