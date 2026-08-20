//! 数据库状态 tab。

use dioxus::prelude::*;

use crate::components::forms::{FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::LoadingButton;

use crate::utils::format_bytes;

/// 自动刷新间隔可选项（秒；None = 手动）。
const REFRESH_INTERVAL_OPTIONS: &[(Option<u32>, &str)] = &[
    (None, "手动"),
    (Some(1), "1s"),
    (Some(2), "2s"),
    (Some(5), "5s"),
    (Some(30), "30s"),
];

/// 数据库状态 tab：概览卡片 + 表清单 + 索引 Top + 活跃连接。
/// 手动刷新按钮 + 自动刷新开关（1s/2s/5s/30s/手动，默认手动）。
#[allow(non_snake_case)]
// status/error/loading 在 spawn/onclick 闭包里 .set()，仅 WASM 前端真正用到；
// server 构建里这些 set 调用都在被剥离的 #[cfg(wasm32)] 块内，故 allow unused_mut。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn DbStatusTab() -> Element {
    use crate::api::database::status::DbStatus;
    // get_db_status 只在 WASM 前端调用，server 构建时该 server function 的客户端桩不需要导入。
    #[cfg(target_arch = "wasm32")]
    use crate::api::database::status::get_db_status;

    // Signal 是 Copy，可在多个 spawn/effect 中捕获同一副本；set 走内部可变（&self）。
    let mut status = use_signal(|| Option::<DbStatus>::None);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| Option::<String>::None);
    // 自动刷新间隔（秒）；None = 手动。DB 查询有成本，最低 1s。
    let mut refresh_interval: Signal<Option<u32>> = use_signal(|| None);

    // 数据加载：WASM 前端 spawn 请求，SSR 直接结束加载。
    // 因 Signal 是 Copy，每次 spawn 各自捕获副本即可，无需共享闭包。
    let mut load_once = move || {
        loading.set(true);
        #[cfg(target_arch = "wasm32")]
        {
            spawn(async move {
                match get_db_status().await {
                    Ok(s) => {
                        status.set(Some(s));
                        error.set(None);
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                loading.set(false);
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            loading.set(false);
        }
    };

    // 首次加载
    use_effect(move || {
        load_once();
    });

    // 自动刷新：使用官方推荐模式——一个永不重建的长生命周期 loop，在每次循环
    // 内部读取 refresh_interval 的当前值，自然响应间隔切换。
    // 旧做法在闭包同步体内读 status/loading/error signal，导致这些 signal 每次
    // .set() 后都触发 use_future 重建，产生多个并发 loop（请求爆炸）。
    use_future(move || async move {
        #[cfg(target_arch = "wasm32")]
        {
            loop {
                // 每次循环读最新 interval（signal 的 Copy 语义，直接调用即可）。
                let secs = refresh_interval().unwrap_or(0);
                if secs == 0 {
                    // 手动模式：短暂 yield，让事件循环呼吸，避免忙等；
                    // 用户切换到自动模式后最多等 200ms 即响应。
                    crate::utils::time::sleep_ms(200).await;
                    continue;
                }
                crate::utils::time::sleep_ms(secs * 1000).await;
                // 二次检查：sleep 期间用户可能切回手动。
                if refresh_interval().is_none() {
                    continue;
                }
                loading.set(true);
                spawn(async move {
                    match get_db_status().await {
                        Ok(s) => {
                            status.set(Some(s));
                            error.set(None);
                        }
                        Err(e) => error.set(Some(e.to_string())),
                    }
                    loading.set(false);
                });
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (status, loading, error, refresh_interval);
        }
    });

    // Option<DbStatus> 非 Copy，读出来克隆一份供 rsx 消费。
    let current = status.read().clone();

    rsx! {
        div { class: "space-y-6",
            // 工具栏：刷新按钮 + 自动刷新开关
            div { class: "flex items-center justify-between gap-4",
                LoadingButton {
                    label: "刷新数据".to_string(),
                    loading: loading(),
                    variant: "sm",
                    onclick: move |_| {
                        loading.set(true);
                        #[cfg(target_arch = "wasm32")]
                        {
                            spawn(async move {
                                match get_db_status().await {
                                    Ok(s) => {
                                        status.set(Some(s));
                                        error.set(None);
                                    }
                                    Err(e) => error.set(Some(e.to_string())),
                                }
                                loading.set(false);
                            });
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            loading.set(false);
                        }
                    },
                }
                div { class: "inline-flex items-center gap-2 px-3 py-1.5 rounded-full bg-[var(--color-paper-entry)]/60 border border-[var(--color-paper-border)]/60 shadow-2xs",
                    svg {
                        class: "w-3.5 h-3.5 text-[var(--color-paper-tertiary)]",
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "12", cy: "12", r: "10" }
                        polyline { points: "12 6 12 12 16 14" }
                    }
                    span { class: "text-xs font-medium text-[var(--color-paper-secondary)]", "自动刷新" }
                    FormSelect {
                        trigger_class: Some(FORM_SELECT_COMPACT_CLASS),
                        value: refresh_interval(),
                        options: REFRESH_INTERVAL_OPTIONS.to_vec(),
                        onchange: move |v| refresh_interval.set(v),
                    }
                }
            }
            if let Some(err) = error.read().clone() {
                div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-4 text-sm text-red-700 dark:text-red-300",
                    "加载失败：{err}"
                }
            } else if let Some(s) = current {
                // 概览卡片网格
                div { class: "grid grid-cols-2 lg:grid-cols-4 gap-4",
                    // 卡片 1：数据库总大小
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "数据库大小" }
                            svg {
                                class: "w-4 h-4 text-[var(--color-paper-tertiary)]",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                ellipse { cx: "12", cy: "5", rx: "9", ry: "3" }
                                path { d: "M21 12c0 1.66-4 3-9 3s-9-1.34-9-3" }
                                path { d: "M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5" }
                            }
                        }
                        p { class: "text-2xl font-extrabold font-mono text-[var(--color-paper-primary)] tracking-tight",
                            "{format_bytes(s.db_size_bytes)}"
                        }
                    }
                    // 卡片 2：连接数
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "活跃连接数" }
                            svg {
                                class: "w-4 h-4 text-[var(--color-paper-tertiary)]",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" }
                            }
                        }
                        p { class: "text-2xl font-extrabold font-mono text-[var(--color-paper-primary)] tracking-tight",
                            "{s.total_connections} / {s.max_connections}"
                        }
                    }
                    // 卡片 3：表数量
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "数据表数量" }
                            svg {
                                class: "w-4 h-4 text-[var(--color-paper-tertiary)]",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
                                line { x1: "3", y1: "9", x2: "21", y2: "9" }
                                line { x1: "9", y1: "21", x2: "9", y2: "9" }
                            }
                        }
                        p { class: "text-2xl font-extrabold font-mono text-[var(--color-paper-primary)] tracking-tight",
                            "{s.tables.len()}"
                        }
                    }
                    // 卡片 4：迁移版本
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "当前迁移版本" }
                            svg {
                                class: "w-4 h-4 text-[var(--color-paper-tertiary)]",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                circle { cx: "12", cy: "12", r: "4" }
                                line { x1: "1.05", y1: "12", x2: "7", y2: "12" }
                                line { x1: "17.01", y1: "12", x2: "22.96", y2: "12" }
                            }
                        }
                        p { class: "text-2xl font-extrabold font-mono text-[var(--color-paper-accent)] tracking-tight truncate",
                            {s.migration_version.clone().map(|v| format!("v{v}")).unwrap_or_else(|| "—".to_string())}
                        }
                    }
                }

                // 表清单
                div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 overflow-hidden",
                    div { class: "px-5 py-4 border-b border-[var(--color-paper-border)]/70 flex flex-col sm:flex-row sm:items-center justify-between gap-1 select-none",
                        div { class: "flex items-center gap-2 font-semibold text-sm text-[var(--color-paper-primary)]",
                            svg {
                                class: "w-4 h-4 text-[var(--color-paper-accent)]",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                                polyline { points: "14 2 14 8 20 8" }
                            }
                            "数据表空间与记录清单"
                        }
                        span { class: "text-xs text-[var(--color-paper-tertiary)]", "小表为真实行数，大表标 ~ 为统计估算" }
                    }
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-sm",
                            thead {
                                tr { class: "bg-[var(--color-paper-entry)]/80 border-b border-[var(--color-paper-border)]/70 text-left text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)] select-none",
                                    th { class: "px-5 py-3.5", "表名" }
                                    th { class: "px-4 py-3.5 text-right whitespace-nowrap", "行数" }
                                    th { class: "px-4 py-3.5 text-right whitespace-nowrap", "数据大小" }
                                    th { class: "px-4 py-3.5 text-right whitespace-nowrap", "索引大小" }
                                    th { class: "px-4 py-3.5 text-right whitespace-nowrap", "总占用" }
                                    th { class: "px-5 py-3.5 text-right whitespace-nowrap", "死元组" }
                                }
                            }
                            tbody {
                                for t in s.tables.iter() {
                                    tr { class: "border-b border-[var(--color-paper-border)]/60 last:border-0 hover:bg-[var(--color-paper-accent-soft)]/20 transition-colors",
                                        td { class: "px-5 py-3 font-mono font-medium text-[var(--color-paper-primary)]",
                                            "{t.name}"
                                        }
                                        td { class: "px-4 py-3 text-right font-mono text-[var(--color-paper-secondary)]",
                                            if t.row_count_estimated {
                                                "~{t.row_count}"
                                            } else {
                                                "{t.row_count}"
                                            }
                                        }
                                        td { class: "px-4 py-3 text-right font-mono text-[var(--color-paper-secondary)]",
                                            "{format_bytes(t.table_size_bytes)}"
                                        }
                                        td { class: "px-4 py-3 text-right font-mono text-[var(--color-paper-secondary)]",
                                            "{format_bytes(t.index_size_bytes)}"
                                        }
                                        td { class: "px-4 py-3 text-right font-mono text-[var(--color-paper-primary)] font-semibold",
                                            "{format_bytes(t.total_size_bytes)}"
                                        }
                                        td { class: "px-5 py-3 text-right font-mono text-[var(--color-paper-tertiary)]",
                                            "{t.dead_tuples}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 索引占用 Top 10
                if !s.top_indexes.is_empty() {
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 overflow-hidden",
                        div { class: "px-5 py-4 border-b border-[var(--color-paper-border)]/70 flex items-center gap-2 select-none",
                            svg {
                                class: "w-4 h-4 text-[var(--color-paper-accent)]",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                line { x1: "18", y1: "20", x2: "18", y2: "10" }
                                line { x1: "12", y1: "20", x2: "12", y2: "4" }
                                line { x1: "6", y1: "20", x2: "6", y2: "14" }
                            }
                            span { class: "font-semibold text-sm text-[var(--color-paper-primary)]", "索引占用 Top 10" }
                        }
                        div { class: "overflow-x-auto",
                            table { class: "w-full text-sm",
                                thead {
                                    tr { class: "bg-[var(--color-paper-entry)]/80 border-b border-[var(--color-paper-border)]/70 text-left text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)] select-none",
                                        th { class: "px-5 py-3.5", "索引名" }
                                        th { class: "px-4 py-3.5", "所属表" }
                                        th { class: "px-5 py-3.5 text-right whitespace-nowrap", "占用空间" }
                                    }
                                }
                                tbody {
                                    for i in s.top_indexes.iter() {
                                        tr { class: "border-b border-[var(--color-paper-border)]/60 last:border-0 hover:bg-[var(--color-paper-accent-soft)]/20 transition-colors",
                                            td { class: "px-5 py-3 font-mono text-xs font-medium text-[var(--color-paper-primary)]",
                                                "{i.name}"
                                            }
                                            td { class: "px-4 py-3 font-mono text-xs text-[var(--color-paper-secondary)]",
                                                "{i.table_name}"
                                            }
                                            td { class: "px-5 py-3 text-right font-mono text-xs text-[var(--color-paper-primary)] font-semibold",
                                                "{format_bytes(i.size_bytes)}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 活跃连接
                div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 overflow-hidden",
                    div { class: "px-5 py-4 border-b border-[var(--color-paper-border)]/70 flex items-center gap-2 select-none",
                        svg {
                            class: "w-4 h-4 text-[var(--color-paper-accent)]",
                            xmlns: "http://www.w3.org/2000/svg",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" }
                        }
                        span { class: "font-semibold text-sm text-[var(--color-paper-primary)]",
                            "当前活跃连接 ({s.active_connections.len()})"
                        }
                    }
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-sm",
                            thead {
                                tr { class: "bg-[var(--color-paper-entry)]/80 border-b border-[var(--color-paper-border)]/70 text-left text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)] select-none",
                                    th { class: "px-5 py-3.5 w-20", "PID" }
                                    th { class: "px-4 py-3.5 w-28", "用户" }
                                    th { class: "px-4 py-3.5 w-24", "状态" }
                                    th { class: "px-4 py-3.5 w-24 text-right whitespace-nowrap", "执行时长" }
                                    th { class: "px-5 py-3.5", "SQL 查询语句" }
                                }
                            }
                            tbody {
                                for c in s.active_connections.iter() {
                                    tr { class: "border-b border-[var(--color-paper-border)]/60 last:border-0 hover:bg-[var(--color-paper-accent-soft)]/20 transition-colors",
                                        td { class: "px-5 py-3 font-mono text-xs text-[var(--color-paper-secondary)]",
                                            "{c.pid}"
                                        }
                                        td { class: "px-4 py-3 font-mono text-xs text-[var(--color-paper-secondary)]",
                                            "{c.user}"
                                        }
                                        td { class: "px-4 py-3 text-xs text-[var(--color-paper-secondary)]",
                                            {c.state.clone().unwrap_or_else(|| "—".to_string())}
                                        }
                                        td { class: "px-4 py-3 text-right font-mono text-xs text-[var(--color-paper-secondary)]",
                                            {
                                                c.query_duration_secs
                                                    .map(|d| format!("{:.1}s", d))
                                                    .unwrap_or_else(|| "—".to_string())
                                            }
                                        }
                                        td { class: "px-5 py-3 font-mono text-xs text-[var(--color-paper-secondary)] max-w-lg truncate",
                                            {c.query.clone().unwrap_or_else(|| "—".to_string())}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else if loading() {
                // 首次加载骨架屏：延迟 200ms 显示，避免快速加载闪烁。
                DelayedSkeleton {
                    div { class: "space-y-4",
                        // 概览卡片骨架
                        div { class: "grid grid-cols-2 md:grid-cols-4 gap-4",
                            for _ in 0..4 {
                                div { class: "rounded-2xl bg-paper-entry border border-paper-border p-4 space-y-2",
                                    SkeletonBox { class: "h-3 w-16 rounded" }
                                    SkeletonBox { class: "h-6 w-24 rounded" }
                                }
                            }
                        }
                        // 表清单骨架
                        div { class: "rounded-2xl bg-paper-entry border border-paper-border overflow-hidden",
                            div { class: "px-4 py-3 border-b border-paper-border",
                                SkeletonBox { class: "h-4 w-40 rounded" }
                            }
                            for _ in 0..5 {
                                div { class: "flex justify-between px-4 py-3 border-b border-paper-border last:border-0",
                                    SkeletonBox { class: "h-4 w-24 rounded" }
                                    SkeletonBox { class: "h-4 w-16 rounded" }
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "text-paper-secondary py-8", "暂无数据" }
            }
        }
    }
}
