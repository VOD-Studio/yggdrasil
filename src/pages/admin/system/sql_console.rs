//! SQL 控制台 tab。

use dioxus::prelude::*;

use crate::components::ui::{Checkbox, LoadingButton};

/// SQL 控制台 tab：CodeMirror 编辑器（SQL 高亮/补全/Vim）+ 4 道护栏 + 结果表 + EXPLAIN。
///
/// 护栏 4（前端二次确认）：提交写操作前弹窗确认。
#[allow(non_snake_case)]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn SqlConsoleTab() -> Element {
    #[cfg(target_arch = "wasm32")]
    use crate::api::database::schema::get_db_schema;
    use crate::api::database::sql_console::SqlResult;
    #[cfg(target_arch = "wasm32")]
    use crate::api::database::sql_console::{execute_sql, ExecuteSqlOpts};
    #[cfg(target_arch = "wasm32")]
    use crate::bridges::codemirror;
    use crate::components::sql_result_table::SqlResultTable;
    use crate::components::ui::ADMIN_TABLE_CLASS;
    // use_resolved_theme 两种构建都用：resolved() 在 wasm 块内消费，非 wasm 仅引用避免警告。
    use crate::theme::use_resolved_theme;
    #[cfg(target_arch = "wasm32")]
    use crate::theme::ResolvedTheme;

    let resolved = use_resolved_theme();
    let sql_text = use_signal(String::new);
    let mut result = use_signal(|| Option::<SqlResult>::None);
    let mut error = use_signal(|| Option::<String>::None);
    let mut running = use_signal(|| false);
    // 选项 toggles
    let mut with_explain = use_signal(|| false);
    let mut allow_multi = use_signal(|| false);
    let mut confirm_dangerous = use_signal(|| false);
    // resolved/sql_text 仅在 wasm32 块内使用；server 构建时显式引用避免 unused 警告。
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (&resolved, &sql_text);

    // CodeMirror 实例句柄（仅 WASM）。
    #[cfg(target_arch = "wasm32")]
    let mut editor_handle: Signal<Option<codemirror::EditorHandle>> = use_signal(|| None);

    // 执行 SQL 的核心逻辑：抽成独立闭包，按钮 onclick 与 Ctrl+Enter 快捷键共用。
    // 所有捕获都是 Copy 的 Signal，故每次调用读取最新值，闭包本身是 Fn 可重复调用。
    // 用两个独立闭包（execute_for_editor / run_sql）避免 move 单一所有权冲突——
    // 它们捕获相同的 Copy signal，行为完全等价。
    #[cfg(target_arch = "wasm32")]
    let mut execute_for_editor = move || {
        running.set(true);
        error.set(None);
        let sql = sql_text.read().clone();
        let opts = ExecuteSqlOpts {
            allow_multi: allow_multi(),
            confirm_dangerous: confirm_dangerous(),
            with_explain: with_explain(),
        };
        // 护栏 4：写操作前端二次确认（简单判断：含 UPDATE/DELETE/INSERT/ALTER/DROP/TRUNCATE/CREATE 关键词）
        let lower = sql.to_lowercase();
        let looks_write = [
            "update ",
            "delete ",
            "insert ",
            "alter ",
            "drop ",
            "truncate ",
            "create ",
        ]
        .iter()
        .any(|k| lower.contains(k));
        if looks_write && !confirm_dangerous() {
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
        spawn(async move {
            match execute_sql(sql, opts).await {
                Ok(r) => result.set(Some(r)),
                Err(e) => error.set(Some(e.to_string())),
            }
            running.set(false);
        });
    };

    // 捕获当前 scope id：Ctrl+Enter 从 CodeMirror 的 JS 事件处理中触发时，
    // dioxus scope stack 为空，spawn() 内部 current_scope_id().unwrap() 会 panic。
    // 在 effect 里用 Runtime::in_scope 重建 scope 上下文，保证 spawn 找得到 origin。
    #[cfg(target_arch = "wasm32")]
    let scope_id = dioxus::core::Runtime::current()
        .try_current_scope_id()
        .unwrap_or(dioxus::core::ScopeId::ROOT);

    // 初始化 CodeMirror + 拉取 schema 注入补全。仅 WASM。
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::closure::Closure;
        use_effect(move || {
            if editor_handle.read().is_some() {
                return;
            }
            let mut text = sql_text;
            let on_change = Closure::new(move |v: String| {
                text.set(v);
            });
            let on_ready = Closure::new(|| {});
            // Ctrl/Cmd+Enter 触发执行（与按钮共用同一套资源/护栏逻辑）。
            // 从 CodeMirror JS 事件触发时无 dioxus scope，必须用 in_scope 重建。
            // 刻意用闭包 `|| execute_for_editor()` 而非直接传 `execute_for_editor`：
            // 后者会 move 闭包值进 in_scope，导致第二次快捷键触发时 use-after-move
            // （Closure::new 要求 Fn 可重复调用）。闭包包装走借用调用，每次都读最新值。
            #[allow(clippy::redundant_closure)]
            let on_run_shortcut = Closure::new(move || {
                dioxus::core::Runtime::current().in_scope(scope_id, || execute_for_editor());
            });

            let theme_name = if resolved() == ResolvedTheme::Dark {
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
                editor_handle.set(Some(handle));
            }

            // 异步拉取 schema 注入补全
            spawn(async move {
                if let Ok(schema) = get_db_schema().await {
                    // SqlSchema 是 serde 类型：先用 serde_wasm_bindgen 转 JsValue，
                    // 再传给 CodeMirror（extern set_schema 只接受 &JsValue）。
                    if let Ok(js) = serde_wasm_bindgen::to_value(&schema) {
                        if let Some(h) = editor_handle.read().as_ref() {
                            h.instance().set_schema(&js);
                        }
                    }
                }
            });
        });

        // 主题切换（含 System 模式下系统偏好变化）时同步编辑器主题。
        // resolved 是 theme + system_dark 的派生 memo，任一变化都自动触发此 effect。
        use_effect(move || {
            let r = resolved();
            if let Some(h) = editor_handle.read().as_ref() {
                h.instance().set_theme(if r == ResolvedTheme::Dark {
                    "dark"
                } else {
                    "light"
                });
            }
        });
    }

    // 执行 SQL：按钮 onclick 用的闭包。wasm 下复制 execute_for_editor 的逻辑，
    // server 下仅复位 running（无网络层）。两处闭包捕获相同的 Copy signal。
    let mut run_sql = move || {
        running.set(true);
        error.set(None);
        #[cfg(target_arch = "wasm32")]
        {
            let sql = sql_text.read().clone();
            let opts = ExecuteSqlOpts {
                allow_multi: allow_multi(),
                confirm_dangerous: confirm_dangerous(),
                with_explain: with_explain(),
            };
            // 护栏 4：写操作前端二次确认（简单判断：含 UPDATE/DELETE/INSERT/ALTER/DROP/TRUNCATE/CREATE 关键词）
            let lower = sql.to_lowercase();
            let looks_write = [
                "update ",
                "delete ",
                "insert ",
                "alter ",
                "drop ",
                "truncate ",
                "create ",
            ]
            .iter()
            .any(|k| lower.contains(k));
            if looks_write && !confirm_dangerous() {
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
            spawn(async move {
                match execute_sql(sql, opts).await {
                    Ok(r) => result.set(Some(r)),
                    Err(e) => error.set(Some(e.to_string())),
                }
                running.set(false);
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            running.set(false);
        }
    };

    let current_result = result.read().clone();
    let current_error = error.read().clone();
    let elapsed = current_result.as_ref().map(|r| r.elapsed_ms).unwrap_or(0);
    let affected = current_result
        .as_ref()
        .map(|r| r.affected_rows)
        .unwrap_or(0);
    let stmt_type = current_result
        .as_ref()
        .map(|r| r.statement_type.clone())
        .unwrap_or_default();
    let truncated = current_result
        .as_ref()
        .map(|r| r.truncated)
        .unwrap_or(false);
    let explain = current_result.as_ref().and_then(|r| r.explain.clone());

    rsx! {
        div { class: "space-y-5",
            // 编辑器卡片：标题栏 + CodeMirror 一体化。
            div { class: "rounded-2xl overflow-hidden border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)]",
                // 标题栏
                div { class: "flex justify-between items-center px-4 py-2.5 border-b border-[var(--color-paper-border)] bg-[var(--color-paper-theme)]",
                    div { class: "flex items-center gap-2",
                        span { class: "w-2 h-2 rounded-full bg-[var(--color-paper-accent)]" }
                        span { class: "font-mono text-sm font-semibold text-[var(--color-paper-primary)]",
                            "SQL"
                        }
                    }
                    span { class: "text-xs text-[var(--color-paper-tertiary)] font-mono",
                        "⌘↵ 执行"
                    }
                }
                // CodeMirror 容器：用 flex 让 .cm-editor(flex:1) 填满整个高度，
                // 避免编辑器塌缩到内容高度、底部透出容器背景造成上下色差。
                div {
                    id: "sql-editor",
                    style: "min-height: 280px; display: flex; flex-direction: column",
                }
            }

            // 工具条：执行按钮 + 普通/危险选项分层
            div { class: "flex flex-wrap items-center gap-x-4 gap-y-3",
                LoadingButton {
                    label: "执行".to_string(),
                    loading: running(),
                    onclick: move |_| run_sql(),
                }
                // 普通选项
                label { class: "flex items-center gap-1.5 text-sm text-[var(--color-paper-secondary)] cursor-pointer",
                    Checkbox {
                        checked: with_explain(),
                        onchange: move |checked: bool| with_explain.set(checked),
                    }
                    "EXPLAIN"
                }
                label { class: "flex items-center gap-1.5 text-sm text-[var(--color-paper-secondary)] cursor-pointer",
                    Checkbox {
                        checked: allow_multi(),
                        onchange: move |checked: bool| allow_multi.set(checked),
                    }
                    "允许多语句"
                }
                // 危险选项：视觉分隔 + 红色语义
                span { class: "w-px h-4 bg-[var(--color-paper-border)] mx-1" }
                label { class: "flex items-center gap-1.5 text-sm text-red-600 dark:text-red-400 cursor-pointer",
                    Checkbox {
                        danger: true,
                        checked: confirm_dangerous(),
                        onchange: move |checked: bool| confirm_dangerous.set(checked),
                    }
                    "我了解后果（DROP/TRUNCATE/ALTER）"
                }
            }

            // 错误
            if let Some(err) = current_error {
                div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-2xl p-3 text-sm text-red-700 dark:text-red-300",
                    "{err}"
                }
            }

            // 结果摘要（徽章式）
            if current_result.is_some() {
                div { class: "flex flex-wrap items-center gap-2",
                    if !stmt_type.is_empty() {
                        span { class: "inline-flex items-center px-2.5 py-1 rounded-full bg-[var(--color-paper-accent-soft)] text-[var(--color-paper-secondary)] text-xs font-medium",
                            "{stmt_type}"
                        }
                    }
                    if affected > 0 {
                        span { class: "inline-flex items-center px-2.5 py-1 rounded-full bg-[var(--color-paper-accent-soft)] text-[var(--color-paper-secondary)] text-xs font-medium",
                            "影响 {affected} 行"
                        }
                    }
                    span { class: "inline-flex items-center px-2.5 py-1 rounded-full bg-[var(--color-paper-accent-soft)] text-[var(--color-paper-secondary)] text-xs font-medium",
                        "{elapsed}ms"
                    }
                    if truncated {
                        span { class: "inline-flex items-center px-2.5 py-1 rounded-full bg-amber-100 dark:bg-amber-900/30 text-amber-700 dark:text-amber-300 text-xs font-medium",
                            "结果超过 500 行，已截断"
                        }
                    }
                }
            }

            // 结果表格
            if let Some(res) = &current_result {
                if !res.rows.is_empty() {
                    SqlResultTable { result: res.clone() }
                } else if res.statement_type.to_uppercase().contains("SELECT") {
                    // 有结果但无行：SELECT 返回空集的友好提示
                    div { class: "{ADMIN_TABLE_CLASS} px-4 py-8 text-center text-sm text-[var(--color-paper-tertiary)]",
                        "查询成功，无返回行"
                    }
                }
            }

            // EXPLAIN 输出
            if let Some(explain) = explain {
                div { class: "{ADMIN_TABLE_CLASS}",
                    div { class: "px-4 py-2 border-b border-paper-border text-sm font-medium text-paper-primary",
                        "执行计划"
                    }
                    pre { class: "p-4 text-xs font-mono text-paper-secondary overflow-x-auto whitespace-pre m-0",
                        "{explain}"
                    }
                }
            }
        }
    }
}
