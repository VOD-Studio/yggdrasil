//! 后台「系统」页面：服务器、数据库、备份、SQL 控制台、导出五个 tab。
//!
//! tab 状态用 `RwSignal`（不深链 / 不走路由）。各 tab 拆为独立子模块、状态互不
//! 共享，切换时整段重建，旧 tab 的轮询与定时器随 owner 销毁而停止。本文件另提供各
//! tab 共用的指标卡片、进度条、刷新控件与「定时刷新的状态」钩子。样式见
//! styles/pages/admin-system.css。

mod backup;
mod db_status;
mod export;
mod server_status;
mod sql_console;

use std::future::Future;
use std::time::Duration;

use backup::BackupTab;
use db_status::DbStatusTab;
use export::ExportTab;
use leptos::prelude::*;
use leptos_router::{lazy_route, LazyRoute};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use server_status::ServerStatusTab;
use sql_console::SqlConsoleTab;

use crate::api::error::ApiError;
use crate::components::admin_layout::AdminPageHead;
use crate::components::forms::{FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, Icon, IconName, SegmentedControl, SegmentedItem,
};

/// 路由：`/admin/system`。数据全部在客户端读取，没有 SSR 数据。
pub struct SystemRoute;

#[lazy_route]
impl LazyRoute for SystemRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <System/> }.into_any()
    }
}

/// 系统页的五个 tab。
#[derive(Clone, Copy, PartialEq, Debug)]
enum SystemTab {
    /// 服务器状态（应用内 + 主机层 CPU / 内存 / 磁盘、缓存命中率）。
    ServerStatus,
    /// 数据库运行状态（表、索引、连接、迁移版本）。
    DbStatus,
    /// 备份与恢复（pg_dump + 任务进度）。
    Backup,
    /// SQL 控制台（全读写 + 护栏）。
    SqlConsole,
    /// 数据导出（SQL / CSV 流式下载）。
    Export,
}

impl SystemTab {
    /// 变体 → 稳定字符串 key（与基于 String 的 `SegmentedControl` 桥接）。
    fn as_str(&self) -> &'static str {
        match self {
            SystemTab::ServerStatus => "server_status",
            SystemTab::DbStatus => "db_status",
            SystemTab::Backup => "backup",
            SystemTab::SqlConsole => "sql_console",
            SystemTab::Export => "export",
        }
    }

    /// 字符串 key → 变体。未知 / 空串返回 Err（调用方回退到默认 tab）。
    fn from_str(s: &str) -> Result<SystemTab, &'static str> {
        match s {
            "server_status" => Ok(SystemTab::ServerStatus),
            "db_status" => Ok(SystemTab::DbStatus),
            "backup" => Ok(SystemTab::Backup),
            "sql_console" => Ok(SystemTab::SqlConsole),
            "export" => Ok(SystemTab::Export),
            _ => Err("unknown tab key"),
        }
    }
}

/// 自动刷新间隔可选项（毫秒；None = 手动）。
const REFRESH_OPTIONS: [(Option<u32>, &str); 5] = [
    (None, "手动刷新"),
    (Some(1_000), "每 1 秒"),
    (Some(2_000), "每 2 秒"),
    (Some(5_000), "每 5 秒"),
    (Some(10_000), "每 10 秒"),
];

/// URL 编码：只保留 `A-Za-z0-9-_.~`，其余按 UTF-8 字节写成 `%XX`。
/// 导出参数与备份下载链接共用；两端编码一致，SSR 与水合产出同一份 href。
const URL_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

fn urlencode(s: &str) -> String {
    utf8_percent_encode(s, URL_COMPONENT).to_string()
}

/// 按 `refresh_ms` 定时调用 `reload`。间隔变化时清掉旧定时器再建新的（不会叠加多个
/// 轮询）；effect 重跑与 owner 销毁都会清除定时器。effect 只在浏览器执行。
fn use_auto_refresh(refresh_ms: Signal<Option<u32>>, reload: Callback<()>) {
    Effect::new(move |_| {
        let Some(ms) = refresh_ms.get() else {
            return;
        };
        if ms == 0 {
            return;
        }
        let Ok(handle) =
            set_interval_with_handle(move || reload.run(()), Duration::from_millis(u64::from(ms)))
        else {
            return;
        };
        on_cleanup(move || handle.clear());
    });
}

