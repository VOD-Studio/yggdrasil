//! 日志页的日志流：历史加载、SSE 实时追加、向上翻页与滚动跟随。
use crate::api::logs::get_logs;
use crate::components::skeletons::{atoms::SkeletonBox, delayed_skeleton::DelayedSkeleton};
use crate::components::ui::{button_class, ButtonSize, ButtonVariant};
use crate::models::log::LogEntry;
use crate::utils::time::to_local;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// 首屏历史加载条数。
const INITIAL_LIMIT: i32 = 200;
/// 「加载更早」单次翻页条数。
const OLDER_PAGE_LIMIT: i32 = 200;
/// 判定「贴底」的阈值（px）：距底小于该值时新行自动跟随滚动。
#[cfg(target_arch = "wasm32")]
const STICK_THRESHOLD_PX: i32 = 48;
/// 列表缓冲上限（行）：超出时丢弃最旧行，保护 DOM 规模。
#[cfg(target_arch = "wasm32")]
const MAX_BUFFER_ROWS: usize = 2000;
/// 触发上限时一次性裁掉的行数（留缓冲，避免每行都触发 drain）。
#[cfg(target_arch = "wasm32")]
const BUFFER_TRIM: usize = 500;

/// 日志行：entry（历史 id>0 / 实时 id=0）或 gap 分隔提示。
///
/// `key` 是客户端单调序号（SSE 实时行 id 恒 0，不能用作渲染 key）。
#[derive(Clone)]
struct LogRow {
    key: u64,
    kind: LogRowKind,
}

#[derive(Clone)]
enum LogRowKind {
    Entry(LogEntry),
    /// broadcast lagged 分隔提示；Some(n) 为服务端报告的丢弃条数。
    // 只在浏览器端的 SSE 回调里构造，服务端构建下会报 dead_code。
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Gap(Option<u64>),
}

/// 挂载时刻的筛选快照：面板按筛选 remount，历史查询、翻页与 SSE 都用同一份。
#[derive(Clone)]
struct StreamFilter {
    levels: Vec<String>,
    target: Option<String>,
    query: String,
}

impl StreamFilter {
    /// 空关键字按「无关键字」传给服务端。
    fn query_opt(&self) -> Option<String> {
        (!self.query.is_empty()).then(|| self.query.clone())
    }
}

/// 分配客户端单调行 key（SSE 实时行 id 恒 0，不能用作渲染 key）。
fn next_row_key(counter: StoredValue<u64>) -> u64 {
    counter
        .try_update_value(|next| {
            let key = *next;
            *next += 1;
            key
        })
        .unwrap_or(0)
}

