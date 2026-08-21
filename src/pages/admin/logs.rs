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

// server function 仅在 WASM 事件闭包内调用（SSR 下 unused），按项目惯例放行。
#[allow(unused_imports)]
use crate::api::logs::{
    export_logs, get_log_settings, get_log_targets, get_logs, update_log_settings,
};
use crate::components::forms::{FormInput, ToggleSwitch, INPUT_INLINE_CLASS};
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{
    CollapsibleSettingsCard, LoadingButton, BADGE_BASE, BTN_GHOST,
};
use crate::models::log::{LogEntry, LogSettings};

/// 全部可选级别（多选 chips，顺序即展示与服务端白名单顺序）。
const ALL_LEVELS: [&str; 5] = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];
/// 首屏历史加载条数。
/// 以下常量与 LogRowKind 仅在 wasm32 门控块内构造（SSR 不加载数据），
/// server target 会报 dead_code，按本文件组件同款 cfg_attr 放行。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const INITIAL_LIMIT: i32 = 200;
/// 「加载更早」单次翻页条数。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const OLDER_PAGE_LIMIT: i32 = 200;
/// 判定「贴底」的阈值（px）：距底小于该值时新行自动跟随滚动。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const STICK_THRESHOLD_PX: i32 = 48;
/// 列表缓冲上限（行）：超出时丢弃最旧行，保护 DOM 规模。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const MAX_BUFFER_ROWS: usize = 2000;
/// 触发上限时一次性裁掉的行数（留缓冲，避免每行都触发 drain）。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const BUFFER_TRIM: usize = 500;

/// LogsStreamPane 实例计数器（滚动容器 id 跨实例全局唯一，同 FormSelect 模式）。
static LOG_STREAM_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

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

