//! 后台「系统」页面：服务器、数据库、备份、SQL 控制台、导出五个 tab。
//!
//! tab 状态用 `use_signal`（不深链 / 不走路由）。各 tab 拆为独立子模块、状态互不
//! 共享，切换时以 key 重建，旧 tab 的轮询随之停止。本文件另提供各 tab 共用的指标
//! 卡片、进度条与刷新控件。样式见 styles/pages/admin-system.css。

mod backup;
mod db_status;
mod export;
mod server_status;
mod sql_console;

use backup::BackupTab;
use db_status::DbStatusTab;
use dioxus::prelude::*;
use export::ExportTab;
use server_status::ServerStatusTab;
use sql_console::SqlConsoleTab;

use crate::components::forms::{FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::ui::{button_class, ButtonSize, ButtonVariant, FilterTabs, Icon, IconName};

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
    /// 变体 → 稳定字符串 key（与基于 String 的 `FilterTabs` 桥接）。
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

/// 按 `refresh_ms` 定时调用 `reload`。单一长生命周期循环，每轮读取最新间隔，
/// 切换间隔不会叠加多个轮询。
fn use_auto_refresh(refresh_ms: Signal<Option<u32>>, reload: Callback<()>) {
    use_future(move || async move {
        loop {
            let ms = refresh_ms.peek().unwrap_or(0);
            if ms == 0 {
                crate::utils::time::sleep_ms(200).await;
                continue;
            }
            crate::utils::time::sleep_ms(ms).await;
            if refresh_ms.peek().is_some() {
                reload.call(());
            }
        }
    });
}

/// 刷新控件：自动刷新间隔与「立即刷新」。自动刷新时圆点呼吸。
#[component]
fn RefreshControl(
    mut refresh_ms: Signal<Option<u32>>,
    loading: bool,
    on_refresh: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "sys-refresh",
            span { class: "sys-live", "data-on": refresh_ms().is_some().then_some("true"), aria_hidden: "true" }
            FormSelect {
                value: refresh_ms(),
                options: REFRESH_OPTIONS.to_vec(),
                trigger_class: FORM_SELECT_COMPACT_CLASS,
                aria_label: "自动刷新",
                onchange: move |v| refresh_ms.set(v),
            }
            button {
                class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                r#type: "button",
                disabled: loading,
                onclick: move |_| on_refresh.call(()),
                Icon { name: IconName::RefreshCw, size: 14 }
                if loading { "刷新中…" } else { "立即刷新" }
            }
        }
    }
}

/// 指标卡片：标签、数值、补充说明，可带进度条等子元素。
#[component]
fn Metric(
    label: &'static str,
    #[props(into)] value: String,
    #[props(default)] meta: String,
    #[props(default)] mono: bool,
    #[props(default)] small: bool,
    children: Element,
) -> Element {
    rsx! {
        div { class: "admin-panel sys-metric",
            span { class: "sys-metric-label", "{label}" }
            span {
                class: "sys-metric-value",
                "data-mono": mono.then_some("true"),
                "data-small": small.then_some("true"),
                title: "{value}",
                "{value}"
            }
            {children}
            if !meta.is_empty() {
                span { class: "sys-metric-meta", "{meta}" }
            }
        }
    }
}

/// 占用条。超过 80% 时转为陶土色提醒；`calm` 的不提醒（命中率、占比）。
#[component]
fn Meter(pct: f64, #[props(default)] calm: bool) -> Element {
    let pct = pct.clamp(0.0, 100.0);
    rsx! {
        span { class: "sys-meter", "data-warn": (!calm && pct >= 80.0).then_some("true"), aria_hidden: "true",
            span { style: "width: {pct:.1}%" }
        }
    }
}

/// 系统页入口组件。
#[component]
pub fn System() -> Element {
    let mut active_tab = use_signal(|| SystemTab::ServerStatus);

    rsx! {
        div { class: "admin-system",
            div { class: "admin-page-head",
                div {
                    h1 { "系统" }
                    p { "服务器与数据库状态、备份恢复、SQL 控制台与数据导出。" }
                }
            }
            FilterTabs {
                class: "sys-tabs",
                aria_label: "系统分区",
                items: vec![
                    ("server_status", "服务器"),
                    ("db_status", "数据库"),
                    ("backup", "备份"),
                    ("sql_console", "SQL 控制台"),
                    ("export", "导出"),
                ],
                active_value: active_tab().as_str().to_string(),
                on_change: move |v: String| {
                    active_tab.set(SystemTab::from_str(&v).unwrap_or(SystemTab::ServerStatus));
                },
            }
            // 经 std::iter::once 包一层 keyed remount：Dioxus 0.7 会忽略非列表元素上的裸
            // key。切 tab 时真正卸载旧 tab，轮询与 hook 状态不残留。
            for tab_key in std::iter::once(active_tab().as_str()) {
                div { key: "{tab_key}", class: "sys-tab",
                    match active_tab() {
                        SystemTab::ServerStatus => rsx! {
                            ServerStatusTab {}
                        },
                        SystemTab::DbStatus => rsx! {
                            DbStatusTab {}
                        },
                        SystemTab::Backup => rsx! {
                            BackupTab {}
                        },
                        SystemTab::SqlConsole => rsx! {
                            SqlConsoleTab {}
                        },
                        SystemTab::Export => rsx! {
                            ExportTab {}
                        },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SystemTab;

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
}
