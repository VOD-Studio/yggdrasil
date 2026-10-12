//! stream behavior of the log page.
#[cfg(target_arch = "wasm32")]
use crate::api::logs::get_logs;
use crate::components::skeletons::{atoms::SkeletonBox, delayed_skeleton::DelayedSkeleton};
use crate::components::ui::{BADGE_BASE, BTN_GHOST};
use crate::models::log::LogEntry;
use dioxus::prelude::*;
/// 首屏历史加载条数。
/// 以下常量与 LogRowKind 仅在 wasm32 门控块内构造（SSR 不加载数据），
/// server target 会报 dead_code，按本文件组件同款 cfg_attr 放行。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const INITIAL_LIMIT: i32 = 200;
/// 「加载更早」单次翻页条数。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const OLDER_PAGE_LIMIT: i32 = 200;
/// 判定「贴底」的阈值（px）：距底小于该值时新行自动跟随滚动。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const STICK_THRESHOLD_PX: i32 = 48;
/// 列表缓冲上限（行）：超出时丢弃最旧行，保护 DOM 规模。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const MAX_BUFFER_ROWS: usize = 2000;
/// 触发上限时一次性裁掉的行数（留缓冲，避免每行都触发 drain）。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const BUFFER_TRIM: usize = 500;

/// LogsStreamPane 实例计数器（滚动容器 id 跨实例全局唯一，同 FormSelect 模式）。
static LOG_STREAM_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 日志行：entry（历史 id>0 / 实时 id=0）或 gap 分隔提示。
///
/// `key` 是客户端单调序号（SSE 实时行 id 恒 0，不能用作渲染 key）。
#[derive(Clone)]
struct LogRow {
    key: u64,
    kind: LogRowKind,
}

#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
enum LogRowKind {
    Entry(LogEntry),
    /// broadcast lagged 分隔提示；Some(n) 为服务端报告的丢弃条数。
    Gap(Option<u64>),
}

