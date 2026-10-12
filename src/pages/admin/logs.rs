//! 管理后台「运行日志」页面（`/admin/logs`，侧边栏「工具」组）。
//!
//! 数据流：筛选变更（级别 chips / target 选择器 / 关键字）→ 变更 `filter_key` →
//! `LogsStreamPane` key-based remount（同 runner.rs 语言切换模式）：卸载即
//! `use_drop` close EventSource，挂载即重新拉历史（`get_logs` limit 200）并
//! 重连 SSE `/api/logs/stream`。SSE 实时行追加到列表尾部并按 id 去重——注意
//! 实时事件 id 恒为 0（未落库无 id，见 backend 契约），故去重只对历史行
//! （id != 0）生效，实时行到达即追加、绝不参与游标计算。
//!
//! 渲染纯度：render body 无任何副作用；网络 / DOM / EventSource 全部在
//! `use_effect` / 事件闭包 / `#[cfg(target_arch = "wasm32")]` 门控块内。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::logs::{export_logs, get_log_targets};
use crate::components::forms::{FormInput, ToggleSwitch, INPUT_INLINE_CLASS};
use crate::components::ui::LoadingButton;

/// 全部可选级别（多选 chips，顺序即展示与服务端白名单顺序）。
const ALL_LEVELS: [&str; 5] = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];
/// 运行日志页面：页头（状态/跟随/导出）+ 筛选栏 + 清理策略 + 实时日志流。
///
/// internal-scroll 路由（AdminLayout 已切到卡片不滚动变体）：本页自组织
/// 「固定页头/筛选 + flex-1 内部滚动日志区」的分区布局，同 settings 页模式。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn Logs() -> Element {
    // —— 已应用筛选（任何变化都会 remount 下方 LogsStreamPane）——
    let mut levels: Signal<Vec<&'static str>> = use_signal(|| vec!["ERROR", "WARN", "INFO"]);
    let mut target: Signal<Option<String>> = use_signal(|| None);
    let mut query: Signal<String> = use_signal(String::new);
    // 关键字草稿：Enter 提交、清空即时生效，避免每击键都重连 SSE。
    let mut query_draft: Signal<String> = use_signal(String::new);

    // —— 页头状态 ——
    let mut follow: Signal<bool> = use_signal(|| true);
    let connected: Signal<bool> = use_signal(|| false);
    let dropped: Signal<u64> = use_signal(|| 0);
    let mut exporting: Signal<bool> = use_signal(|| false);

    // target 候选列表：挂载后拉取一次（服务端 moka 缓存 60s；筛选 remount
    // 只重建子面板，本页不重拉）。
    let mut targets: Signal<Vec<String>> = use_signal(Vec::new);
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            if let Ok(list) = get_log_targets().await {
                targets.set(list);
            }
        });
    });

    // 导出反馈提示条（ygg-toast 高度过渡 + 3s 自动消失，同 settings/profile 模式）。
    let mut toast_state: Signal<Option<(String, bool)>> = use_signal(|| None);
    let mut display_msg: Signal<String> = use_signal(String::new);
    let mut display_err: Signal<bool> = use_signal(|| false);
    use_effect(move || {
        if let Some((msg, is_err)) = toast_state() {
            display_msg.set(msg.clone());
            display_err.set(is_err);
            let key = msg.clone();
            spawn(async move {
                crate::utils::time::sleep_ms(3000).await;
                // 仅当未被新提示覆盖时清除。
                if toast_state().map(|(m, _)| m == key).unwrap_or(false) {
                    toast_state.set(None);
                }
            });
        }
    });

    // 行内 target 点击 → 设为 target 筛选（触发 remount）。
    let on_pick_target: Callback<String> = Callback::new(move |t: String| target.set(Some(t)));

    // 筛选签名：作为 remount key（级别顺序固定、target/query 原样拼接）。
    let filter_key = format!(
        "{}|{}|{}",
        levels().join(","),
        target().unwrap_or_default(),
        query()
    );

    let subtitle = if dropped() > 0 {
        format!(
            "进程内结构化日志 · 历史查询 + 实时流 · 启动以来丢弃 {} 条",
            dropped()
        )
    } else {
        "进程内结构化日志 · 历史查询 + 实时流".to_string()
    };

    rsx! {
        div { class: "animate-page-enter w-full flex-1 min-h-0 flex flex-col px-6 py-8",
            // 页头（固定，不随日志区滚动）
            div { class: "flex-shrink-0 flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70 mb-4",
                div {
                    h1 { class: "text-3xl sm:text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                        "运行日志"
                    }
                    p { class: "text-sm text-[var(--color-paper-secondary)] mt-1.5", "{subtitle}" }
                }
                div { class: "flex items-center gap-3 flex-wrap",
                    // 实时连接状态指示（绿 = 已连接 / 灰 = 断开）
                    div { class: "inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-full text-xs bg-[var(--color-paper-entry)] text-[var(--color-paper-secondary)] border border-[var(--color-paper-border)]/70 shadow-2xs",
                        span {
                            class: if connected() { "w-1.5 h-1.5 rounded-full bg-[var(--color-paper-accent)]" } else { "w-1.5 h-1.5 rounded-full bg-[var(--color-paper-tertiary)]" },
                        }
                        span { if connected() { "已连接" } else { "未连接" } }
                    }
                    // 自动跟随开关
                    label { class: "flex items-center gap-2 cursor-pointer select-none",
                        span { class: "text-xs text-[var(--color-paper-secondary)]", "自动跟随" }
                        ToggleSwitch {
                            checked: follow(),
                            ontoggle: move |_| follow.set(!follow()),
                        }
                    }
                    // 导出按钮：按当前筛选导出为 .log 文件
                    LoadingButton {
                        label: "导出日志".to_string(),
                        loading: exporting(),
                        variant: "sm",
                        onclick: move |_| {
                            #[cfg(target_arch = "wasm32")]
                            {
                                if *exporting.peek() {
                                    return;
                                }
                                exporting.set(true);
                                let lv: Vec<String> = levels().iter().map(|s| s.to_string()).collect();
                                let t = target();
                                let q = query();
                                spawn(async move {
                                    match export_logs(lv, t, if q.is_empty() { None } else { Some(q) }).await {
                                        Ok(content) => {
                                            let count = content.lines().count();
                                            download_text_file(&log_export_filename(), &content);
                                            toast_state.set(Some((format!("已导出 {count} 条日志"), false)));
                                        }
                                        Err(_) => {
                                            toast_state.set(Some(("日志导出失败，请重试".to_string(), true)));
                                        }
                                    }
                                    exporting.set(false);
                                });
                            }
                        },
                    }
                }
            }

            // 操作提示条（导出反馈）
            div { class: if toast_state().is_some() { "ygg-toast is-open" } else { "ygg-toast" },
                div { class: if display_err() { "ygg-toast-inner text-sm rounded-lg px-3 py-2 bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300" } else { "ygg-toast-inner text-sm rounded-lg px-3 py-2 bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300" },
                    "{display_msg()}"
                }
            }

            // 筛选栏（固定）：级别多选 chips + target 选择器 + 关键字
            div { class: "flex-shrink-0 bg-[var(--color-paper-entry)]/40 rounded-2xl border border-[var(--color-paper-border)]/70 shadow-xs p-4 sm:p-5 mb-4 space-y-4",
                div { class: "flex flex-wrap items-center gap-2",
                    for level in ALL_LEVELS {
                        button {
                            key: "{level}",
                            r#type: "button",
                            class: if levels().contains(&level) { "inline-flex items-center px-3 py-1 text-xs font-mono font-medium rounded-full transition-all cursor-pointer bg-[var(--color-paper-accent)] text-[var(--color-paper-theme)] shadow-2xs" } else { "inline-flex items-center px-3 py-1 text-xs font-mono font-medium rounded-full transition-all cursor-pointer text-[var(--color-paper-secondary)] bg-[var(--color-paper-entry)] hover:bg-[var(--color-paper-theme)] hover:text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]/70" },
                            onclick: move |_| {
                                levels.with_mut(|sel| {
                                    if let Some(pos) = sel.iter().position(|l| *l == level) {
                                        sel.remove(pos);
                                    } else {
                                        sel.push(level);
                                        // 保持 ALL_LEVELS  canonical 顺序。
                                        sel.sort_by_key(|l| {
                                            ALL_LEVELS.iter().position(|a| a == l).unwrap_or(usize::MAX)
                                        });
                                    }
                                });
                            },
                            "{level}"
                        }
                    }
                    if levels().is_empty() {
                        span { class: "text-xs text-[var(--color-paper-tertiary)]", "未选择级别时显示全部" }
                    }
                }
                div { class: "flex flex-col sm:flex-row sm:items-center gap-3",
                    TargetSelect {
                        value: target(),
                        options: targets(),
                        onchange: move |v: Option<String>| target.set(v),
                    }
                    FormInput {
                        r#type: "search",
                        placeholder: "搜索日志内容，Enter 生效",
                        value: query_draft(),
                        class: Some(INPUT_INLINE_CLASS),
                        oninput: move |v: String| {
                            query_draft.set(v.clone());
                            // 原生 search 清空按钮：清空即恢复无关键字。
                            if v.is_empty() && !query().is_empty() {
                                query.set(String::new());
                            }
                        },
                        onkeydown: move |e: KeyboardEvent| {
                            if e.key() == Key::Enter {
                                query.set(query_draft());
                            }
                        },
                    }
                }
            }

            // 清理策略（折叠卡，固定区）
            div { class: "flex-shrink-0 mb-4",
                CleanupPolicy {}
            }

            // 筛选变更 → key-based remount：卸载旧面板（use_drop close
            // EventSource）→ 挂载新面板（重拉历史 + 重连 SSE）。
            for key in std::iter::once(filter_key) {
                div { key: "{key}", class: "flex-1 min-h-0 flex flex-col",
                    LogsStreamPane {
                        levels: levels().iter().map(|s| s.to_string()).collect(),
                        target: target(),
                        query: query(),
                        follow,
                        connected,
                        dropped,
                        on_pick_target,
                    }
                }
            }
        }
    }
}

mod cleanup;
#[cfg(target_arch = "wasm32")]
mod export;
mod stream;
mod target_selector;
use cleanup::CleanupPolicy;
#[cfg(target_arch = "wasm32")]
use export::{download_text_file, log_export_filename};
use stream::LogsStreamPane;
use target_selector::TargetSelect;
