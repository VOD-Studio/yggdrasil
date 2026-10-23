//! 管理后台「运行日志」页面（`/admin/logs`）。
//!
//! 数据流：筛选变更（级别开关 / target 下拉 / 关键字）→ 变更 `filter_key` →
//! `LogsStreamPane` key-based remount：卸载即 `use_drop` close EventSource，挂载即
//! 重新拉历史（`get_logs` limit 200）并重连 SSE `/api/logs/stream`。SSE 实时行追加到
//! 列表尾部并按 id 去重——实时事件 id 恒为 0（未落库），去重只对历史行生效。
//!
//! 页面在主内容区内部滚动：标题行与筛选栏固定，日志区占满剩余高度。导出按当前筛选
//! 下载 .log 文件；清理策略在「⋯」展开的面板里。样式见 styles/pages/admin-logs.css。
use crate::components::admin_layout::AdminPageHead;
use dioxus::prelude::*;

use crate::components::forms::{FormSelect, ToggleSwitch, FORM_SELECT_COMPACT_CLASS};
use crate::components::ui::{button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconName};

/// 全部可选级别（顺序即展示与服务端白名单顺序）。
const ALL_LEVELS: [&str; 5] = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

/// 运行日志页面。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn Logs() -> Element {
    let toast = use_toast();
    // —— 已应用筛选（任何变化都会 remount 下方 LogsStreamPane）——
    let mut levels: Signal<Vec<&'static str>> = use_signal(|| vec!["ERROR", "WARN", "INFO"]);
    let mut target: Signal<Option<String>> = use_signal(|| None);
    let mut query: Signal<String> = use_signal(String::new);
    // 关键字草稿：Enter 提交、清空即时生效，避免每击键都重连 SSE。
    let mut query_draft: Signal<String> = use_signal(String::new);
    let mut follow: Signal<bool> = use_signal(|| true);
    let connected: Signal<bool> = use_signal(|| false);
    let dropped: Signal<u64> = use_signal(|| 0);
    let mut exporting: Signal<bool> = use_signal(|| false);
    let mut cleanup_open = use_signal(|| false);
    // target 候选：挂载后拉取一次（服务端缓存 60s）。
    let mut targets: Signal<Vec<String>> = use_signal(Vec::new);
    use_hook(move || {
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            if let Ok(list) = crate::api::logs::get_log_targets().await {
                targets.set(list);
            }
        });
    });

    // 行内 target 点击 → 设为 target 筛选（触发 remount）。
    let on_pick_target: Callback<String> = Callback::new(move |t: String| target.set(Some(t)));
    let filter_key = format!(
        "{}|{}|{}",
        levels().join(","),
        target().unwrap_or_default(),
        query()
    );
    let mut target_options: Vec<(Option<String>, String)> = vec![(None, "全部 target".into())];
    target_options.extend(targets().into_iter().map(|t| (Some(t.clone()), t)));
    let export = move |_| {
        if exporting() {
            return;
        }
        exporting.set(true);
        let lv: Vec<String> = levels().iter().map(|s| s.to_string()).collect();
        let (t, q) = (target(), query());
        spawn(async move {
            match crate::api::logs::export_logs(lv, t, (!q.is_empty()).then_some(q)).await {
                Ok(content) => {
                    let count = content.lines().count();
                    #[cfg(target_arch = "wasm32")]
                    download_text_file(&log_export_filename(), &content);
                    toast.success(format!("已导出 {count} 条日志"));
                }
                Err(_) => toast.error("日志导出失败，请重试"),
            }
            exporting.set(false);
        });
    };

    rsx! {
        div { class: "logs-page",
            AdminPageHead {
                title: "运行日志",
                class: "logs-head",
                actions: rsx! {
                    span { class: "logs-live", "data-on": connected().then_some("true"), role: "status",
                        span { class: "logs-live-dot", aria_hidden: "true" }
                        if connected() { "已连接" } else { "未连接" }
                    }
                    button {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                        r#type: "button",
                        aria_expanded: "{cleanup_open()}",
                        onclick: move |_| cleanup_open.toggle(),
                        Icon { name: IconName::SlidersHorizontal, size: 15 }
                        "清理策略"
                    }
                    button {
                        class: button_class(ButtonVariant::Secondary, ButtonSize::Md),
                        r#type: "button",
                        disabled: exporting(),
                        onclick: export,
                        Icon { name: IconName::Download, size: 15 }
                        if exporting() { "导出中…" } else { "导出日志" }
                    }
                },
                "进程内结构化日志，历史查询加实时流。"
                if dropped() > 0 {
                    "启动以来丢弃 {dropped()} 条。"
                }
            }
            if cleanup_open() {
                CleanupPolicy { on_close: move |_| cleanup_open.set(false) }
            }
            div { class: "logs-filters",
                div { class: "logs-levels", role: "group", aria_label: "日志级别",
                    for level in ALL_LEVELS {
                        button {
                            key: "{level}",
                            r#type: "button",
                            class: "logs-level-toggle",
                            "data-level": level,
                            aria_pressed: "{levels().contains(&level)}",
                            onclick: move |_| {
                                levels.with_mut(|sel| {
                                    if let Some(pos) = sel.iter().position(|l| *l == level) {
                                        sel.remove(pos);
                                    } else {
                                        sel.push(level);
                                        sel.sort_by_key(|l| ALL_LEVELS.iter().position(|a| a == l).unwrap_or(usize::MAX));
                                    }
                                });
                            },
                            "{level}"
                        }
                    }
                }
                FormSelect {
                    id: "logs-target",
                    value: target(),
                    options: target_options,
                    trigger_class: FORM_SELECT_COMPACT_CLASS,
                    aria_label: "按 target 筛选",
                    onchange: move |v: Option<String>| target.set(v),
                }
                label { class: "field field-sm admin-filter-search logs-search",
                    Icon { name: IconName::Search, size: 14 }
                    span { class: "sr-only", "搜索日志" }
                    input {
                        r#type: "search",
                        placeholder: "搜索日志内容，Enter 生效",
                        value: "{query_draft}",
                        oninput: move |e| {
                            let v = e.value();
                            // 清空即恢复无关键字。
                            if v.is_empty() && !query().is_empty() {
                                query.set(String::new());
                            }
                            query_draft.set(v);
                        },
                        onkeydown: move |e| {
                            if e.key() == Key::Enter && !e.data().is_composing() {
                                query.set(query_draft());
                            }
                        },
                    }
                }
                label { class: "logs-follow",
                    ToggleSwitch {
                        checked: follow(),
                        aria_label: "自动跟随".to_string(),
                        ontoggle: move |_| follow.toggle(),
                    }
                    "自动跟随"
                }
            }
            if levels().is_empty() {
                p { class: "logs-hint", "没有选择级别时显示全部。" }
            }
            // 筛选变更 → key-based remount：卸载旧面板（close EventSource）→ 挂载新面板。
            for key in std::iter::once(filter_key) {
                div { key: "{key}", class: "logs-pane-slot",
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
use cleanup::CleanupPolicy;
#[cfg(target_arch = "wasm32")]
use export::{download_text_file, log_export_filename};
use stream::LogsStreamPane;