/// 清理策略子组件：保留天数 + 最大行数，复用共享可折叠设置卡片。
///
/// 模式镜像 posts_trash.rs `AutoPurgeSettings`：草稿信号 + `use_memo` dirty +
/// LoadingButton 保存 + just_saved 反馈，加载/保存均在组件内部完成。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn CleanupPolicy() -> Element {
    let mut saved: Signal<LogSettings> = use_signal(LogSettings::default);
    let mut draft_days: Signal<String> = use_signal(|| "7".to_string());
    let mut draft_rows: Signal<String> = use_signal(|| "100000".to_string());
    let mut saving: Signal<bool> = use_signal(|| false);
    // 保存成功后的短暂反馈标记（用户再次编辑时清除）。
    let mut just_saved: Signal<bool> = use_signal(|| false);

    // 挂载即拉取一次服务端配置（缺键时服务端回退默认值）。
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            if let Ok(s) = get_log_settings().await {
                draft_days.set(s.retention_days.to_string());
                draft_rows.set(s.max_rows.to_string());
                saved.set(s);
            }
        });
    });

    // 草稿相对已保存配置是否存在差异（use_memo：依赖不变不重算 parse）。
    let dirty = use_memo(move || {
        let days_diff = draft_days()
            .trim()
            .parse::<i32>()
            .ok()
            .map(|d| d != saved().retention_days)
            .unwrap_or(true);
        let rows_diff = draft_rows()
            .trim()
            .parse::<i32>()
            .ok()
            .map(|r| r != saved().max_rows)
            .unwrap_or(true);
        days_diff || rows_diff
    });

    rsx! {
        CollapsibleSettingsCard {
            title: "清理策略".to_string(),
            summary: format!("保留 {} 天 · 上限 {} 行", saved().retention_days, saved().max_rows),
            enabled: true,
            on_toggle: move |_| just_saved.set(false),
            div { class: "border-t border-paper-border p-5 space-y-6",
                // 保留天数行
                div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-3",
                    div { class: "min-w-0",
                        div { class: "text-sm font-medium text-paper-primary", "保留天数" }
                        div { class: "text-xs text-paper-secondary mt-1",
                            "超过保留期的日志将被后台任务自动清理（1–90 天）"
                        }
                    }
                    div { class: "flex items-center gap-2 flex-shrink-0",
                        div { class: "w-28",
                            FormInput {
                                r#type: "number",
                                placeholder: "7",
                                value: draft_days(),
                                oninput: move |v: String| {
                                    draft_days.set(v);
                                    just_saved.set(false);
                                },
                            }
                        }
                        span { class: "text-xs text-paper-secondary", "天" }
                    }
                }

                // 最大行数行
                div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-3",
                    div { class: "min-w-0",
                        div { class: "text-sm font-medium text-paper-primary", "最大行数" }
                        div { class: "text-xs text-paper-secondary mt-1",
                            "日志表行数上限，超出后按最旧优先裁剪（1000–1000000）"
                        }
                    }
                    div { class: "flex items-center gap-2 flex-shrink-0",
                        div { class: "w-28",
                            FormInput {
                                r#type: "number",
                                placeholder: "100000",
                                value: draft_rows(),
                                oninput: move |v: String| {
                                    draft_rows.set(v);
                                    just_saved.set(false);
                                },
                            }
                        }
                        span { class: "text-xs text-paper-secondary", "行" }
                    }
                }

                // 底部操作行：草稿状态提示 + 保存按钮
                div { class: "flex items-center justify-between gap-4 pt-1",
                    if just_saved() {
                        span { class: "inline-flex items-center gap-1.5 text-xs text-paper-accent",
                            svg {
                                class: "w-3.5 h-3.5",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2.5",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M5 13l4 4L19 7",
                                }
                            }
                            "已保存"
                        }
                    } else if dirty() {
                        span { class: "text-xs text-paper-secondary", "有未保存的更改" }
                    } else {
                        span { class: "text-xs text-transparent select-none", "·" }
                    }
                    LoadingButton {
                        label: "保存设置".to_string(),
                        loading: saving(),
                        disabled: just_saved() || !dirty(),
                        variant: "sm",
                        onclick: move |_| {
                            // 与服务端 clamp 一致的客户端预收敛（服务端仍会兜底）。
                            let days = draft_days().trim().parse::<i32>().unwrap_or(7).clamp(1, 90);
                            let max_rows = draft_rows()
                                .trim()
                                .parse::<i32>()
                                .unwrap_or(100_000)
                                .clamp(1000, 1_000_000);
                            saving.set(true);
                            spawn(async move {
                                if let Ok(s) = update_log_settings(days, max_rows).await {
                                    // 先拷出字段再 set（LogSettings 非 Copy，set 会 move）。
                                    let saved_days = s.retention_days;
                                    let saved_rows = s.max_rows;
                                    saved.set(s);
                                    draft_days.set(saved_days.to_string());
                                    draft_rows.set(saved_rows.to_string());
                                    just_saved.set(true);
                                }
                                saving.set(false);
                            });
                        },
                    }
                }
            }
        }
    }
}