/// 实时日志流面板：历史加载 + SSE 追加 + 向上翻页 + 滚动跟随。
///
/// 父组件按筛选变化重建本组件（props 均为挂载时刻快照）：卸载时 `on_cleanup`
/// 关闭 EventSource，挂载时重新拉历史并重连。`follow` 是父组件的开关，
/// `connected` / `dropped` 是与父组件共享的信号。
#[component]
pub(super) fn LogsStreamPane(
    levels: Vec<String>,
    target: Option<String>,
    query: String,
    #[prop(into)] follow: Signal<bool>,
    connected: RwSignal<bool>,
    dropped: RwSignal<u64>,
    #[prop(into)] on_pick_target: Callback<String>,
) -> impl IntoView {
    let filter = StoredValue::new(StreamFilter {
        levels,
        target,
        query,
    });
    let rows = RwSignal::new(Vec::<LogRow>::new());
    let next_cursor = RwSignal::new(None::<i64>);
    let initial_loaded = RwSignal::new(false);
    let load_error = RwSignal::new(false);
    let stream_error = RwSignal::new(false);
    let loading_older = RwSignal::new(false);
    // 贴底状态：用户上滑离开底部即暂停跟随，浮出「回到底部」。
    let stick = RwSignal::new(true);
    // prepend 后待补偿的滚动基准（prepend 前的 scrollHeight）。
    let prepend_base = RwSignal::new(None::<i32>);
    let row_key = StoredValue::new(1_u64);
    let scroll_ref = NodeRef::<leptos::html::Div>::new();

    // —— 生命周期：挂载拉历史 + 连 SSE；卸载 close EventSource ——
    // 句柄含 JS 对象与闭包（非 Send），放进 local 存储；Drop 时关闭连接并释放回调。
    #[cfg(target_arch = "wasm32")]
    let es_slot = StoredValue::<Option<LogStreamHandle>, LocalStorage>::new_local(None);
    #[cfg(target_arch = "wasm32")]
    on_cleanup(move || {
        let _ = es_slot.try_update_value(Option::take);
    });

    // 无追踪信号，只在挂载后运行一次。
    Effect::new(move |_| {
        let snapshot = filter.get_value();
        spawn_local(async move {
            // remount 期间状态点回落为「未连接」，待 SSE open 后再置绿。
            connected.try_set(false);
            let result = get_logs(
                snapshot.levels.clone(),
                snapshot.target.clone(),
                snapshot.query_opt(),
                None,
                INITIAL_LIMIT,
            )
            .await;
            // 等待期间筛选变更会卸载本面板（信号已释放）：直接收尾，不再建连。
            if rows.try_with_untracked(|_| ()).is_none() {
                return;
            }
            match result {
                Ok(page) => {
                    dropped.try_set(page.dropped);
                    next_cursor.try_set(page.next_cursor);
                    let mut entries = page.entries;
                    entries.sort_by_key(|e| e.id);
                    rows.try_set(
                        entries
                            .into_iter()
                            .map(|e| LogRow {
                                key: next_row_key(row_key),
                                kind: LogRowKind::Entry(e),
                            })
                            .collect(),
                    );
                }
                Err(_) => {
                    load_error.try_set(true);
                }
            }
            initial_loaded.try_set(true);
            // 历史落地后连 SSE 实时流；失败仅提示，不影响历史查看。
            #[cfg(target_arch = "wasm32")]
            if open_log_stream(&snapshot, es_slot, rows, row_key, connected).is_err() {
                stream_error.try_set(true);
            }
        });
    });

    // 滚动跟随 / prepend 位移补偿：渲染后执行的真副作用（DOM 写入）。
    // 订阅 rows / follow / stick / prepend_base——开启「自动跟随」会立即贴底。
    Effect::new(move |_| {
        rows.with(Vec::len);
        let follow_now = follow.get();
        let stick_now = stick.get();
        let pending = prepend_base.get();
        #[cfg(target_arch = "wasm32")]
        if let Some(el) = scroll_ref.get() {
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
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (follow_now, stick_now, pending);
    });

    // 「加载更早」：游标来自服务端 LogsPage.next_cursor。
    let load_older = move |_| {
        let Some(cursor) = next_cursor.get_untracked() else {
            return;
        };
        if loading_older.get_untracked() {
            return;
        }
        loading_older.set(true);
        #[cfg(target_arch = "wasm32")]
        let base = scroll_ref
            .get_untracked()
            .map(|el| el.scroll_height())
            .unwrap_or(0);
        #[cfg(not(target_arch = "wasm32"))]
        let base = 0;
        let snapshot = filter.get_value();
        spawn_local(async move {
            if let Ok(page) = get_logs(
                snapshot.levels.clone(),
                snapshot.target.clone(),
                snapshot.query_opt(),
                Some(cursor),
                OLDER_PAGE_LIMIT,
            )
            .await
            {
                next_cursor.try_set(page.next_cursor);
                let mut older = page.entries;
                older.sort_by_key(|e| e.id);
                // prepend 重叠去重：仅对历史行（id != 0）。
                let merged = rows
                    .try_with_untracked(|current| {
                        let mut merged: Vec<LogRow> = older
                            .into_iter()
                            .filter(|e| {
                                e.id == 0
                                    || !current.iter().any(|r| {
                                        matches!(&r.kind, LogRowKind::Entry(cur) if cur.id == e.id)
                                    })
                            })
                            .map(|e| LogRow {
                                key: next_row_key(row_key),
                                kind: LogRowKind::Entry(e),
                            })
                            .collect();
                        if merged.is_empty() {
                            return None;
                        }
                        merged.extend(current.iter().cloned());
                        Some(merged)
                    })
                    .flatten();
                if let Some(merged) = merged {
                    rows.try_set(merged);
                    prepend_base.try_set(Some(base));
                }
            }
            loading_older.try_set(false);
        });
    };

    view! {
        <section class="admin-panel logs-pane" aria-label="日志">
            // SSE 建连失败提示条（历史仍可用）
            {move || {
                stream_error
                    .get()
                    .then(|| {
                        view! {
                            <p class="logs-banner" role="status">
                                "实时流连接失败，只显示历史快照。"
                            </p>
                        }
                    })
            }}
            // 日志滚动区（等宽字体行列表，新行自底部进入）
            <div
                node_ref=scroll_ref
                data-vt-scroll="admin-logs"
                class="logs-scroll"
                on:scroll=move |_| {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(el) = scroll_ref.get_untracked() {
                        let near_bottom = el.scroll_height() - el.scroll_top() - el.client_height()
                            < STICK_THRESHOLD_PX;
                        if near_bottom != stick.get_untracked() {
                            stick.set(near_bottom);
                        }
                    }
                }
            >
                // 向上翻页：加载更早
                {move || {
                    next_cursor
                        .get()
                        .is_some()
                        .then(|| {
                            view! {
                                <div class="logs-older">
                                    <button
                                        type="button"
                                        class=button_class(ButtonVariant::Ghost, ButtonSize::Xs)
                                        disabled=move || loading_older.get()
                                        on:click=load_older
                                    >
                                        {move || if loading_older.get() { "加载中…" } else { "加载更早" }}
                                    </button>
                                </div>
                            }
                        })
                }}
                // 首屏加载 / 错误 / 空态
                {move || {
                    let empty = rows.with(Vec::is_empty);
                    if !initial_loaded.get() {
                        view! {
                            <DelayedSkeleton>
                                <div class="logs-skeleton">
                                    <SkeletonBox class="h-4 w-[88%] rounded"/>
                                    <SkeletonBox class="h-4 w-[72%] rounded"/>
                                    <SkeletonBox class="h-4 w-[81%] rounded"/>
                                    <SkeletonBox class="h-4 w-[64%] rounded"/>
                                    <SkeletonBox class="h-4 w-[77%] rounded"/>
                                </div>
                            </DelayedSkeleton>
                        }
                            .into_any()
                    } else if load_error.get() && empty {
                        view! {
                            <p class="logs-empty" data-error="true">
                                "历史日志加载失败，请调整筛选后重试。"
                            </p>
                        }
                            .into_any()
                    } else if empty {
                        view! { <p class="logs-empty">"当前筛选条件下没有日志。"</p> }.into_any()
                    } else {
                        ().into_any()
                    }
                }}
                <For each=move || rows.get() key=|row| row.key let:row>
                    {render_row(row, on_pick_target)}
                </For>
            </div>
            // 「回到底部」浮动按钮（离开底部时浮出）
            {move || {
                (!stick.get())
                    .then(|| {
                        view! {
                            <button
                                type="button"
                                class="logs-to-bottom"
                                on:click=move |_| {
                                    #[cfg(target_arch = "wasm32")]
                                    if let Some(el) = scroll_ref.get_untracked() {
                                        el.set_scroll_top(el.scroll_height());
                                    }
                                    stick.set(true);
                                }
                            >
                                "回到底部"
                            </button>
                        }
                    })
            }}
        </section>
    }
}

/// 渲染一行日志或 gap 分隔提示。
fn render_row(row: LogRow, on_pick_target: Callback<String>) -> AnyView {
    match row.kind {
        LogRowKind::Gap(missed) => {
            let gap_text = match missed {
                Some(n) => format!("…部分日志已跳过（{n} 条）…"),
                None => "…部分日志已跳过…".to_string(),
            };
            view! { <div class="logs-gap">{gap_text}</div> }.into_any()
        }
        LogRowKind::Entry(entry) => {
            let time_hms = to_local(entry.ts).format("%H:%M:%S%.3f").to_string();
            let time_full = to_local(entry.ts).to_rfc3339();
            let LogEntry {
                level,
                target,
                message,
                ..
            } = entry;
            let target_name = target.clone();
            let row_level = level.clone();
            view! {
                <div class="logs-row" data-level=row_level>
                    <time title=time_full>{time_hms}</time>
                    <span class="logs-level">{level}</span>
                    <button
                        type="button"
                        class="logs-target"
                        title="只看这个 target"
                        on:click=move |_| on_pick_target.run(target_name.clone())
                    >
                        {target}
                    </button>
                    <span class="logs-message">{message}</span>
                </div>
            }
            .into_any()
        }
    }
}

/// SSE 连接与回调闭包同生共死：drop 时关闭连接，闭包字段随之释放回调表槽位。
#[cfg(target_arch = "wasm32")]
struct LogStreamHandle {
    es: web_sys::EventSource,
    _on_open: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
    _on_log: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MessageEvent)>,
    _on_gap: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MessageEvent)>,
    _on_error: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for LogStreamHandle {
    fn drop(&mut self) {
        self.es.close();
    }
}