/// 实时日志流面板：历史加载 + SSE 追加 + 向上翻页 + 滚动跟随。
///
/// 以 `filter_key` remount 重建（props 均为挂载时刻快照）；`follow` /
/// `connected` / `dropped` 是与父组件共享的响应式信号。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn LogsStreamPane(
    levels: Vec<String>,
    target: Option<String>,
    query: String,
    follow: Signal<bool>,
    connected: Signal<bool>,
    dropped: Signal<u64>,
    on_pick_target: Callback<String>,
) -> Element {
    let mut rows: Signal<Vec<LogRow>> = use_signal(Vec::new);
    let mut next_cursor: Signal<Option<i64>> = use_signal(|| None);
    let mut initial_loaded: Signal<bool> = use_signal(|| false);
    let mut load_error: Signal<bool> = use_signal(|| false);
    let mut stream_error: Signal<bool> = use_signal(|| false);
    let mut loading_older: Signal<bool> = use_signal(|| false);
    // 贴底状态：用户上滑离开底部即暂停跟随，浮出「回到底部」。
    let mut stick: Signal<bool> = use_signal(|| true);
    // prepend 后待补偿的滚动基准（prepend 前的 scrollHeight）。
    let mut prepend_base: Signal<Option<i32>> = use_signal(|| None);

    let scroll_id = use_hook(|| {
        format!(
            "logs-stream-{}",
            LOG_STREAM_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        )
    });

    // —— WASM-only 生命周期：挂载拉历史 + 连 SSE；卸载 close EventSource ——
    #[cfg(target_arch = "wasm32")]
    let es_slot: std::rc::Rc<std::cell::RefCell<Option<web_sys::EventSource>>> =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(None)));
    #[cfg(target_arch = "wasm32")]
    let row_key: std::rc::Rc<std::cell::Cell<u64>> =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(1)));

    // 挂载 effect 是 FnMut：spawn 内用的非 Copy 值必须在闭包体内 clone，
    // 否则把捕获变量 move 进 async 块会让 effect 闭包退化为 FnOnce。
    let levels_for_load = levels.clone();
    let target_for_load = target.clone();
    let query_for_load = query.clone();
    // move 闭包按值捕获，es_slot/row_key 先克隆出 effect 专用副本，
    // 保留原件供下方 use_drop 使用。
    #[cfg(target_arch = "wasm32")]
    let es_slot_for_effect = es_slot.clone();
    #[cfg(target_arch = "wasm32")]
    let row_key_for_effect = row_key.clone();
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        {
            let levels = levels_for_load.clone();
            let target = target_for_load.clone();
            let query = query_for_load.clone();
            let es_slot = es_slot_for_effect.clone();
            let row_key = row_key_for_effect.clone();
            spawn(async move {
                // remount 期间状态点回落为「未连接」，待 SSE open 后再置绿。
                connected.set(false);
                let q = if query.is_empty() {
                    None
                } else {
                    Some(query.clone())
                };
                match get_logs(levels.clone(), target.clone(), q, None, INITIAL_LIMIT).await {
                    Ok(page) => {
                        dropped.set(page.dropped);
                        next_cursor.set(page.next_cursor);
                        let mut entries = page.entries;
                        entries.sort_by_key(|e| e.id);
                        rows.set(
                            entries
                                .into_iter()
                                .map(|e| LogRow {
                                    key: next_row_key(&row_key),
                                    kind: LogRowKind::Entry(e),
                                })
                                .collect(),
                        );
                    }
                    Err(_) => load_error.set(true),
                }
                initial_loaded.set(true);
                // 历史落地后连 SSE 实时流；失败仅提示，不影响历史查看。
                if open_log_stream(&levels, &target, &query, es_slot, rows, row_key, connected)
                    .is_err()
                {
                    stream_error.set(true);
                }
            });
        }
    });

    #[cfg(target_arch = "wasm32")]
    {
        let es_slot_for_drop = es_slot.clone();
        use_drop(move || {
            if let Some(es) = es_slot_for_drop.borrow_mut().take() {
                es.close();
            }
        });
    }

    // 滚动跟随 / prepend 位移补偿：渲染后执行的真副作用（DOM 写入）。
    // 订阅 rows / follow / stick / prepend_base——开启「自动跟随」会立即贴底。
    let scroll_id_for_effect = scroll_id.clone();
    use_effect(move || {
        let _row_count = rows.read().len();
        let follow_now = follow();
        let stick_now = stick();
        let pending = prepend_base();
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(el) = get_scroll_el(&scroll_id_for_effect) {
                if let Some(base) = pending {
                    // 向上 prepend 后补偿位移，保持视口锚定原内容。
                    let delta = el.scroll_height() - base;
                    if delta > 0 {
                        el.set_scroll_top(el.scroll_top() + delta);
                    }
                    prepend_base.set(None);
                } else if follow_now && stick_now {
                    el.set_scroll_top(el.scroll_height());
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (follow_now, stick_now, pending);
    });

    #[cfg(target_arch = "wasm32")]
    let scroll_id_for_scroll = scroll_id.clone();
    #[cfg(target_arch = "wasm32")]
    let scroll_id_for_older = scroll_id.clone();
    #[cfg(target_arch = "wasm32")]
    let scroll_id_for_back = scroll_id.clone();

    // 「加载更早」onclick 是 FnMut：spawn 内用的非 Copy 值在闭包体内 clone。
    #[cfg(target_arch = "wasm32")]
    let levels_for_older = levels.clone();
    #[cfg(target_arch = "wasm32")]
    let target_for_older = target.clone();
    #[cfg(target_arch = "wasm32")]
    let query_for_older = query.clone();

    let rows_snapshot = rows();

    rsx! {
        div { class: "relative flex-1 min-h-0 flex flex-col bg-[var(--color-paper-entry)]/40 rounded-2xl border border-[var(--color-paper-border)]/70 shadow-xs overflow-hidden",
            // SSE 建连失败提示条（历史仍可用）
            if stream_error() {
                div { class: "flex-shrink-0 px-4 py-2 text-xs text-amber-600 dark:text-amber-400 border-b border-[var(--color-paper-border)]/40 bg-amber-500/5",
                    "实时流连接失败，仅显示历史快照"
                }
            }

            // 日志滚动区（等宽字体行列表，新行自底部进入）
            div {
                id: "{scroll_id}",
                "data-vt-scroll": "admin-logs",
                class: "flex-1 min-h-0 overflow-y-auto overscroll-contain py-1 font-mono text-xs leading-5",
                onscroll: move |_| {
                    #[cfg(target_arch = "wasm32")]
                    {
                        if let Some(el) = get_scroll_el(&scroll_id_for_scroll) {
                            let near_bottom = el.scroll_height()
                                - el.scroll_top()
                                - el.client_height()
                                < STICK_THRESHOLD_PX;
                            if near_bottom != *stick.peek() {
                                stick.set(near_bottom);
                            }
                        }
                    }
                },

                // 向上翻页：加载更早（游标来自服务端 LogsPage.next_cursor）
                if next_cursor().is_some() {
                    div { class: "flex justify-center py-1.5 border-b border-[var(--color-paper-border)]/30 mb-1",
                        button {
                            r#type: "button",
                            class: "{BTN_GHOST} font-sans",
                            disabled: loading_older(),
                            onclick: move |_| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    let Some(cursor) = *next_cursor.peek() else { return };
                                    if *loading_older.peek() {
                                        return;
                                    }
                                    loading_older.set(true);
                                    let base = get_scroll_el(&scroll_id_for_older)
                                        .map(|el| el.scroll_height())
                                        .unwrap_or(0);
                                    let levels = levels_for_older.clone();
                                    let target = target_for_older.clone();
                                    let q = if query_for_older.is_empty() {
                                        None
                                    } else {
                                        Some(query_for_older.clone())
                                    };
                                    let row_key = row_key.clone();
                                    spawn(async move {
                                        if let Ok(page) = get_logs(
                                            levels, target, q, Some(cursor), OLDER_PAGE_LIMIT,
                                        )
                                        .await
                                        {
                                            next_cursor.set(page.next_cursor);
                                            let mut older = page.entries;
                                            older.sort_by_key(|e| e.id);
                                            // prepend 重叠去重：仅对历史行（id != 0）。
                                            older.retain(|e| {
                                                e.id == 0
                                                    || !rows.read().iter().any(|r| {
                                                        matches!(&r.kind, LogRowKind::Entry(cur) if cur.id == e.id && e.id != 0)
                                                    })
                                            });
                                            if !older.is_empty() {
                                                let mut merged: Vec<LogRow> = older
                                                    .into_iter()
                                                    .map(|e| LogRow {
                                                        key: next_row_key(&row_key),
                                                        kind: LogRowKind::Entry(e),
                                                    })
                                                    .collect();
                                                merged.extend(rows.read().iter().cloned());
                                                rows.set(merged);
                                                prepend_base.set(Some(base));
                                            }
                                        }
                                        loading_older.set(false);
                                    });
                                }
                            },
                            if loading_older() { "加载中…" } else { "加载更早" }
                        }
                    }
                }

                // 首屏加载 / 错误 / 空态
                if !initial_loaded() {
                    DelayedSkeleton {
                        div { class: "px-3 py-2 space-y-2.5",
                            SkeletonBox { class: "h-4 w-[88%] rounded" }
                            SkeletonBox { class: "h-4 w-[72%] rounded" }
                            SkeletonBox { class: "h-4 w-[81%] rounded" }
                            SkeletonBox { class: "h-4 w-[64%] rounded" }
                            SkeletonBox { class: "h-4 w-[77%] rounded" }
                        }
                    }
                } else if load_error() && rows_snapshot.is_empty() {
                    div { class: "py-16 text-center text-sm font-sans text-red-500 dark:text-red-400",
                        "历史日志加载失败，请调整筛选后重试"
                    }
                } else if rows_snapshot.is_empty() {
                    div { class: "py-16 text-center text-sm font-sans text-[var(--color-paper-tertiary)]",
                        "当前筛选条件下暂无日志"
                    }
                }

                for row in rows_snapshot {
                    match row {
                        LogRow { key, kind: LogRowKind::Gap(missed) } => {
                            let gap_text = match missed {
                                Some(n) => format!("…部分日志已跳过（{n} 条）…"),
                                None => "…部分日志已跳过…".to_string(),
                            };
                            rsx! {
                                div { key: "row-{key}", class: "flex items-center gap-3 px-3 py-1 select-none",
                                    div { class: "flex-1 border-t border-dashed border-[var(--color-paper-border)]" }
                                    span { class: "text-[11px] font-sans text-[var(--color-paper-tertiary)]",
                                        "{gap_text}"
                                    }
                                    div { class: "flex-1 border-t border-dashed border-[var(--color-paper-border)]" }
                                }
                            }
                        }
                        LogRow { key, kind: LogRowKind::Entry(entry) } => {
                            let badge = level_badge_class(&entry.level);
                            let time_hms = entry.ts.format("%H:%M:%S").to_string();
                            let time_full = entry.ts.to_rfc3339();
                            let target_name = entry.target.clone();
                            rsx! {
                                div { key: "row-{key}", class: "flex items-baseline gap-3 px-3 py-1 hover:bg-[var(--color-paper-entry)]/70 transition-colors",
                                    // 时间：HH:MM:SS，hover title 显示完整时间戳
                                    span {
                                        class: "flex-shrink-0 tabular-nums text-[var(--color-paper-tertiary)]",
                                        title: "{time_full}",
                                        "{time_hms}"
                                    }
                                    // 级别 badge（Catppuccin 语义色）
                                    span { class: "{BADGE_BASE} {badge} w-16 justify-center flex-shrink-0",
                                        "{entry.level}"
                                    }
                                    // target：弱化色，点击设为 target 筛选
                                    button {
                                        r#type: "button",
                                        class: "flex-shrink-0 max-w-40 truncate text-[var(--color-paper-tertiary)] hover:text-[var(--color-paper-accent)] transition-colors cursor-pointer text-left",
                                        title: "按此 target 筛选",
                                        onclick: move |_| on_pick_target.call(target_name.clone()),
                                        "{entry.target}"
                                    }
                                    // message：pre-wrap 全量展示（boring 方案，无展开交互）
                                    span { class: "flex-1 min-w-0 whitespace-pre-wrap break-words text-[var(--color-paper-primary)]/90",
                                        "{entry.message}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 「回到底部」浮动按钮（离开底部时浮出）
            if !stick() {
                button {
                    r#type: "button",
                    class: "absolute bottom-3 right-3 z-10 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium bg-[var(--color-paper-entry)] text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] border border-[var(--color-paper-border)] shadow-md transition-all cursor-pointer",
                    onclick: move |_| {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(el) = get_scroll_el(&scroll_id_for_back) {
                            el.set_scroll_top(el.scroll_height());
                        }
                        stick.set(true);
                    },
                    "回到底部"
                }
            }
        }
    }
}

/// 级别 badge 配色（Catppuccin 语义色，沿用 StatusBadge 色板惯例）。
fn level_badge_class(level: &str) -> &'static str {
    match level {
        "ERROR" => "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300",
        "WARN" => "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
        "INFO" => "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300",
        "DEBUG" => "bg-[var(--color-paper-theme)] text-[var(--color-paper-secondary)]",
        // TRACE 最弱化：无底色，仅三级文字色。
        _ => "text-[var(--color-paper-tertiary)]",
    }
}

/// 分配客户端单调行 key（SSE 实时行 id 恒 0，不能用作渲染 key）。
#[cfg(target_arch = "wasm32")]
fn next_row_key(counter: &std::cell::Cell<u64>) -> u64 {
    let v = counter.get();
    counter.set(v + 1);
    v
}

/// 按 id 取日志滚动容器元素。
#[cfg(target_arch = "wasm32")]
fn get_scroll_el(id: &str) -> Option<web_sys::Element> {
    web_sys::window()?.document()?.get_element_by_id(id)
}

/// 建立 SSE 连接消费实时日志流。
///
/// `log` 事件（JSON LogEntry，id 恒 0）追加到列表尾部；`gap` 事件
/// （broadcast lagged）插入分隔提示行；`open`/`error` 维护连接状态点。
/// 浏览器在 `error` 后自动重连（重连丢段由服务端 `gap` 通知），
/// 故 error 只标记断开、不 close。EventSource 存入 `es_slot`，
/// 由组件 `use_drop` 统一 close。
#[cfg(target_arch = "wasm32")]
fn open_log_stream(
    levels: &[String],
    target: &Option<String>,
    query: &str,
    es_slot: std::rc::Rc<std::cell::RefCell<Option<web_sys::EventSource>>>,
    rows: Signal<Vec<LogRow>>,
    row_key: std::rc::Rc<std::cell::Cell<u64>>,
    connected: Signal<bool>,
) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use web_sys::{EventSource, MessageEvent};

    let mut url = format!("/api/logs/stream?levels={}", levels.join(","));
    if let Some(t) = target {
        if !t.is_empty() {
            url.push_str("&target=");
            url.push_str(&String::from(js_sys::encode_uri_component(t)));
        }
    }
    if !query.is_empty() {
        url.push_str("&q=");
        url.push_str(&String::from(js_sys::encode_uri_component(query)));
    }
    let es = EventSource::new(&url)?;

    // open → 已连接
    let mut connected_for_open = connected;
    let on_open = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        connected_for_open.set(true);
    });
    es.add_event_listener_with_callback("open", on_open.as_ref().unchecked_ref())?;
    on_open.forget();

    // log 事件 → 追加（实时行 id 恒 0：到达即追加，不去重、不作游标）
    let mut rows_for_log = rows;
    let row_key_for_log = row_key.clone();
    let on_log = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        if let Some(s) = e.data().as_string() {
            if let Ok(entry) = serde_json::from_str::<LogEntry>(&s) {
                rows_for_log.with_mut(|list| {
                    list.push(LogRow {
                        key: next_row_key(&row_key_for_log),
                        kind: LogRowKind::Entry(entry),
                    });
                    // 缓冲上限：超出后裁掉最旧一段，保护 DOM 规模。
                    if list.len() > MAX_BUFFER_ROWS {
                        list.drain(..BUFFER_TRIM);
                    }
                });
            }
        }
    });
    es.add_event_listener_with_callback("log", on_log.as_ref().unchecked_ref())?;
    on_log.forget();

    // gap 事件 → 分隔提示行（broadcast lagged；data 为文本 "missed N log events"）
    let mut rows_for_gap = rows;
    let row_key_for_gap = row_key.clone();
    let on_gap = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        // data 形如 "missed N log events"，解析出 N 用于提示；解析失败则省略计数。
        let missed = e
            .data()
            .as_string()
            .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok());
        rows_for_gap.with_mut(|list| {
            list.push(LogRow {
                key: next_row_key(&row_key_for_gap),
                kind: LogRowKind::Gap(missed),
            });
        });
    });
    es.add_event_listener_with_callback("gap", on_gap.as_ref().unchecked_ref())?;
    on_gap.forget();

    // error → 标记断开（浏览器自动重连，open 时恢复）
    let mut connected_for_error = connected;
    let on_error = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        connected_for_error.set(false);
    });
    es.add_event_listener_with_callback("error", on_error.as_ref().unchecked_ref())?;
    on_error.forget();

    *es_slot.borrow_mut() = Some(es);
    Ok(())
}