/// 定时刷新的状态：数据、加载中、失败标记、刷新间隔与重新加载回调。
#[derive(Clone, Copy)]
struct PolledStatus<T: Send + Sync + 'static> {
    status: RwSignal<Option<T>>,
    loading: RwSignal<bool>,
    failed: RwSignal<bool>,
    refresh_ms: RwSignal<Option<u32>>,
    reload: Callback<()>,
}

/// 挂载时读取一次，之后按 `refresh_ms` 定时刷新。
///
/// 每次请求领取递增的序号：只应用比已应用结果更新的响应，迟到的旧响应被丢弃
/// （短间隔下较慢的响应也能落地，不会被后续请求饿死）；`loading` 只在最新一次
/// 请求结束时复位。组件销毁后迟到的响应经 `try_*` 静默丢弃。
fn use_polled_status<T, F, Fut>(fetch: F, initial_refresh: Option<u32>) -> PolledStatus<T>
where
    T: Send + Sync + 'static,
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, ApiError>> + 'static,
{
    let status = RwSignal::new(None::<T>);
    let loading = RwSignal::new(true);
    let failed = RwSignal::new(false);
    let refresh_ms = RwSignal::new(initial_refresh);
    // (已发出的最新序号, 已应用的最新序号)
    let sequence = StoredValue::new((0u64, 0u64));
    let reload = Callback::new(move |()| {
        let Some(current) = sequence.try_update_value(|(issued, _)| {
            *issued += 1;
            *issued
        }) else {
            return;
        };
        let _ = loading.try_set(true);
        let request = fetch();
        leptos::task::spawn_local(async move {
            let result = request.await;
            let Some((fresh, latest)) = sequence.try_update_value(|(issued, applied)| {
                let fresh = current > *applied;
                if fresh {
                    *applied = current;
                }
                (fresh, current == *issued)
            }) else {
                return;
            };
            if fresh {
                match result {
                    Ok(value) => {
                        let _ = status.try_set(Some(value));
                        let _ = failed.try_set(false);
                    }
                    Err(_) => {
                        let _ = failed.try_set(true);
                    }
                }
            }
            if latest {
                let _ = loading.try_set(false);
            }
        });
    });
    Effect::new(move |_| reload.run(()));
    use_auto_refresh(refresh_ms.into(), reload);
    PolledStatus {
        status,
        loading,
        failed,
        refresh_ms,
        reload,
    }
}

/// 刷新控件：自动刷新间隔与「立即刷新」。自动刷新时圆点呼吸。
#[component]
fn RefreshControl(
    refresh_ms: RwSignal<Option<u32>>,
    #[prop(into)] loading: Signal<bool>,
    #[prop(into)] on_refresh: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="sys-refresh">
            <span
                class="sys-live"
                data-on=move || refresh_ms.get().is_some().then_some("true")
                aria-hidden="true"
            ></span>
            <FormSelect
                value=refresh_ms
                options=REFRESH_OPTIONS.to_vec()
                trigger_class=FORM_SELECT_COMPACT_CLASS
                aria_label="自动刷新"
                onchange={move |v: Option<u32>| refresh_ms.set(v)}
            />
            <button
                class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                type="button"
                disabled=move || loading.get()
                on:click=move |_| on_refresh.run(())
            >
                <Icon name=IconName::RefreshCw size=14/>
                {move || if loading.get() { "刷新中…" } else { "立即刷新" }}
            </button>
        </div>
    }
}

/// 指标卡片：标签、数值、补充说明，可带进度条等子元素。
///
/// `value` / `meta` 可传字符串或信号；`meta` 为空时不渲染说明行。
#[component]
fn Metric(
    label: &'static str,
    #[prop(into)] value: Signal<String>,
    #[prop(optional, into)] meta: Signal<String>,
    #[prop(optional)] mono: bool,
    #[prop(optional)] small: bool,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="admin-panel sys-metric">
            <span class="sys-metric-label">{label}</span>
            <span
                class="sys-metric-value"
                data-mono=mono.then_some("true")
                data-small=small.then_some("true")
                title=move || value.get()
            >
                {move || value.get()}
            </span>
            {children.map(|children| children())}
            {move || {
                let meta = meta.get();
                (!meta.is_empty()).then(|| view! { <span class="sys-metric-meta">{meta}</span> })
            }}
        </div>
    }
}

