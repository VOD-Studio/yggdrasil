//! 服务器 tab：应用与主机指标卡片（运行时间、CPU、内存、磁盘、负载、连接池、会话、
//! 宿主系统）与内存缓存命中率表。主机层由后台 500ms 采样，前端轮询只读快照，
//! 自动刷新可选 1s / 2s / 5s / 10s。
//!
//! 指标卡片与占用条的文本、宽度直接绑定到状态信号：刷新时节点原地更新，占用条的
//! `width` 过渡动画得以保留。缓存表按整行内容做 key，数值变化时重建该行。
use leptos::prelude::*;

use super::{use_polled_status, Meter, Metric, PolledStatus, RefreshControl};
use crate::api::database::system_status::{get_server_status, ServerStatus};
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

fn mem_pct(s: &ServerStatus) -> f64 {
    percent(s.host.used_memory as f64, s.host.total_memory as f64)
}

fn disk_pct(s: &ServerStatus) -> f64 {
    let used = s.host.disk_total.saturating_sub(s.host.disk_available);
    percent(used as f64, s.host.disk_total as f64)
}

fn pool_pct(s: &ServerStatus) -> f64 {
    percent(s.pool_size as f64, s.pool_max_size as f64)
}

/// 服务器 tab。
#[component]
pub(super) fn ServerStatusTab() -> impl IntoView {
    let PolledStatus {
        status,
        loading,
        failed,
        refresh_ms,
        reload,
    } = use_polled_status(get_server_status, Some(10_000));
    let ready = Memo::new(move |_| status.with(Option::is_some));
    let show_error = Memo::new(move |_| failed.get() && !ready.get());

    view! {
        <RefreshControl refresh_ms loading on_refresh=move |_| reload.run(())/>
        {move || {
            if show_error.get() {
                view! {
                    <LoadError title="服务器状态加载失败" on_retry=move |_| reload.run(())/>
                }
                    .into_any()
            } else if ready.get() {
                view! { <ServerStatusBody status/> }.into_any()
            } else {
                view! {
                    <div class="sys-metrics" aria-hidden="true">
                        {(0..8)
                            .map(|_| {
                                view! {
                                    <div class="admin-panel sys-metric">
                                        <SkeletonBox class="h-3 w-16 rounded"/>
                                        <SkeletonBox class="h-6 w-24 rounded mt-2"/>
                                    </div>
                                }
                            })
                            .collect_view()}
                    </div>
                }
                    .into_any()
            }
        }}
    }
}

/// 指标卡片与缓存表：只在已有数据后渲染，字段绑定到 `status`。
#[component]
fn ServerStatusBody(status: RwSignal<Option<ServerStatus>>) -> impl IntoView {
    let text = move |f: fn(&ServerStatus) -> String| {
        Signal::derive(move || status.with(|s| s.as_ref().map(f).unwrap_or_default()))
    };
    let pct = move |f: fn(&ServerStatus) -> f64| {
        Signal::derive(move || status.with(|s| s.as_ref().map(f).unwrap_or_default()))
    };
    let caches = move || status.with(|s| s.as_ref().map(|s| s.caches.clone()).unwrap_or_default());

    view! {
        <div class="sys-metrics">
            <Metric
                label="运行时间"
                value=text(|s| format_uptime(s.uptime_secs))
                meta=text(|s| format!("主机已运行 {}", format_uptime(s.host.uptime_secs)))
            />
            <Metric label="应用 CPU" value=text(|s| format!("{:.1}%", s.host.cpu_usage))>
                <Meter pct=pct(|s| s.host.cpu_usage as f64)/>
            </Metric>
            <Metric
                label="内存"
                value=text(|s| format_bytes(s.host.used_memory as i64))
                meta=text(|s| format!("共 {}", format_bytes(s.host.total_memory as i64)))
            >
                <Meter pct=pct(mem_pct)/>
            </Metric>
            <Metric
                label="磁盘"
                value=text(|s| format!("{:.0}%", disk_pct(s)))
                meta=text(|s| format!("剩余 {}", format_bytes(s.host.disk_available as i64)))
            >
                <Meter pct=pct(disk_pct)/>
            </Metric>
            <Metric
                label="平均负载"
                value=text(|s| format!("{:.2}", s.host.load_avg_1))
                meta="1 分钟"
            />
            <Metric
                label="数据库连接池"
                value=text(|s| format!("{} / {}", s.pool_size, s.pool_max_size))
                meta=text(|s| format!("空闲 {} · 等待 {}", s.pool_available, s.pool_waiting))
            >
                <Meter pct=pct(pool_pct)/>
            </Metric>
            <Metric
                label="活跃会话"
                value=text(|s| s.active_sessions.to_string())
                meta="未过期的登录"
            />
            <Metric
                label="宿主系统"
                value=text(|s| s.host.os_name.clone())
                meta=text(|s| s.host.kernel_version.clone())
                small=true
            />
        </div>
        <section class="admin-panel sys-section" aria-labelledby="sys-cache-h">
            <div class="sys-section-head">
                <h2 id="sys-cache-h">"内存缓存"</h2>
            </div>
            <div class="sys-table-scroll">
                <table class="admin-table">
                    <thead>
                        <tr>
                            <th>"缓存"</th>
                            <th class="admin-table-num">"条目"</th>
                            <th class="admin-table-num">"命中"</th>
                            <th class="admin-table-num">"未命中"</th>
                            <th>"命中率"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=caches
                            key=|c| (c.name.clone(), c.entry_count, c.hits, c.misses, c.hit_rate.to_bits())
                            children=|c| {
                                view! {
                                    <tr>
                                        <td class="sys-mono">{c.name.clone()}</td>
                                        <td class="admin-table-num">{c.entry_count}</td>
                                        <td class="admin-table-num">{c.hits}</td>
                                        <td class="admin-table-num">{c.misses}</td>
                                        <td>
                                            <div class="sys-rate">
                                                <Meter pct=c.hit_rate * 100.0 calm=true/>
                                                <span>{format!("{:.0}%", c.hit_rate * 100.0)}</span>
                                            </div>
                                        </td>
                                    </tr>
                                }
                            }
                        />
                    </tbody>
                </table>
            </div>
        </section>
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
