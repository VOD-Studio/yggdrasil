//! 管理后台「运行日志」页面（`/admin/logs`）。
//!
//! 数据流：筛选变更（级别开关 / target 下拉 / 关键字）→ 变更 `filter_key` →
//! `LogsStreamPane` 整体重建：卸载即 `on_cleanup` close EventSource，挂载即
//! 重新拉历史（`get_logs` limit 200）并重连 SSE `/api/logs/stream`。SSE 实时行追加到
//! 列表尾部并按 id 去重——实时事件 id 恒为 0（未落库），去重只对历史行生效。
//!
//! 页面在主内容区内部滚动：标题行与筛选栏固定，日志区占满剩余高度。导出按当前筛选
//! 下载 .log 文件；清理策略在「⋯」展开的面板里。样式见 styles/pages/admin-logs.css。
//!
//! 页面没有 SSR 数据：历史与实时流都在浏览器端加载，加载前的骨架屏由后台外壳按路径
//! 提供（`LogsSkeleton`）。
use crate::components::admin_layout::AdminPageHead;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{lazy_route, LazyRoute};

use crate::components::forms::{FormSelect, ToggleSwitch, FORM_SELECT_COMPACT_CLASS};
use crate::components::ui::{button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconName};

mod cleanup;
#[cfg(target_arch = "wasm32")]
mod export;
mod stream;
use cleanup::CleanupPolicy;
#[cfg(target_arch = "wasm32")]
use export::{download_text_file, log_export_filename};
use stream::LogsStreamPane;

/// 全部可选级别（顺序即展示与服务端白名单顺序）。
const ALL_LEVELS: [&str; 5] = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

/// `/admin/logs` 路由：无 SSR 数据，直接渲染页面。
pub struct LogsRoute;

#[lazy_route]
impl LazyRoute for LogsRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <Logs/> }.into_any()
    }
}

