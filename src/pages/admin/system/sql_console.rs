//! SQL 控制台 tab：CodeMirror 编辑器（SQL 高亮、补全、Vim）、执行选项与结果。

use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::database::schema::get_db_schema;
use crate::api::database::sql_console::SqlResult;
#[cfg(target_arch = "wasm32")]
use crate::api::database::sql_console::{execute_sql, ExecuteSqlOpts};
#[cfg(target_arch = "wasm32")]
use crate::bridges::codemirror;
use crate::bridges::library::{library_ready, use_browser_library, LibraryLoadError};
// SQL 报错本身就是原因（如解析失败），不再加「执行失败」前缀。
use crate::components::sql_result_table::SqlResultTable;
use crate::components::ui::{button_class, ButtonSize, ButtonVariant, Checkbox};
use crate::theme::use_resolved_theme;
#[cfg(target_arch = "wasm32")]
use crate::theme::ResolvedTheme;
#[cfg(target_arch = "wasm32")]
use crate::utils::error::error_reason;

/// 前端判定为写操作的关键词（护栏 4：提交前二次确认）。
#[cfg(target_arch = "wasm32")]
const WRITE_KEYWORDS: [&str; 7] = [
    "update ",
    "delete ",
    "insert ",
    "alter ",
    "drop ",
    "truncate ",
    "create ",
];

