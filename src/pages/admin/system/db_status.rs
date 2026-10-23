//! 数据库 tab：概览卡片（大小、连接、表数、迁移版本）、数据表占用、索引占用前十与
//! 活跃连接。DB 查询有成本，自动刷新最快 1 秒、默认手动。
use dioxus::prelude::*;

use super::{Meter, Metric, RefreshControl};
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::LoadError;
use crate::utils::format_bytes;

/// 数据库 tab。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn DbStatusTab() -> Element {
    use crate::api::database::status::{get_db_status, DbStatus};
    let mut status = use_signal(|| None::<DbStatus>);
    let mut loading = use_signal(|| true);
    let mut failed = use_signal(|| false);
    let refresh_ms: Signal<Option<u32>> = use_signal(|| None);
    let reload = use_callback(move |()| {
        loading.set(true);
        spawn(async move {
            match get_db_status().await {
                Ok(s) => {
                    status.set(Some(s));
                    failed.set(false);
                }
                Err(_) => failed.set(true),
            }
            loading.set(false);
        });
    });
    use_hook(move || {
        #[cfg(target_arch = "wasm32")]
        reload.call(());
    });
    super::use_auto_refresh(refresh_ms, reload);

    let current = status();
    rsx! {
        RefreshControl { refresh_ms, loading: loading(), on_refresh: move |_| reload.call(()) }
        if failed() && current.is_none() {
            LoadError { title: "数据库状态加载失败", on_retry: move |_| reload.call(()) }
        } else if let Some(s) = current {
            {
                let largest = s.tables.iter().map(|t| t.total_size_bytes).max().unwrap_or(0).max(1);
                rsx! {
                    div { class: "sys-metrics",
                        Metric { label: "数据库大小", value: format_bytes(s.db_size_bytes) }
                        Metric { label: "连接", value: format!("{} / {}", s.total_connections, s.max_connections), meta: format!("活跃 {}", s.active_connections.len()),
                            Meter { pct: s.total_connections as f64 / s.max_connections.max(1) as f64 * 100.0 }
                        }
                        Metric { label: "数据表", value: s.tables.len().to_string() }
                        Metric { label: "迁移版本", value: s.migration_version.clone().unwrap_or_else(|| "—".into()), mono: true }
                    }
                    section { class: "admin-panel sys-section", aria_labelledby: "sys-tables-h",
                        div { class: "sys-section-head",
                            h2 { id: "sys-tables-h", "数据表" }
                            span { "按总占用排序 · 大表行数为估算" }
                        }
                        div { class: "sys-table-scroll",
                            table { class: "admin-table",
                                thead {
                                    tr {
                                        th { "表" }
                                        th { class: "admin-table-num", "行数" }
                                        th { class: "admin-table-num", "数据" }
                                        th { class: "admin-table-num", "索引" }
                                        th { class: "admin-table-num", "总占用" }
                                        th { class: "admin-table-num", "死元组" }
                                        th { class: "sys-col-share", "占比" }
                                    }
                                }
                                tbody {
                                    for t in s.tables.iter() {
                                        tr { key: "{t.name}",
                                            td { class: "sys-mono", "{t.name}" }
                                            td { class: "admin-table-num",
                                                if t.row_count_estimated { "~{t.row_count}" } else { "{t.row_count}" }
                                            }
                                            td { class: "admin-table-num", "{format_bytes(t.table_size_bytes)}" }
                                            td { class: "admin-table-num", "{format_bytes(t.index_size_bytes)}" }
                                            td { class: "admin-table-num", "{format_bytes(t.total_size_bytes)}" }
                                            td { class: "admin-table-num", "data-warn": (t.dead_tuples > 1000).then_some("true"), "{t.dead_tuples}" }
                                            td { class: "sys-col-share",
                                                Meter { pct: t.total_size_bytes as f64 / largest as f64 * 100.0, calm: true }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !s.top_indexes.is_empty() {
                        section { class: "admin-panel sys-section", aria_labelledby: "sys-index-h",
                            div { class: "sys-section-head",
                                h2 { id: "sys-index-h", "索引占用前十" }
                            }
                            div { class: "sys-table-scroll",
                                table { class: "admin-table",
                                    thead {
                                        tr {
                                            th { "索引" }
                                            th { "表" }
                                            th { class: "admin-table-num", "大小" }
                                        }
                                    }
                                    tbody {
                                        for i in s.top_indexes.iter() {
                                            tr { key: "{i.name}",
                                                td { class: "sys-mono", "{i.name}" }
                                                td { class: "sys-mono admin-table-muted", "{i.table_name}" }
                                                td { class: "admin-table-num", "{format_bytes(i.size_bytes)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    section { class: "admin-panel sys-section", aria_labelledby: "sys-conn-h",
                        div { class: "sys-section-head",
                            h2 { id: "sys-conn-h", "活跃连接" }
                            span { "{s.active_connections.len()} 个" }
                        }
                        div { class: "sys-table-scroll",
                            table { class: "admin-table",
                                thead {
                                    tr {
                                        th { class: "admin-table-num", "PID" }
                                        th { "用户" }
                                        th { "状态" }
                                        th { class: "admin-table-num", "耗时" }
                                        th { "查询" }
                                    }
                                }
                                tbody {
                                    for c in s.active_connections.iter() {
                                        tr { key: "{c.pid}",
                                            td { class: "admin-table-num", "{c.pid}" }
                                            td { "{c.user}" }
                                            td { class: "admin-table-muted", {c.state.clone().unwrap_or_else(|| "—".into())} }
                                            td { class: "admin-table-num",
                                                {c.query_duration_secs.map(|d| format!("{d:.1} 秒")).unwrap_or_else(|| "—".into())}
                                            }
                                            td { class: "sys-mono sys-query", title: c.query.clone().unwrap_or_default(),
                                                {c.query.clone().unwrap_or_else(|| "—".into())}
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
                for i in 0..4 {
                    div { key: "{i}", class: "admin-panel sys-metric",
                        SkeletonBox { class: "h-3 w-16 rounded" }
                        SkeletonBox { class: "h-6 w-24 rounded mt-2" }
                    }
                }
            }
        }
    }
}
