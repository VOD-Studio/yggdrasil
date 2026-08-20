//! 服务器状态 tab。

use dioxus::prelude::*;

use crate::components::forms::{FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::LoadingButton;

use crate::utils::format_bytes;

/// 秒数 → 人类可读运行时间（如 1d 2h 3m）。
fn format_uptime(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("{secs}s")
    }
}
/// 自动刷新间隔可选项（毫秒；None = 手动）。
const REFRESH_MS_OPTIONS: &[(Option<u32>, &str)] = &[
    (None, "手动"),
    (Some(500), "500ms"),
    (Some(1000), "1s"),
    (Some(2000), "2s"),
    (Some(5000), "5s"),
];

/// 服务器状态 tab：应用内指标（连接池/会话/缓存命中率）+ 主机层（CPU/内存/磁盘）。
/// 手动刷新 + 自动刷新开关（500ms/1s/2s/5s/手动，默认手动）。
/// 主机层数据由后台 500ms 采样，前端轮询只读快照零成本，故可高频。
#[allow(non_snake_case)]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn ServerStatusTab() -> Element {
    #[cfg(target_arch = "wasm32")]
    use crate::api::database::system_status::get_server_status;
    use crate::api::database::system_status::ServerStatus;

    let mut status = use_signal(|| Option::<ServerStatus>::None);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| Option::<String>::None);
    // 自动刷新间隔（毫秒）；None = 手动。主机层后台采样，前端可高频轮询。
    let mut refresh_ms: Signal<Option<u32>> = use_signal(|| None);

    let mut load_once = move || {
        loading.set(true);
        #[cfg(target_arch = "wasm32")]
        {
            spawn(async move {
                match get_server_status().await {
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

    use_effect(move || {
        load_once();
    });

    // 自动刷新：同 DbStatusTab，采用官方推荐的单一长生命周期 loop 模式。
    // 闭包体内不读任何 signal（避免隐式依赖追踪导致重建），loop 内部实时读取。
    use_future(move || async move {
        #[cfg(target_arch = "wasm32")]
        {
            loop {
                let ms = refresh_ms().unwrap_or(0);
                if ms == 0 {
                    crate::utils::time::sleep_ms(200).await;
                    continue;
                }
                crate::utils::time::sleep_ms(ms).await;
                if refresh_ms().is_none() {
                    continue;
                }
                loading.set(true);
                spawn(async move {
                    match get_server_status().await {
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
            let _ = (status, loading, error, refresh_ms);
        }
    });

    let current = status.read().clone();
    // rsx 不支持格式说明符（{:.1}），也不允许在 for 循环体内 let，故预格式化所有展示值。
    let cpu_pct = current
        .as_ref()
        .map(|s| format!("{:.1}%", s.host.cpu_usage))
        .unwrap_or_default();
    let load_1 = current
        .as_ref()
        .map(|s| format!("{:.2}", s.host.load_avg_1))
        .unwrap_or_default();
    // 缓存表预格式化：把每行需要展示的值都算好字符串，避免在 rsx 里做格式化。
    let cache_rows: Vec<(String, u64, u64, u64, String)> = current
        .as_ref()
        .map(|s| {
            s.caches
                .iter()
                .map(|c| {
                    (
                        c.name.clone(),
                        c.entry_count,
                        c.hits,
                        c.misses,
                        format!("{:.1}%", c.hit_rate * 100.0),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    rsx! {
        div { class: "space-y-6",
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
                                match get_server_status().await {
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
                        value: refresh_ms(),
                        options: REFRESH_MS_OPTIONS.to_vec(),
                        onchange: move |v| refresh_ms.set(v),
                    }
                }
            }
            if let Some(err) = error.read().clone() {
                div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-4 text-sm text-red-700 dark:text-red-300",
                    "加载失败：{err}"
                }
            } else if let Some(s) = current {
                // 应用内指标卡片
                div { class: "grid grid-cols-2 lg:grid-cols-4 gap-4",
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "服务运行时间" }
                            svg {
                                class: "w-4 h-4 text-[var(--color-paper-tertiary)]",
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
                        }
                        p { class: "text-2xl font-extrabold font-mono text-[var(--color-paper-primary)] tracking-tight",
                            "{format_uptime(s.uptime_secs)}"
                        }
                    }
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "DB 连接池" }
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
                            "{s.pool_size} / {s.pool_max_size}"
                        }
                        p { class: "text-xs text-[var(--color-paper-tertiary)] mt-0.5",
                            "空闲 {s.pool_available} · 等待 {s.pool_waiting}"
                        }
                    }
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "活跃会话" }
                            svg {
                                class: "w-4 h-4 text-[var(--color-paper-tertiary)]",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" }
                                circle { cx: "9", cy: "7", r: "4" }
                            }
                        }
                        p { class: "text-2xl font-extrabold font-mono text-[var(--color-paper-primary)] tracking-tight",
                            "{s.active_sessions}"
                        }
                    }
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "应用 CPU" }
                            svg {
                                class: "w-4 h-4 text-[var(--color-paper-tertiary)]",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                rect { x: "4", y: "4", width: "16", height: "16", rx: "2" }
                                rect { x: "9", y: "9", width: "6", height: "6" }
                            }
                        }
                        p { class: "text-2xl font-extrabold font-mono text-[var(--color-paper-accent)] tracking-tight",
                            "{cpu_pct}"
                        }
                    }
                }

                // 主机层指标卡片
                div { class: "grid grid-cols-2 lg:grid-cols-4 gap-4",
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "内存占用" }
                        }
                        p { class: "text-xl font-bold font-mono text-[var(--color-paper-primary)] tracking-tight",
                            "{format_bytes(s.host.used_memory as i64)} / {format_bytes(s.host.total_memory as i64)}"
                        }
                    }
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "磁盘空间" }
                        }
                        p { class: "text-xl font-bold font-mono text-[var(--color-paper-primary)] tracking-tight",
                            "{format_bytes((s.host.disk_total - s.host.disk_available) as i64)} / {format_bytes(s.host.disk_total as i64)}"
                        }
                    }
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "平均负载 (1m)" }
                        }
                        p { class: "text-xl font-bold font-mono text-[var(--color-paper-primary)] tracking-tight",
                            "{load_1}"
                        }
                    }
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl p-5 border border-[var(--color-paper-border)]/70 shadow-xs flex flex-col gap-1.5",
                        div { class: "flex items-center justify-between text-[var(--color-paper-secondary)]",
                            span { class: "text-xs font-semibold uppercase tracking-wider", "宿主系统" }
                        }
                        p { class: "text-base font-semibold font-mono text-[var(--color-paper-primary)] truncate mt-0.5",
                            "{s.host.os_name}"
                        }
                    }
                }

                // 缓存命中率表
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
                            path { d: "M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" }
                        }
                        span { class: "font-semibold text-sm text-[var(--color-paper-primary)]", "内存缓存指标 (Moka Cache)" }
                    }
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-sm",
                            thead {
                                tr { class: "bg-[var(--color-paper-entry)]/80 border-b border-[var(--color-paper-border)]/70 text-left text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)] select-none",
                                    th { class: "px-5 py-3.5", "缓存名称" }
                                    th { class: "px-4 py-3.5 text-right whitespace-nowrap", "当前条目" }
                                    th { class: "px-4 py-3.5 text-right whitespace-nowrap", "命中次数" }
                                    th { class: "px-4 py-3.5 text-right whitespace-nowrap", "未命中数" }
                                    th { class: "px-5 py-3.5 text-right whitespace-nowrap", "命中率" }
                                }
                            }
                            tbody {
                                for (name, entry_count, hits, misses, rate_pct) in cache_rows.iter() {
                                    tr { class: "border-b border-[var(--color-paper-border)]/60 last:border-0 hover:bg-[var(--color-paper-accent-soft)]/20 transition-colors",
                                        td { class: "px-5 py-3 font-mono font-medium text-[var(--color-paper-primary)]",
                                            "{name}"
                                        }
                                        td { class: "px-4 py-3 text-right font-mono text-[var(--color-paper-secondary)]",
                                            "{entry_count}"
                                        }
                                        td { class: "px-4 py-3 text-right font-mono text-[var(--color-paper-secondary)]",
                                            "{hits}"
                                        }
                                        td { class: "px-4 py-3 text-right font-mono text-[var(--color-paper-secondary)]",
                                            "{misses}"
                                        }
                                        td { class: "px-5 py-3 text-right font-mono text-[var(--color-paper-primary)] font-semibold",
                                            "{rate_pct}"
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
                        // 应用内指标卡片骨架
                        div { class: "grid grid-cols-2 md:grid-cols-4 gap-4",
                            for _ in 0..4 {
                                div { class: "rounded-2xl bg-paper-entry border border-paper-border p-4 space-y-2",
                                    SkeletonBox { class: "h-3 w-16 rounded" }
                                    SkeletonBox { class: "h-6 w-24 rounded" }
                                }
                            }
                        }
                        // 主机层指标卡片骨架
                        div { class: "grid grid-cols-2 md:grid-cols-4 gap-4",
                            for _ in 0..4 {
                                div { class: "rounded-2xl bg-paper-entry border border-paper-border p-4 space-y-2",
                                    SkeletonBox { class: "h-3 w-16 rounded" }
                                    SkeletonBox { class: "h-6 w-24 rounded" }
                                }
                            }
                        }
                        // 缓存命中率表骨架
                        div { class: "rounded-2xl bg-paper-entry border border-paper-border overflow-hidden",
                            div { class: "px-4 py-3 border-b border-paper-border",
                                SkeletonBox { class: "h-4 w-24 rounded" }
                            }
                            for _ in 0..4 {
                                div { class: "flex justify-between px-4 py-3 border-b border-paper-border last:border-0",
                                    SkeletonBox { class: "h-4 w-20 rounded" }
                                    SkeletonBox { class: "h-4 w-12 rounded" }
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
