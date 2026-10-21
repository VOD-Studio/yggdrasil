//! 服务器 tab：应用与主机指标卡片（运行时间、CPU、内存、磁盘、负载、连接池、会话、
//! 宿主系统）与内存缓存命中率表。主机层由后台 500ms 采样，前端轮询只读快照，
//! 自动刷新可选 1s / 2s / 5s / 10s。
use dioxus::prelude::*;

use super::{Meter, Metric, RefreshControl};
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::LoadError;
use crate::utils::format_bytes;

/// 秒数 → 「6 天 4 小时」「4 小时 3 分」「3 分」「42 秒」。
fn format_uptime(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d} 天 {h} 小时")
    } else if h > 0 {
        format!("{h} 小时 {m} 分")
    } else if m > 0 {
        format!("{m} 分")
    } else {
        format!("{secs} 秒")
    }
}

/// 百分比，分母为 0 时为 0。
fn percent(part: f64, whole: f64) -> f64 {
    if whole <= 0.0 {
        0.0
    } else {
        (part / whole * 100.0).clamp(0.0, 100.0)
    }
}

/// 服务器 tab。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn ServerStatusTab() -> Element {
    use crate::api::database::system_status::{get_server_status, ServerStatus};
    let mut status = use_signal(|| None::<ServerStatus>);
    let mut loading = use_signal(|| true);
    let mut failed = use_signal(|| false);
    let refresh_ms: Signal<Option<u32>> = use_signal(|| Some(10_000));
    let reload = use_callback(move |()| {
        loading.set(true);
        spawn(async move {
            match get_server_status().await {
                Ok(s) => {
                    status.set(Some(s));
                    failed.set(false);
                }
                Err(_) => failed.set(true),
            }
            loading.set(false);
        });
    });
    use_hook(move || reload.call(()));
    super::use_auto_refresh(refresh_ms, reload);

    let current = status();
    rsx! {
        RefreshControl { refresh_ms, loading: loading(), on_refresh: move |_| reload.call(()) }
        if failed() && current.is_none() {
            LoadError { title: "服务器状态加载失败", on_retry: move |_| reload.call(()) }
        } else if let Some(s) = current {
            {
                let h = &s.host;
                let mem_pct = percent(h.used_memory as f64, h.total_memory as f64);
                let disk_used = h.disk_total.saturating_sub(h.disk_available);
                let disk_pct = percent(disk_used as f64, h.disk_total as f64);
                let pool_pct = percent(s.pool_size as f64, s.pool_max_size as f64);
                rsx! {
                    div { class: "sys-metrics",
                        Metric { label: "运行时间", value: format_uptime(s.uptime_secs), meta: format!("主机已运行 {}", format_uptime(h.uptime_secs)) }
                        Metric { label: "应用 CPU", value: format!("{:.1}%", h.cpu_usage),
                            Meter { pct: h.cpu_usage as f64 }
                        }
                        Metric { label: "内存", value: format_bytes(h.used_memory as i64), meta: format!("共 {}", format_bytes(h.total_memory as i64)),
                            Meter { pct: mem_pct }
                        }
                        Metric { label: "磁盘", value: format!("{disk_pct:.0}%"), meta: format!("剩余 {}", format_bytes(h.disk_available as i64)),
                            Meter { pct: disk_pct }
                        }
                        Metric { label: "平均负载", value: format!("{:.2}", h.load_avg_1), meta: "1 分钟".to_string() }
                        Metric { label: "数据库连接池", value: format!("{} / {}", s.pool_size, s.pool_max_size), meta: format!("空闲 {} · 等待 {}", s.pool_available, s.pool_waiting),
                            Meter { pct: pool_pct }
                        }
                        Metric { label: "活跃会话", value: s.active_sessions.to_string(), meta: "未过期的登录".to_string() }
                        Metric { label: "宿主系统", value: h.os_name.clone(), meta: h.kernel_version.clone(), small: true }
                    }
                    section { class: "admin-panel sys-section", aria_labelledby: "sys-cache-h",
                        div { class: "sys-section-head",
                            h2 { id: "sys-cache-h", "内存缓存" }
                        }
                        div { class: "sys-table-scroll",
                            table { class: "admin-table",
                                thead {
                                    tr {
                                        th { "缓存" }
                                        th { class: "admin-table-num", "条目" }
                                        th { class: "admin-table-num", "命中" }
                                        th { class: "admin-table-num", "未命中" }
                                        th { "命中率" }
                                    }
                                }
                                tbody {
                                    for c in s.caches.iter() {
                                        tr { key: "{c.name}",
                                            td { class: "sys-mono", "{c.name}" }
                                            td { class: "admin-table-num", "{c.entry_count}" }
                                            td { class: "admin-table-num", "{c.hits}" }
                                            td { class: "admin-table-num", "{c.misses}" }
                                            td {
                                                div { class: "sys-rate",
                                                    Meter { pct: c.hit_rate * 100.0, calm: true }
                                                    span { {format!("{:.0}%", c.hit_rate * 100.0)} }
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
        } else {
            div { class: "sys-metrics", aria_hidden: "true",
                for i in 0..8 {
                    div { key: "{i}", class: "admin-panel sys-metric",
                        SkeletonBox { class: "h-3 w-16 rounded" }
                        SkeletonBox { class: "h-6 w-24 rounded mt-2" }
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
    fn uptime_reads_in_largest_two_units() {
        assert_eq!(format_uptime(42), "42 秒");
        assert_eq!(format_uptime(180), "3 分");
        assert_eq!(format_uptime(3 * 3600 + 120), "3 小时 2 分");
        assert_eq!(format_uptime(6 * 86400 + 4 * 3600), "6 天 4 小时");
    }

    #[test]
    fn percent_handles_zero_total() {
        assert_eq!(percent(1.0, 0.0), 0.0);
        assert_eq!(percent(1.0, 4.0), 25.0);
        assert_eq!(percent(9.0, 4.0), 100.0);
    }
}