/// TargetSelect 实例计数器（跨实例全局唯一，同 FormSelect 模式）。
static TARGET_SELECT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// target 筛选选择器：触发器 + 下拉列表面板。
///
/// 选项为动态 target 列表（含「全部 target」清空项与暂无 target 提示），
/// 列表项采用等宽字体展示 Rust module 路径。
/// 触发器与面板尺寸、圆角、层级和动画完全对齐 [`FormSelect`] 与 [`INPUT_INLINE_CLASS`]。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn TargetSelect(
    value: Option<String>,
    options: Vec<String>,
    onchange: EventHandler<Option<String>>,
) -> Element {
    let id_prefix = use_hook(|| TARGET_SELECT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
    let trigger_id = format!("target-select-{id_prefix}");

    #[cfg(target_arch = "wasm32")]
    let trigger_id_click = trigger_id.clone();
    #[cfg(target_arch = "wasm32")]
    let trigger_id_keys = trigger_id.clone();

    let mut open = use_signal(|| false);
    #[allow(unused_mut)]
    let mut flip_up = use_signal(|| false);

    let current_label = value.clone().unwrap_or_else(|| "全部 target".to_string());
    let chevron_rotate = if open() { "rotate-180" } else { "" };
    let total_options = options.len() + 1;

    let placement_cls = if flip_up() {
        "bottom-full mb-1.5 origin-bottom"
    } else {
        "top-full mt-1.5 origin-top"
    };

    rsx! {
        div { class: "relative flex-shrink-0 w-full sm:w-60",
            button {
                id: "{trigger_id}",
                r#type: "button",
                class: "relative inline-flex w-full items-center justify-between cursor-pointer select-none text-left pl-4 pr-10 py-2 border border-paper-border rounded-2xl bg-paper-entry text-paper-primary hover:bg-paper-theme focus:outline-none focus:border-paper-accent focus:ring-1 focus:ring-paper-accent/30 transition-colors duration-200",
                aria_haspopup: "listbox",
                aria_expanded: "{open()}",
                aria_label: "按 target 筛选",
                onclick: move |_| {
                    if !open() {
                        #[cfg(target_arch = "wasm32")]
                        flip_up.set(crate::components::forms::measure_flip(&trigger_id_click, total_options));
                        open.set(true);
                    } else {
                        open.set(false);
                    }
                },
                onkeydown: move |e: KeyboardEvent| {
                    let key = e.key();
                    let is_space = matches!(&key, Key::Character(s) if s == " ");
                    if !open() {
                        if key == Key::ArrowDown || key == Key::ArrowUp || key == Key::Enter || is_space {
                            e.prevent_default();
                            #[cfg(target_arch = "wasm32")]
                            flip_up.set(crate::components::forms::measure_flip(&trigger_id_keys, total_options));
                            open.set(true);
                        }
                    } else if key == Key::Escape || key == Key::Tab {
                        open.set(false);
                    }
                },
                span { class: "truncate", "{current_label}" }
                svg {
                    class: "pointer-events-none absolute right-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-paper-secondary transition-transform duration-200 {chevron_rotate}",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M6 9l6 6 6-6",
                    }
                }
            }

            if open() {
                // 透明遮罩：拦截外部点击关闭
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |_| open.set(false),
                }
                ul {
                    class: "absolute left-0 z-50 w-72 max-w-[calc(100vw_-_2rem)] max-h-60 overflow-y-auto rounded-2xl border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] p-1.5 shadow-lg animate-popover-enter-edge {placement_cls} flex flex-col gap-0.5",
                    role: "listbox",
                    aria_labelledby: "{trigger_id}",
                    // 「全部」选项（清除 target 筛选）
                    {
                        let selected = value.is_none();
                        rsx! {
                            li {
                                button {
                                    r#type: "button",
                                    class: if selected { "w-full flex items-center justify-between gap-2 px-3 py-2 rounded-xl cursor-pointer select-none transition-colors hover:bg-[var(--color-paper-accent-soft)] text-paper-accent text-sm" } else { "w-full flex items-center justify-between gap-2 px-3 py-2 rounded-xl cursor-pointer select-none transition-colors hover:bg-[var(--color-paper-accent-soft)] text-[var(--color-paper-primary)] text-sm" },
                                    role: "option",
                                    aria_selected: "{selected}",
                                    onmousedown: move |e| e.prevent_default(),
                                    onclick: move |_| {
                                        onchange.call(None);
                                        open.set(false);
                                    },
                                    span { class: "truncate", "全部 target" }
                                    if selected {
                                        svg {
                                            class: "w-4 h-4 flex-shrink-0 text-paper-accent",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2.5",
                                            path {
                                                stroke_linecap: "round",
                                                stroke_linejoin: "round",
                                                d: "M5 13l4 4L19 7",
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if options.is_empty() {
                        li { class: "px-3 py-2 text-xs text-[var(--color-paper-tertiary)] select-none",
                            "暂无 target（日志落库后出现）"
                        }
                    }
                    for t in options {
                        {
                            let selected = value.as_deref() == Some(t.as_str());
                            let pick = t.clone();
                            rsx! {
                                li { key: "{t}",
                                    button {
                                        r#type: "button",
                                        class: if selected { "w-full flex items-center justify-between gap-2 px-3 py-2 rounded-xl cursor-pointer select-none transition-colors hover:bg-[var(--color-paper-accent-soft)] font-mono text-xs text-paper-accent" } else { "w-full flex items-center justify-between gap-2 px-3 py-2 rounded-xl cursor-pointer select-none transition-colors hover:bg-[var(--color-paper-accent-soft)] font-mono text-xs text-[var(--color-paper-primary)]" },
                                        role: "option",
                                        aria_selected: "{selected}",
                                        onmousedown: move |e| e.prevent_default(),
                                        onclick: move |_| {
                                            onchange.call(Some(pick.clone()));
                                            open.set(false);
                                        },
                                        span { class: "truncate", "{t}" }
                                        if selected {
                                            svg {
                                                class: "w-4 h-4 flex-shrink-0 text-paper-accent",
                                                view_box: "0 0 24 24",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2.5",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M5 13l4 4L19 7",
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
        }
    }
}

/// 日志行：entry（历史 id>0 / 实时 id=0）或 gap 分隔提示。
///
/// `key` 是客户端单调序号（SSE 实时行 id 恒 0，不能用作渲染 key）。
#[derive(Clone)]
struct LogRow {
    key: u64,
    kind: LogRowKind,
}

#[derive(Clone)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
enum LogRowKind {
    Entry(LogEntry),
    /// broadcast lagged 分隔提示；Some(n) 为服务端报告的丢弃条数。
    Gap(Option<u64>),
}

/// 实时日志流面板：历史加载 + SSE 追加 + 向上翻页 + 滚动跟随。
///
/// 以 `filter_key` remount 重建（props 均为挂载时刻快照）；`follow` /
/// `connected` / `dropped` 是与父组件共享的响应式信号。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn LogsStreamPane(
    levels: Vec<String>,
    target: Option<String>,
    query: String,
    follow: Signal<bool>,
    connected: Signal<bool>,
    dropped: Signal<u64>,
    on_pick_target: Callback<String>,
) -> Element {
    let mut rows: Signal<Vec<LogRow>> = use_signal(Vec::new);
    let mut next_cursor: Signal<Option<i64>> = use_signal(|| None);
    let mut initial_loaded: Signal<bool> = use_signal(|| false);
    let mut load_error: Signal<bool> = use_signal(|| false);
    let mut stream_error: Signal<bool> = use_signal(|| false);
    let mut loading_older: Signal<bool> = use_signal(|| false);
    // 贴底状态：用户上滑离开底部即暂停跟随，浮出「回到底部」。
    let mut stick: Signal<bool> = use_signal(|| true);
    // prepend 后待补偿的滚动基准（prepend 前的 scrollHeight）。
    let mut prepend_base: Signal<Option<i32>> = use_signal(|| None);

    let scroll_id = use_hook(|| {
        format!(
            "logs-stream-{}",
            LOG_STREAM_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        )
    });

    // —— WASM-only 生命周期：挂载拉历史 + 连 SSE；卸载 close EventSource ——
    #[cfg(target_arch = "wasm32")]
    let es_slot: std::rc::Rc<std::cell::RefCell<Option<web_sys::EventSource>>> =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(None)));
    #[cfg(target_arch = "wasm32")]
    let row_key: std::rc::Rc<std::cell::Cell<u64>> =
        use_hook(|| std::rc::Rc::new(std::cell::Cell::new(1)));

    // 挂载 effect 是 FnMut：spawn 内用的非 Copy 值必须在闭包体内 clone，
    // 否则把捕获变量 move 进 async 块会让 effect 闭包退化为 FnOnce。
    let levels_for_load = levels.clone();
    let target_for_load = target.clone();
    let query_for_load = query.clone();
    // move 闭包按值捕获，es_slot/row_key 先克隆出 effect 专用副本，
    // 保留原件供下方 use_drop 使用。
    #[cfg(target_arch = "wasm32")]
    let es_slot_for_effect = es_slot.clone();
    #[cfg(target_arch = "wasm32")]
    let row_key_for_effect = row_key.clone();
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        {
            let levels = levels_for_load.clone();
            let target = target_for_load.clone();
            let query = query_for_load.clone();
            let es_slot = es_slot_for_effect.clone();
            let row_key = row_key_for_effect.clone();
            spawn(async move {
                // remount 期间状态点回落为「未连接」，待 SSE open 后再置绿。
                connected.set(false);
                let q = if query.is_empty() {
                    None
                } else {
                    Some(query.clone())
                };
                match get_logs(levels.clone(), target.clone(), q, None, INITIAL_LIMIT).await {
                    Ok(page) => {
                        dropped.set(page.dropped);
                        next_cursor.set(page.next_cursor);
                        let mut entries = page.entries;
                        entries.sort_by_key(|e| e.id);
                        rows.set(
                            entries
                                .into_iter()
                                .map(|e| LogRow {
                                    key: next_row_key(&row_key),
                                    kind: LogRowKind::Entry(e),
                                })
                                .collect(),
                        );
                    }
                    Err(_) => load_error.set(true),
                }
                initial_loaded.set(true);
                // 历史落地后连 SSE 实时流；失败仅提示，不影响历史查看。
                if open_log_stream(&levels, &target, &query, es_slot, rows, row_key, connected)
                    .is_err()
                {
                    stream_error.set(true);
                }
            });
        }
    });

    #[cfg(target_arch = "wasm32")]
    {
        let es_slot_for_drop = es_slot.clone();
        use_drop(move || {
            if let Some(es) = es_slot_for_drop.borrow_mut().take() {
                es.close();
            }
        });
    }

    // 滚动跟随 / prepend 位移补偿：渲染后执行的真副作用（DOM 写入）。
    // 订阅 rows / follow / stick / prepend_base——开启「自动跟随」会立即贴底。
    let scroll_id_for_effect = scroll_id.clone();
    use_effect(move || {
        let _row_count = rows.read().len();
        let follow_now = follow();
        let stick_now = stick();
        let pending = prepend_base();
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(el) = get_scroll_el(&scroll_id_for_effect) {
                if let Some(base) = pending {
                    // 向上 prepend 后补偿位移，保持视口锚定原内容。
                    let delta = el.scroll_height() - base;
                    if delta > 0 {
                        el.set_scroll_top(el.scroll_top() + delta);
                    }
                    prepend_base.set(None);
                } else if follow_now && stick_now {
                    el.set_scroll_top(el.scroll_height());
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (follow_now, stick_now, pending);
    });

    #[cfg(target_arch = "wasm32")]
    let scroll_id_for_scroll = scroll_id.clone();
    #[cfg(target_arch = "wasm32")]
    let scroll_id_for_older = scroll_id.clone();
    #[cfg(target_arch = "wasm32")]
    let scroll_id_for_back = scroll_id.clone();

    // 「加载更早」onclick 是 FnMut：spawn 内用的非 Copy 值在闭包体内 clone。
    #[cfg(target_arch = "wasm32")]
    let levels_for_older = levels.clone();
    #[cfg(target_arch = "wasm32")]
    let target_for_older = target.clone();
    #[cfg(target_arch = "wasm32")]
    let query_for_older = query.clone();

    let rows_snapshot = rows();

    rsx! {
        div { class: "relative flex-1 min-h-0 flex flex-col bg-[var(--color-paper-entry)]/40 rounded-2xl border border-[var(--color-paper-border)]/70 shadow-xs overflow-hidden",
            // SSE 建连失败提示条（历史仍可用）
            if stream_error() {
                div { class: "flex-shrink-0 px-4 py-2 text-xs text-amber-600 dark:text-amber-400 border-b border-[var(--color-paper-border)]/40 bg-amber-500/5",
                    "实时流连接失败，仅显示历史快照"
                }
            }

            // 日志滚动区（等宽字体行列表，新行自底部进入）
            div {
                id: "{scroll_id}",
                class: "flex-1 min-h-0 overflow-y-auto overscroll-contain py-1 font-mono text-xs leading-5",
                onscroll: move |_| {
                    #[cfg(target_arch = "wasm32")]
                    {
                        if let Some(el) = get_scroll_el(&scroll_id_for_scroll) {
                            let near_bottom = el.scroll_height()
                                - el.scroll_top()
                                - el.client_height()
                                < STICK_THRESHOLD_PX;
                            if near_bottom != *stick.peek() {
                                stick.set(near_bottom);
                            }
                        }
                    }
                },

                // 向上翻页：加载更早（游标来自服务端 LogsPage.next_cursor）
                if next_cursor().is_some() {
                    div { class: "flex justify-center py-1.5 border-b border-[var(--color-paper-border)]/30 mb-1",
                        button {
                            r#type: "button",
                            class: "{BTN_GHOST} font-sans",
                            disabled: loading_older(),
                            onclick: move |_| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    let Some(cursor) = *next_cursor.peek() else { return };
                                    if *loading_older.peek() {
                                        return;
                                    }
                                    loading_older.set(true);
                                    let base = get_scroll_el(&scroll_id_for_older)
                                        .map(|el| el.scroll_height())
                                        .unwrap_or(0);
                                    let levels = levels_for_older.clone();
                                    let target = target_for_older.clone();
                                    let q = if query_for_older.is_empty() {
                                        None
                                    } else {
                                        Some(query_for_older.clone())
                                    };
                                    let row_key = row_key.clone();
                                    spawn(async move {
                                        if let Ok(page) = get_logs(
                                            levels, target, q, Some(cursor), OLDER_PAGE_LIMIT,
                                        )
                                        .await
                                        {
                                            next_cursor.set(page.next_cursor);
                                            let mut older = page.entries;
                                            older.sort_by_key(|e| e.id);
                                            // prepend 重叠去重：仅对历史行（id != 0）。
                                            older.retain(|e| {
                                                e.id == 0
                                                    || !rows.read().iter().any(|r| {
                                                        matches!(&r.kind, LogRowKind::Entry(cur) if cur.id == e.id && e.id != 0)
                                                    })
                                            });
                                            if !older.is_empty() {
                                                let mut merged: Vec<LogRow> = older
                                                    .into_iter()
                                                    .map(|e| LogRow {
                                                        key: next_row_key(&row_key),
                                                        kind: LogRowKind::Entry(e),
                                                    })
                                                    .collect();
                                                merged.extend(rows.read().iter().cloned());
                                                rows.set(merged);
                                                prepend_base.set(Some(base));
                                            }
                                        }
                                        loading_older.set(false);
                                    });
                                }
                            },
                            if loading_older() { "加载中…" } else { "加载更早" }
                        }
                    }
                }

                // 首屏加载 / 错误 / 空态
                if !initial_loaded() {
                    DelayedSkeleton {
                        div { class: "px-3 py-2 space-y-2.5",
                            SkeletonBox { class: "h-4 w-[88%] rounded" }
                            SkeletonBox { class: "h-4 w-[72%] rounded" }
                            SkeletonBox { class: "h-4 w-[81%] rounded" }
                            SkeletonBox { class: "h-4 w-[64%] rounded" }
                            SkeletonBox { class: "h-4 w-[77%] rounded" }
                        }
                    }
                } else if load_error() && rows_snapshot.is_empty() {
                    div { class: "py-16 text-center text-sm font-sans text-red-500 dark:text-red-400",
                        "历史日志加载失败，请调整筛选后重试"
                    }
                } else if rows_snapshot.is_empty() {
                    div { class: "py-16 text-center text-sm font-sans text-[var(--color-paper-tertiary)]",
                        "当前筛选条件下暂无日志"
                    }
                }

                for row in rows_snapshot {
                    match row {
                        LogRow { key, kind: LogRowKind::Gap(missed) } => {
                            let gap_text = match missed {
                                Some(n) => format!("…部分日志已跳过（{n} 条）…"),
                                None => "…部分日志已跳过…".to_string(),
                            };
                            rsx! {
                                div { key: "row-{key}", class: "flex items-center gap-3 px-3 py-1 select-none",
                                    div { class: "flex-1 border-t border-dashed border-[var(--color-paper-border)]" }
                                    span { class: "text-[11px] font-sans text-[var(--color-paper-tertiary)]",
                                        "{gap_text}"
                                    }
                                    div { class: "flex-1 border-t border-dashed border-[var(--color-paper-border)]" }
                                }
                            }
                        }
                        LogRow { key, kind: LogRowKind::Entry(entry) } => {
                            let badge = level_badge_class(&entry.level);
                            let time_hms = entry.ts.format("%H:%M:%S").to_string();
                            let time_full = entry.ts.to_rfc3339();
                            let target_name = entry.target.clone();
                            rsx! {
                                div { key: "row-{key}", class: "flex items-baseline gap-3 px-3 py-1 hover:bg-[var(--color-paper-entry)]/70 transition-colors",
                                    // 时间：HH:MM:SS，hover title 显示完整时间戳
                                    span {
                                        class: "flex-shrink-0 tabular-nums text-[var(--color-paper-tertiary)]",
                                        title: "{time_full}",
                                        "{time_hms}"
                                    }
                                    // 级别 badge（Catppuccin 语义色）
                                    span { class: "{BADGE_BASE} {badge} w-16 justify-center flex-shrink-0",
                                        "{entry.level}"
                                    }
                                    // target：弱化色，点击设为 target 筛选
                                    button {
                                        r#type: "button",
                                        class: "flex-shrink-0 max-w-40 truncate text-[var(--color-paper-tertiary)] hover:text-[var(--color-paper-accent)] transition-colors cursor-pointer text-left",
                                        title: "按此 target 筛选",
                                        onclick: move |_| on_pick_target.call(target_name.clone()),
                                        "{entry.target}"
                                    }
                                    // message：pre-wrap 全量展示（boring 方案，无展开交互）
                                    span { class: "flex-1 min-w-0 whitespace-pre-wrap break-words text-[var(--color-paper-primary)]/90",
                                        "{entry.message}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 「回到底部」浮动按钮（离开底部时浮出）
            if !stick() {
                button {
                    r#type: "button",
                    class: "absolute bottom-3 right-3 z-10 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium bg-[var(--color-paper-entry)] text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] border border-[var(--color-paper-border)] shadow-md transition-all cursor-pointer",
                    onclick: move |_| {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(el) = get_scroll_el(&scroll_id_for_back) {
                            el.set_scroll_top(el.scroll_height());
                        }
                        stick.set(true);
                    },
                    "回到底部"
                }
            }
        }
    }
}

/// 级别 badge 配色（Catppuccin 语义色，沿用 StatusBadge 色板惯例）。
fn level_badge_class(level: &str) -> &'static str {
    match level {
        "ERROR" => "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300",
        "WARN" => "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
        "INFO" => "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300",
        "DEBUG" => "bg-[var(--color-paper-theme)] text-[var(--color-paper-secondary)]",
        // TRACE 最弱化：无底色，仅三级文字色。
        _ => "text-[var(--color-paper-tertiary)]",
    }
}

/// 分配客户端单调行 key（SSE 实时行 id 恒 0，不能用作渲染 key）。
#[cfg(target_arch = "wasm32")]
fn next_row_key(counter: &std::cell::Cell<u64>) -> u64 {
    let v = counter.get();
    counter.set(v + 1);
    v
}

/// 按 id 取日志滚动容器元素。
#[cfg(target_arch = "wasm32")]
fn get_scroll_el(id: &str) -> Option<web_sys::Element> {
    web_sys::window()?.document()?.get_element_by_id(id)
}

/// 建立 SSE 连接消费实时日志流。
///
/// `log` 事件（JSON LogEntry，id 恒 0）追加到列表尾部；`gap` 事件
/// （broadcast lagged）插入分隔提示行；`open`/`error` 维护连接状态点。
/// 浏览器在 `error` 后自动重连（重连丢段由服务端 `gap` 通知），
/// 故 error 只标记断开、不 close。EventSource 存入 `es_slot`，
/// 由组件 `use_drop` 统一 close。
#[cfg(target_arch = "wasm32")]
fn open_log_stream(
    levels: &[String],
    target: &Option<String>,
    query: &str,
    es_slot: std::rc::Rc<std::cell::RefCell<Option<web_sys::EventSource>>>,
    rows: Signal<Vec<LogRow>>,
    row_key: std::rc::Rc<std::cell::Cell<u64>>,
    connected: Signal<bool>,
) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    use web_sys::{EventSource, MessageEvent};

    let mut url = format!("/api/logs/stream?levels={}", levels.join(","));
    if let Some(t) = target {
        if !t.is_empty() {
            url.push_str("&target=");
            url.push_str(&String::from(js_sys::encode_uri_component(t)));
        }
    }
    if !query.is_empty() {
        url.push_str("&q=");
        url.push_str(&String::from(js_sys::encode_uri_component(query)));
    }
    let es = EventSource::new(&url)?;

    // open → 已连接
    let mut connected_for_open = connected;
    let on_open = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        connected_for_open.set(true);
    });
    es.add_event_listener_with_callback("open", on_open.as_ref().unchecked_ref())?;
    on_open.forget();

    // log 事件 → 追加（实时行 id 恒 0：到达即追加，不去重、不作游标）
    let mut rows_for_log = rows;
    let row_key_for_log = row_key.clone();
    let on_log = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        if let Some(s) = e.data().as_string() {
            if let Ok(entry) = serde_json::from_str::<LogEntry>(&s) {
                rows_for_log.with_mut(|list| {
                    list.push(LogRow {
                        key: next_row_key(&row_key_for_log),
                        kind: LogRowKind::Entry(entry),
                    });
                    // 缓冲上限：超出后裁掉最旧一段，保护 DOM 规模。
                    if list.len() > MAX_BUFFER_ROWS {
                        list.drain(..BUFFER_TRIM);
                    }
                });
            }
        }
    });
    es.add_event_listener_with_callback("log", on_log.as_ref().unchecked_ref())?;
    on_log.forget();

    // gap 事件 → 分隔提示行（broadcast lagged；data 为文本 "missed N log events"）
    let mut rows_for_gap = rows;
    let row_key_for_gap = row_key.clone();
    let on_gap = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        // data 形如 "missed N log events"，解析出 N 用于提示；解析失败则省略计数。
        let missed = e
            .data()
            .as_string()
            .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok());
        rows_for_gap.with_mut(|list| {
            list.push(LogRow {
                key: next_row_key(&row_key_for_gap),
                kind: LogRowKind::Gap(missed),
            });
        });
    });
    es.add_event_listener_with_callback("gap", on_gap.as_ref().unchecked_ref())?;
    on_gap.forget();

    // error → 标记断开（浏览器自动重连，open 时恢复）
    let mut connected_for_error = connected;
    let on_error = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        connected_for_error.set(false);
    });
    es.add_event_listener_with_callback("error", on_error.as_ref().unchecked_ref())?;
    on_error.forget();

    *es_slot.borrow_mut() = Some(es);
    Ok(())
}

/// 将文本内容触发为浏览器下载（Blob + ObjectURL + 临时 `<a download>`）。
#[cfg(target_arch = "wasm32")]
fn download_text_file(filename: &str, content: &str) {
    use wasm_bindgen::JsCast;

    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(content));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("text/plain;charset=utf-8");
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    if let Ok(el) = document.create_element("a") {
        if let Ok(anchor) = el.dyn_into::<web_sys::HtmlAnchorElement>() {
            anchor.set_href(&url);
            anchor.set_download(filename);
            anchor.click();
        }
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

/// 导出文件名：`yggdrasil-logs-<YYYY-MM-DD>.log`（浏览器本地日期）。
#[cfg(target_arch = "wasm32")]
fn log_export_filename() -> String {
    let iso: String = js_sys::Date::new_0().to_iso_string().into();
    let date = iso.get(..10).unwrap_or("unknown-date");
    format!("yggdrasil-logs-{date}.log")
}
