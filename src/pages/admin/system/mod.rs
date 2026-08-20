//! 后台系统管理页面（数据库 + 服务器状态 + SQL 控制台 + 导出 + 备份）。
//!
//! 用顶部 tab 切换 5 个功能，tab 状态用 `use_signal`（不深链 / 不走分页路由）。
//! 各 tab 拆分为独立子模块，状态完全独立、互不共享——切换 tab 时父组件用 `key`
//! 强制卸载旧 tab 组件，其内部 signal 随之销毁。

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

use crate::components::ui::FilterTabs;

/// 系统管理的 5 个功能 tab。
#[derive(Clone, Copy, PartialEq, Debug)]
enum SystemTab {
    /// 数据库运行状态（表/连接/死元组/迁移版本）。
    DbStatus,
    /// 服务器状态（应用内 + 主机层 CPU/内存/磁盘）。
    ServerStatus,
    /// SQL 控制台（全读写 + 护栏）。
    SqlConsole,
    /// 数据导出（SQL/CSV 流式下载）。
    Export,
    /// 备份恢复（pg_dump + 任务进度）。
    Backup,
}

impl SystemTab {
    /// 变体 → 稳定字符串 key(用于与基于 String 的 `FilterTabs` 组件桥接)。
    /// 改这些 key 会破坏潜在的持久化/调试场景,见 `from_str` 的反向映射。
    fn as_str(&self) -> &'static str {
        match self {
            SystemTab::DbStatus => "db_status",
            SystemTab::ServerStatus => "server_status",
            SystemTab::SqlConsole => "sql_console",
            SystemTab::Export => "export",
            SystemTab::Backup => "backup",
        }
    }

    /// 字符串 key → 变体。未知/空串返回 Err(调用方 fallback 到默认 tab)。
    /// 与 `as_str` 严格对应;大小写敏感。
    fn from_str(s: &str) -> Result<SystemTab, &'static str> {
        match s {
            "db_status" => Ok(SystemTab::DbStatus),
            "server_status" => Ok(SystemTab::ServerStatus),
            "sql_console" => Ok(SystemTab::SqlConsole),
            "export" => Ok(SystemTab::Export),
            "backup" => Ok(SystemTab::Backup),
            _ => Err("unknown tab key"),
        }
    }
}

/// 系统管理入口组件。
#[component]
pub fn System() -> Element {
    // tab 状态：默认进第一个 tab（数据库状态）。用 signal 而非 URL query——
    // tab 切换无需深链/书签，避免新增路由变体。
    let mut active_tab = use_signal(|| SystemTab::DbStatus);

    rsx! {
        div { class: "animate-page-enter w-full max-w-7xl mx-auto space-y-6",
            // 页面标题
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70 mb-6",
                div {
                    h1 { class: "text-3xl sm:text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                        "系统面板"
                    }
                    p { class: "text-sm text-[var(--color-paper-secondary)] mt-1.5",
                        "数据库健康状态、服务器性能指标与系统诊断中心"
                    }
                }
                div { class: "flex items-center gap-2.5",
                    div { class: "inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-full text-xs font-mono bg-[var(--color-paper-entry)] text-[var(--color-paper-secondary)] border border-[var(--color-paper-border)]/70 shadow-2xs",
                        span { class: "w-1.5 h-1.5 rounded-full bg-[var(--color-paper-accent)]" }
                        span { "诊断引擎在线" }
                    }
                }
            }
            // 顶部 tab 切换栏:复用公共 FilterTabs 组件(String API,经 as_str/from_str 桥接枚举)。
            // 视觉与评论页一致:平滑滑动指示条 + 选中文字 text-paper-primary。
            FilterTabs {
                items: vec![
                    ("db_status", "数据库状态"),
                    ("server_status", "服务器状态"),
                    ("sql_console", "SQL 控制台"),
                    ("export", "数据导出"),
                    ("backup", "备份恢复"),
                ],
                active_value: active_tab().as_str().to_string(),
                on_change: move |v: String| {
                    // 未知 key fallback 到默认 tab,保证状态始终有效。
                    active_tab.set(SystemTab::from_str(&v).unwrap_or(SystemTab::DbStatus));
                },
            }

            // tab 内容
            // 经 std::iter::once 包一层 keyed remount：Dioxus 0.7 对非列表元素的裸 key
            // 会忽略（见 post_detail.rs 头文档约定 #5、settings/mod.rs 同款修复）。
            // once 让 active_tab() 变化时真正卸载/重建内层 div，既保证 hook slot
            // 不复用（DelayedSkeleton 的 visible 信号不残留），又让 animate-section-enter
            // 在每次切 tab 时重播。
            for tab_key in std::iter::once(active_tab().as_str()) {
                div { key: "{tab_key}", class: "animate-section-enter",
                    match active_tab() {
                        SystemTab::DbStatus => rsx! {
                            DbStatusTab {}
                        },
                        SystemTab::ServerStatus => rsx! {
                            ServerStatusTab {}
                        },
                        SystemTab::SqlConsole => rsx! {
                            SqlConsoleTab {}
                        },
                        SystemTab::Export => rsx! {
                            ExportTab {}
                        },
                        SystemTab::Backup => rsx! {
                            BackupTab {}
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