/// 运行日志页面。
#[component]
pub fn Logs() -> impl IntoView {
    let toast = use_toast();
    // —— 已应用筛选（任何变化都会重建下方 LogsStreamPane）——
    let levels = RwSignal::new(vec!["ERROR", "WARN", "INFO"]);
    let target = RwSignal::new(None::<String>);
    let query = RwSignal::new(String::new());
    // 关键字草稿：Enter 提交、清空即时生效，避免每击键都重连 SSE。
    let query_draft = RwSignal::new(String::new());
    let follow = RwSignal::new(true);
    let connected = RwSignal::new(false);
    let dropped = RwSignal::new(0_u64);
    let exporting = RwSignal::new(false);
    let cleanup_open = RwSignal::new(false);
    // target 候选：挂载后拉取一次（服务端缓存 60s）。
    let targets = RwSignal::new(Vec::<String>::new());
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(list) = crate::api::logs::get_log_targets().await {
                targets.try_set(list);
            }
        });
    });

    // 行内 target 点击 → 设为 target 筛选（触发重建）。
    let on_pick_target = Callback::new(move |t: String| target.set(Some(t)));
    // FormSelect 的类型参数靠闭包参数类型推断；闭包放在 view! 外，避免宏解析泛型尖括号。
    let on_target_change = move |v: Option<String>| target.set(v);
    // 只有筛选内容真的变化时 Memo 才通知，面板因此不会被无关更新重建。
    let filter_key = Memo::new(move |_| {
        format!(
            "{}|{}|{}",
            levels.with(|l| l.join(",")),
            target.get().unwrap_or_default(),
            query.get()
        )
    });
    let target_options = Signal::derive(move || {
        let mut options: Vec<(Option<String>, String)> = vec![(None, "全部 target".into())];
        options.extend(targets.get().into_iter().map(|t| (Some(t.clone()), t)));
        options
    });
    let export = move |_| {
        if exporting.get_untracked() {
            return;
        }
        exporting.set(true);
        let lv: Vec<String> = levels.with_untracked(|l| l.iter().map(|s| s.to_string()).collect());
        let (t, q) = (target.get_untracked(), query.get_untracked());
        spawn_local(async move {
            match crate::api::logs::export_logs(lv, t, (!q.is_empty()).then_some(q)).await {
                Ok(content) => {
                    let count = content.lines().count();
                    #[cfg(target_arch = "wasm32")]
                    download_text_file(&log_export_filename(), &content);
                    toast.success(format!("已导出 {count} 条日志"));
                }
                Err(_) => toast.error("日志导出失败，请重试"),
            }
            exporting.try_set(false);
        });
    };

    view! {
        <div class="logs-page">
            <AdminPageHead
                title="运行日志"
                class="logs-head"
                actions=move || {
                    view! {
                        <span
                            class="logs-live"
                            data-on=move || connected.get().then_some("true")
                            role="status"
                        >
                            <span class="logs-live-dot" aria-hidden="true"></span>
                            {move || if connected.get() { "已连接" } else { "未连接" }}
                        </span>
                        <button
                            class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                            type="button"
                            aria-expanded=move || cleanup_open.get().to_string()
                            on:click=move |_| cleanup_open.update(|open| *open = !*open)
                        >
                            <Icon name=IconName::SlidersHorizontal size=15/>
                            "清理策略"
                        </button>
                        <button
                            class=button_class(ButtonVariant::Secondary, ButtonSize::Md)
                            type="button"
                            disabled=move || exporting.get()
                            on:click=export
                        >
                            <Icon name=IconName::Download size=15/>
                            {move || if exporting.get() { "导出中…" } else { "导出日志" }}
                        </button>
                    }
                }
            >
                "进程内结构化日志，历史查询加实时流。"
                {move || {
                    (dropped.get() > 0)
                        .then(|| format!("启动以来丢弃 {} 条。", dropped.get()))
                }}
            </AdminPageHead>
            {move || {
                cleanup_open
                    .get()
                    .then(|| {
                        view! { <CleanupPolicy on_close=move |_| cleanup_open.set(false)/> }
                    })
            }}
            <div class="logs-filters">
                <div class="logs-levels" role="group" aria-label="日志级别">
                    {ALL_LEVELS
                        .into_iter()
                        .map(|level| {
                            view! {
                                <button
                                    type="button"
                                    class="logs-level-toggle"
                                    data-level=level
                                    aria-pressed=move || {
                                        levels.with(|sel| sel.contains(&level)).to_string()
                                    }
                                    on:click=move |_| {
                                        levels
                                            .update(|sel| {
                                                if let Some(pos) = sel.iter().position(|l| *l == level) {
                                                    sel.remove(pos);
                                                } else {
                                                    sel.push(level);
                                                    sel.sort_by_key(|l| {
                                                        ALL_LEVELS
                                                            .iter()
                                                            .position(|a| a == l)
                                                            .unwrap_or(usize::MAX)
                                                    });
                                                }
                                            });
                                    }
                                >
                                    {level}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
                <FormSelect
                    id="logs-target"
                    value=target
                    options=target_options
                    trigger_class=FORM_SELECT_COMPACT_CLASS
                    aria_label="按 target 筛选"
                    onchange=on_target_change
                />
                <label class="field field-sm admin-filter-search logs-search">
                    <Icon name=IconName::Search size=14/>
                    <span class="sr-only">"搜索日志"</span>
                    <input
                        type="search"
                        placeholder="搜索日志内容，Enter 生效"
                        prop:value=move || query_draft.get()
                        on:input=move |e| {
                            let v = event_target_value(&e);
                            // 清空即恢复无关键字。
                            if v.is_empty() && !query.get_untracked().is_empty() {
                                query.set(String::new());
                            }
                            query_draft.set(v);
                        }
                        on:keydown=move |e: leptos::ev::KeyboardEvent| {
                            if e.key() == "Enter" && !e.is_composing() {
                                query.set(query_draft.get_untracked());
                            }
                        }
                    />
                </label>
                <label class="logs-follow">
                    <ToggleSwitch
                        checked=follow
                        aria_label="自动跟随"
                        ontoggle=move |_| follow.update(|on| *on = !*on)
                    />
                    "自动跟随"
                </label>
            </div>
            {move || {
                levels
                    .with(Vec::is_empty)
                    .then(|| view! { <p class="logs-hint">"没有选择级别时显示全部。"</p> })
            }}
            // 筛选变更 → 重建：卸载旧面板（close EventSource）→ 挂载新面板。
            // 只订阅 filter_key，面板参数一律按挂载时刻的快照读取。
            {move || {
                let _ = filter_key.get();
                let levels: Vec<String> = levels
                    .with_untracked(|l| l.iter().map(|s| s.to_string()).collect());
                let target = target.get_untracked();
                let query = query.get_untracked();
                view! {
                    <div class="logs-pane-slot">
                        <LogsStreamPane
                            levels
                            target
                            query
                            follow
                            connected
                            dropped
                            on_pick_target
                        />
                    </div>
                }
            }}
        </div>
    }
}
