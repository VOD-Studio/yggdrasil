//! 数据库 tab：概览卡片（大小、连接、表数、迁移版本）、数据表占用、索引占用前十与
//! 活跃连接。DB 查询有成本，自动刷新最快 1 秒、默认手动。
//!
//! 概览卡片与占用条直接绑定状态信号，刷新时原地更新；三张表按整行内容做 key，
//! 内容变化时重建该行（滚动容器保持不动）。
use leptos::prelude::*;

use super::{use_polled_status, Meter, Metric, PolledStatus, RefreshControl};
use crate::api::database::status::{get_db_status, ConnInfo, DbStatus, IndexInfo, TableInfo};
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::LoadError;
use crate::utils::format_bytes;

/// 数据库 tab。
#[component]
pub(super) fn DbStatusTab() -> impl IntoView {
    let PolledStatus {
        status,
        loading,
        failed,
        refresh_ms,
        reload,
    } = use_polled_status(get_db_status, None);
    let ready = Memo::new(move |_| status.with(Option::is_some));
    let show_error = Memo::new(move |_| failed.get() && !ready.get());

    view! {
        <RefreshControl refresh_ms loading on_refresh=move |_| reload.run(())/>
        {move || {
            if show_error.get() {
                view! {
                    <LoadError title="数据库状态加载失败" on_retry=move |_| reload.run(())/>
                }
                    .into_any()
            } else if ready.get() {
                view! { <DbStatusBody status/> }.into_any()
            } else {
                view! {
                    <div class="sys-metrics" aria-hidden="true">
                        {(0..4)
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

/// 表占用行：与最大表的占比随行内容一起固定在行内（`largest` 变化时行内容也变）。
#[derive(Clone)]
struct TableRow {
    info: TableInfo,
    largest: i64,
}

/// 概览卡片与四张明细：只在已有数据后渲染，字段绑定到 `status`。
#[component]
fn DbStatusBody(status: RwSignal<Option<DbStatus>>) -> impl IntoView {
    let text = move |f: fn(&DbStatus) -> String| {
        Signal::derive(move || status.with(|s| s.as_ref().map(f).unwrap_or_default()))
    };
    let conn_pct = Signal::derive(move || {
        status.with(|s| {
            s.as_ref()
                .map(|s| {
                    f64::from(s.total_connections) / f64::from(s.max_connections.max(1)) * 100.0
                })
                .unwrap_or_default()
        })
    });
    let table_count = Memo::new(move |_| status.with(|s| s.as_ref().map_or(0, |s| s.tables.len())));
    let conn_count =
        Memo::new(move |_| status.with(|s| s.as_ref().map_or(0, |s| s.active_connections.len())));
    let tables = move || {
        status.with(|s| {
            s.as_ref()
                .map(|s| {
                    let largest = s
                        .tables
                        .iter()
                        .map(|t| t.total_size_bytes)
                        .max()
                        .unwrap_or(0)
                        .max(1);
                    s.tables
                        .iter()
                        .map(|info| TableRow {
                            info: info.clone(),
                            largest,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    };
    let indexes = move || {
        status.with(|s| {
            s.as_ref()
                .map(|s| s.top_indexes.clone())
                .unwrap_or_default()
        })
    };
    let has_indexes =
        Memo::new(move |_| status.with(|s| s.as_ref().is_some_and(|s| !s.top_indexes.is_empty())));
    let connections = move || {
        status.with(|s| {
            s.as_ref()
                .map(|s| s.active_connections.clone())
                .unwrap_or_default()
        })
    };

    view! {
        <div class="sys-metrics">
            <Metric label="数据库大小" value=text(|s| format_bytes(s.db_size_bytes))/>
            <Metric
                label="连接"
                value=text(|s| format!("{} / {}", s.total_connections, s.max_connections))
                meta=text(|s| format!("活跃 {}", s.active_connections.len()))
            >
                <Meter pct=conn_pct/>
            </Metric>
            <Metric label="数据表" value=Signal::derive(move || table_count.get().to_string())/>
            <Metric
                label="迁移版本"
                value=text(|s| s.migration_version.clone().unwrap_or_else(|| "—".into()))
                mono=true
            />
        </div>
        <section class="admin-panel sys-section" aria-labelledby="sys-tables-h">
            <div class="sys-section-head">
                <h2 id="sys-tables-h">"数据表"</h2>
                <span>"按总占用排序 · 大表行数为估算"</span>
            </div>
            <div class="sys-table-scroll">
                <table class="admin-table">
                    <thead>
                        <tr>
                            <th>"表"</th>
                            <th class="admin-table-num">"行数"</th>
                            <th class="admin-table-num">"数据"</th>
                            <th class="admin-table-num">"索引"</th>
                            <th class="admin-table-num">"总占用"</th>
                            <th class="admin-table-num">"死元组"</th>
                            <th class="sys-col-share">"占比"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=tables
                            key=|row| {
                                let t = &row.info;
                                (
                                    t.name.clone(),
                                    t.row_count,
                                    t.row_count_estimated,
                                    t.table_size_bytes,
                                    t.index_size_bytes,
                                    t.total_size_bytes,
                                    t.dead_tuples,
                                    row.largest,
                                )
                            }
                            children=|row| {
                                let t = row.info;
                                let rows = if t.row_count_estimated {
                                    format!("~{}", t.row_count)
                                } else {
                                    t.row_count.to_string()
                                };
                                view! {
                                    <tr>
                                        <td class="sys-mono">{t.name.clone()}</td>
                                        <td class="admin-table-num">{rows}</td>
                                        <td class="admin-table-num">{format_bytes(t.table_size_bytes)}</td>
                                        <td class="admin-table-num">{format_bytes(t.index_size_bytes)}</td>
                                        <td class="admin-table-num">{format_bytes(t.total_size_bytes)}</td>
                                        <td
                                            class="admin-table-num"
                                            data-warn=(t.dead_tuples > 1000).then_some("true")
                                        >
                                            {t.dead_tuples}
                                        </td>
                                        <td class="sys-col-share">
                                            <Meter
                                                pct=t.total_size_bytes as f64 / row.largest as f64 * 100.0
                                                calm=true
                                            />
                                        </td>
                                    </tr>
                                }
                            }
                        />
                    </tbody>
                </table>
            </div>
        </section>
        <Show when=move || has_indexes.get()>
            <section class="admin-panel sys-section" aria-labelledby="sys-index-h">
                <div class="sys-section-head">
                    <h2 id="sys-index-h">"索引占用前十"</h2>
                </div>
                <div class="sys-table-scroll">
                    <table class="admin-table">
                        <thead>
                            <tr>
                                <th>"索引"</th>
                                <th>"表"</th>
                                <th class="admin-table-num">"大小"</th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each=indexes
                                key=|i| (i.name.clone(), i.table_name.clone(), i.size_bytes)
                                children=|i: IndexInfo| {
                                    view! {
                                        <tr>
                                            <td class="sys-mono">{i.name.clone()}</td>
                                            <td class="sys-mono admin-table-muted">{i.table_name.clone()}</td>
                                            <td class="admin-table-num">{format_bytes(i.size_bytes)}</td>
                                        </tr>
                                    }
                                }
                            />
                        </tbody>
                    </table>
                </div>
            </section>
        </Show>
        <section class="admin-panel sys-section" aria-labelledby="sys-conn-h">
            <div class="sys-section-head">
                <h2 id="sys-conn-h">"活跃连接"</h2>
                <span>{move || format!("{} 个", conn_count.get())}</span>
            </div>
            <div class="sys-table-scroll">
                <table class="admin-table">
                    <thead>
                        <tr>
                            <th class="admin-table-num">"PID"</th>
                            <th>"用户"</th>
                            <th>"状态"</th>
                            <th class="admin-table-num">"耗时"</th>
                            <th>"查询"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=connections
                            key=|c| {
                                (
                                    c.pid,
                                    c.user.clone(),
                                    c.state.clone(),
                                    c.query.clone(),
                                    c.query_duration_secs.map(f64::to_bits),
                                )
                            }
                            children=|c: ConnInfo| {
                                view! {
                                    <tr>
                                        <td class="admin-table-num">{c.pid}</td>
                                        <td>{c.user.clone()}</td>
                                        <td class="admin-table-muted">
                                            {c.state.clone().unwrap_or_else(|| "—".into())}
                                        </td>
                                        <td class="admin-table-num">
                                            {c
                                                .query_duration_secs
                                                .map(|d| format!("{d:.1} 秒"))
                                                .unwrap_or_else(|| "—".into())}
                                        </td>
                                        <td
                                            class="sys-mono sys-query"
                                            title=c.query.clone().unwrap_or_default()
                                        >
                                            {c.query.clone().unwrap_or_else(|| "—".into())}
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