/// 建立 SSE 连接消费实时日志流。
///
/// `log` 事件（JSON LogEntry，id 恒 0）追加到列表尾部；`gap` 事件
/// （broadcast lagged）插入分隔提示行；`open`/`error` 维护连接状态点。
/// 浏览器在 `error` 后自动重连（重连丢段由服务端 `gap` 通知），
/// 故 error 只标记断开、不 close。连接与回调闭包存入 `es_slot` 的 `LogStreamHandle`，
/// 面板卸载（`on_cleanup`）或重连替换时 drop 句柄即关闭连接并释放闭包；
/// 回调里的信号写入一律 `try_*`，卸载后的迟到事件不会触碰已释放的信号。
#[cfg(target_arch = "wasm32")]
fn open_log_stream(
    filter: &StreamFilter,
    es_slot: StoredValue<Option<LogStreamHandle>, LocalStorage>,
    rows: RwSignal<Vec<LogRow>>,
    row_key: StoredValue<u64>,
    connected: RwSignal<bool>,
) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use web_sys::{EventSource, MessageEvent};

    let mut url = format!("/api/logs/stream?levels={}", filter.levels.join(","));
    if let Some(t) = &filter.target {
        if !t.is_empty() {
            url.push_str("&target=");
            url.push_str(&String::from(js_sys::encode_uri_component(t)));
        }
    }
    if !filter.query.is_empty() {
        url.push_str("&q=");
        url.push_str(&String::from(js_sys::encode_uri_component(&filter.query)));
    }
    let es = EventSource::new(&url)?;
    // 注册失败的早退路径也要关闭半接线的连接。
    let close_on_err = |e: wasm_bindgen::JsValue| {
        es.close();
        e
    };

    // open → 已连接
    let on_open = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        connected.try_set(true);
    });
    es.add_event_listener_with_callback("open", on_open.as_ref().unchecked_ref())
        .map_err(close_on_err)?;

    // log 事件 → 追加（实时行 id 恒 0：到达即追加，不去重、不作游标）
    let on_log = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        if let Some(s) = e.data().as_string() {
            if let Ok(entry) = serde_json::from_str::<LogEntry>(&s) {
                rows.try_update(|list| {
                    list.push(LogRow {
                        key: next_row_key(row_key),
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
    es.add_event_listener_with_callback("log", on_log.as_ref().unchecked_ref())
        .map_err(close_on_err)?;

    // gap 事件 → 分隔提示行（broadcast lagged；data 为文本 "missed N log events"）
    let on_gap = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        // data 形如 "missed N log events"，解析出 N 用于提示；解析失败则省略计数。
        let missed = e
            .data()
            .as_string()
            .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok());
        rows.try_update(|list| {
            list.push(LogRow {
                key: next_row_key(row_key),
                kind: LogRowKind::Gap(missed),
            });
        });
    });
    es.add_event_listener_with_callback("gap", on_gap.as_ref().unchecked_ref())
        .map_err(close_on_err)?;

    // error → 标记断开（浏览器自动重连，open 时恢复）
    let on_error = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        connected.try_set(false);
    });
    es.add_event_listener_with_callback("error", on_error.as_ref().unchecked_ref())
        .map_err(close_on_err)?;

    // 面板已卸载（槽位已释放）时 try_set_value 会把句柄原样退回，随即 drop 关闭连接。
    es_slot.try_set_value(Some(LogStreamHandle {
        es,
        _on_open: on_open,
        _on_log: on_log,
        _on_gap: on_gap,
        _on_error: on_error,
    }));
    Ok(())
}