/// 占用条。超过 80% 时转为陶土色提醒；`calm` 的不提醒（命中率、占比）。
///
/// `pct` 可传数值或信号：数值变化时节点原地更新，宽度走 CSS 过渡。
#[component]
fn Meter(#[prop(into)] pct: Signal<f64>, #[prop(optional)] calm: bool) -> impl IntoView {
    let pct = Memo::new(move |_| pct.get().clamp(0.0, 100.0));
    view! {
        <span
            class="sys-meter"
            data-warn=move || (!calm && pct.get() >= 80.0).then_some("true")
            aria-hidden="true"
        >
            <span style=move || format!("width: {:.1}%", pct.get())></span>
        </span>
    }
}

/// 系统页入口组件。
#[component]
pub fn System() -> impl IntoView {
    let active_tab = RwSignal::new(SystemTab::ServerStatus);
    let items = vec![
        SegmentedItem::new("server_status".to_string(), "服务器"),
        SegmentedItem::new("db_status".to_string(), "数据库"),
        SegmentedItem::new("backup".to_string(), "备份"),
        SegmentedItem::new("sql_console".to_string(), "SQL 控制台"),
        SegmentedItem::new("export".to_string(), "导出"),
    ];

    view! {
        <div class="admin-system">
            <AdminPageHead title="系统">
                "服务器与数据库状态、备份恢复、SQL 控制台与数据导出。"
            </AdminPageHead>
            <SegmentedControl
                class="sys-tabs"
                aria_label="系统分区"
                items=items
                value=Signal::derive(move || active_tab.get().as_str().to_string())
                on_change=move |v: String| {
                    active_tab.set(SystemTab::from_str(&v).unwrap_or(SystemTab::ServerStatus));
                }
            />
            // 切 tab 时整段重建：真正卸载旧 tab，轮询与状态不残留。
            {move || {
                let tab = active_tab.get();
                view! {
                    <div class="sys-tab">
                        {match tab {
                            SystemTab::ServerStatus => view! { <ServerStatusTab/> }.into_any(),
                            SystemTab::DbStatus => view! { <DbStatusTab/> }.into_any(),
                            SystemTab::Backup => view! { <BackupTab/> }.into_any(),
                            SystemTab::SqlConsole => view! { <SqlConsoleTab/> }.into_any(),
                            SystemTab::Export => view! { <ExportTab/> }.into_any(),
                        }}
                    </div>
                }
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::{urlencode, SystemTab};

    #[test]
    fn as_str_roundtrips_all_variants() {
        // 每个变体经 as_str -> from_str 必须还原为自身。
        for tab in [
            SystemTab::DbStatus,
            SystemTab::ServerStatus,
            SystemTab::SqlConsole,
            SystemTab::Export,
            SystemTab::Backup,
        ] {
            let s = tab.as_str();
            assert_eq!(
                SystemTab::from_str(s),
                Ok(tab),
                "roundtrip failed for {tab:?}"
            );
        }
    }

    #[test]
    fn as_str_returns_stable_keys() {
        // 字符串 key 必须稳定(改 key 会破坏 URL/调试/未来持久化),锁定之。
        assert_eq!(SystemTab::DbStatus.as_str(), "db_status");
        assert_eq!(SystemTab::ServerStatus.as_str(), "server_status");
        assert_eq!(SystemTab::SqlConsole.as_str(), "sql_console");
        assert_eq!(SystemTab::Export.as_str(), "export");
        assert_eq!(SystemTab::Backup.as_str(), "backup");
    }

    #[test]
    fn from_str_rejects_unknown_and_empty() {
        assert!(SystemTab::from_str("nonsense").is_err());
        assert!(SystemTab::from_str("").is_err());
        // 大小写敏感:不接受大写变体,避免歧义。
        assert!(SystemTab::from_str("DbStatus").is_err());
    }

    #[test]
    fn urlencode_keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(urlencode("auto_2026-01.sql~"), "auto_2026-01.sql~");
        assert_eq!(urlencode("a b&c=中"), "a%20b%26c%3D%E4%B8%AD");
    }
}