/// SQL 控制台 tab：CodeMirror 编辑器（SQL 高亮/补全/Vim）+ 4 道护栏 + 结果表 + EXPLAIN。
///
/// 护栏 4（前端二次确认）：提交写操作前弹窗确认。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub(super) fn SqlConsoleTab() -> impl IntoView {
    let editor_library = use_browser_library("codemirror", || true);
    let resolved = use_resolved_theme();
    let sql_text = RwSignal::new(String::new());
    let result = RwSignal::new(None::<SqlResult>);
    let error = RwSignal::new(None::<String>);
    let running = RwSignal::new(false);
    // 选项 toggles
    let with_explain = RwSignal::new(false);
    let allow_multi = RwSignal::new(false);
    let confirm_dangerous = RwSignal::new(false);

    // CodeMirror 实例句柄（仅 WASM）：JS 句柄不是 Send，放进本地存储；
    // 卸载时丢弃即触发 Drop → destroy。
    #[cfg(target_arch = "wasm32")]
    let editor_handle: StoredValue<Option<codemirror::EditorHandle>, LocalStorage> =
        StoredValue::new_local(None);

    // 执行 SQL 的核心逻辑：执行按钮与 Ctrl/Cmd+Enter 快捷键共用。闭包只捕获 Copy 的
    // 信号，本身也是 Copy，每次调用读取最新值。入口的 `running` 检查负责防止快捷键
    // 在执行中重入（按钮禁用只挡得住点击）。
    let run_sql = move || {
        // 执行中直接忽略：按钮禁用挡不住 Ctrl/Cmd+Enter，重入检查放在共用入口。
        if running.get_untracked() || !editor_library.ready_untracked() {
            return;
        }
        running.set(true);
        error.set(None);
        #[cfg(target_arch = "wasm32")]
        {
            let sql = sql_text.get_untracked();
            let opts = ExecuteSqlOpts {
                allow_multi: allow_multi.get_untracked(),
                confirm_dangerous: confirm_dangerous.get_untracked(),
                with_explain: with_explain.get_untracked(),
            };
            // 护栏 4：写操作前端二次确认（简单判断：含 UPDATE/DELETE/INSERT/ALTER/DROP/TRUNCATE/CREATE 关键词）
            let lower = sql.to_lowercase();
            let looks_write = WRITE_KEYWORDS.iter().any(|k| lower.contains(k));
            if looks_write && !opts.confirm_dangerous {
                // 简单提示；真正的高危放行靠 confirm_dangerous 开关
                let confirmed = web_sys::window().and_then(|w| {
                    w.confirm_with_message(
                        "这是写操作（修改数据/结构），确认执行？\n\n高危操作（DROP/TRUNCATE/ALTER）还需勾选「我了解后果」。",
                    )
                    .ok()
                });
                if confirmed != Some(true) {
                    running.set(false);
                    return;
                }
            }
            leptos::task::spawn_local(async move {
                match execute_sql(sql, opts).await {
                    Ok(r) => {
                        let _ = result.try_set(Some(r));
                    }
                    Err(e) => {
                        let _ = error.try_set(Some(
                            error_reason(&e).unwrap_or_else(|| "执行失败，请稍后再试".into()),
                        ));
                    }
                }
                let _ = running.try_set(false);
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        running.set(false);
    };

    // 初始化 CodeMirror + 拉取 schema 注入补全。Effect 只在浏览器执行。
    Effect::new(move |_| {
        // 在 effect 顶层读取以便库就绪时重跑。
        let ready = library_ready(editor_library);
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::closure::Closure;

            if !ready || editor_handle.with_value(Option::is_some) {
                return;
            }
            let on_change = Closure::new(move |v: String| {
                let _ = sql_text.try_set(v);
            });
            let on_ready = Closure::new(|| {});
            // Ctrl/Cmd+Enter 触发执行（与按钮共用同一套护栏逻辑）。
            let on_run_shortcut = Closure::new(run_sql);

            let theme_name = if resolved.get_untracked() == ResolvedTheme::Dark {
                "dark"
            } else {
                "light"
            };
            let opts = codemirror::EditorOptions::new();
            opts.set_language("sql");
            opts.set_theme(theme_name);
            opts.set_vim(true);
            opts.set_on_change(&on_change);
            opts.set_on_ready(&on_ready);
            opts.set_on_run_shortcut(&on_run_shortcut);

            if let Ok(Some(inst)) = codemirror::get_module().create("sql-editor", &opts) {
                let handle =
                    codemirror::EditorHandle::new(inst, on_change, on_ready, on_run_shortcut);
                editor_handle.set_value(Some(handle));
            }

            // 异步拉取 schema 注入补全
            leptos::task::spawn_local(async move {
                if let Ok(schema) = get_db_schema().await {
                    // SqlSchema 是 serde 类型：先用 serde_wasm_bindgen 转 JsValue，
                    // 再传给 CodeMirror（extern set_schema 只接受 &JsValue）。
                    if let Ok(js) = serde_wasm_bindgen::to_value(&schema) {
                        let _ = editor_handle.try_with_value(|handle| {
                            if let Some(handle) = handle.as_ref() {
                                handle.instance().set_schema(&js);
                            }
                        });
                    }
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = ready;
    });

    // 主题切换（含 System 模式下系统偏好变化）时同步编辑器主题。
    // resolved 是 theme + system_dark 的派生 memo，任一变化都自动触发此 effect。
    Effect::new(move |_| {
        let r = resolved.get();
        #[cfg(target_arch = "wasm32")]
        editor_handle.with_value(|handle| {
            if let Some(handle) = handle.as_ref() {
                handle.instance().set_theme(if r == ResolvedTheme::Dark {
                    "dark"
                } else {
                    "light"
                });
            }
        });
    });

    on_cleanup(move || {
        #[cfg(target_arch = "wasm32")]
        let _ = editor_handle.try_set_value(None);
    });

    view! {
        <section class="admin-panel sys-sql" aria-label="SQL 编辑器">
            <LibraryLoadError library=editor_library/>
            // CodeMirror 容器：flex 让 .cm-editor(flex:1) 填满高度，避免底部透出背景。
            <div id="sql-editor" class="sys-sql-editor"></div>
            <div class="sys-sql-bar">
                <label class="sys-sql-option">
                    <Checkbox
                        checked=with_explain
                        onchange=move |checked: bool| with_explain.set(checked)
                    />
                    "附带执行计划"
                </label>
                <label class="sys-sql-option">
                    <Checkbox
                        checked=allow_multi
                        onchange=move |checked: bool| allow_multi.set(checked)
                    />
                    "允许多语句"
                </label>
                <label class="sys-sql-option" data-danger="true">
                    <Checkbox
                        danger=true
                        checked=confirm_dangerous
                        onchange=move |checked: bool| confirm_dangerous.set(checked)
                    />
                    "我了解后果（DROP / TRUNCATE / ALTER）"
                </label>
                <button
                    class=format!("{} sys-sql-run", button_class(ButtonVariant::Primary, ButtonSize::Sm))
                    type="button"
                    disabled=move || running.get() || !library_ready(editor_library)
                    on:click=move |_| run_sql()
                >
                    {move || if running.get() { "执行中…" } else { "执行" }}
                    <kbd class="kbd">"⌘↵"</kbd>
                </button>
            </div>
        </section>

        {move || {
            error.get().map(|err| view! { <p class="sys-error" role="alert">{err}</p> })
        }}

        {move || {
            result
                .get()
                .map(|res| {
                    let heading = if res.statement_type.is_empty() {
                        "结果".to_string()
                    } else {
                        res.statement_type.clone()
                    };
                    let mut summary = String::new();
                    if !res.rows.is_empty() {
                        summary.push_str(&format!("{} 行 · ", res.rows.len()));
                    }
                    if res.affected_rows > 0 {
                        summary.push_str(&format!("影响 {} 行 · ", res.affected_rows));
                    }
                    summary.push_str(&format!("{} ms", res.elapsed_ms));
                    if res.truncated {
                        summary.push_str(" · 超过 500 行，已截断");
                    }
                    let body = if !res.rows.is_empty() {
                        view! { <SqlResultTable result=res.clone()/> }.into_any()
                    } else if res.statement_type.to_uppercase().contains("SELECT") {
                        view! { <p class="sys-empty">"查询成功，没有返回行。"</p> }.into_any()
                    } else {
                        ().into_any()
                    };
                    let explain = res.explain.clone();
                    view! {
                        <section class="admin-panel sys-section sys-result">
                            <div class="sys-section-head">
                                <h2>{heading}</h2>
                                <span>{summary}</span>
                            </div>
                            {body}
                        </section>
                        {explain
                            .map(|explain| {
                                view! {
                                    <section class="admin-panel sys-section">
                                        <div class="sys-section-head">
                                            <h2>"执行计划"</h2>
                                        </div>
                                        <pre class="sys-explain">{explain}</pre>
                                    </section>
                                }
                            })}
                    }
                })
        }}
    }
}
