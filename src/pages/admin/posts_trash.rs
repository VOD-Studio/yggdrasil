//! 回收站页面（`/admin/posts/trash` 独立路由，属侧边栏「内容管理」子菜单）。
//!
//! 展示已软删除文章，支持恢复、彻底删除、批量操作、一键清空，
//! 以及自动清理配置（启用开关 + 保留天数）。
//! 数据加载与操作仅在 WASM 前端通过 Dioxus server functions 交互。
//!
#[allow(unused_imports)]
use std::collections::HashSet;

use crate::router::Route;
use dioxus::prelude::*;
use dioxus::router::components::Link;
// 操作类 server function 在 SSR 与 WASM 均需可见（spawn 闭包需类型检查），
// 但部分仅用于 WASM 代码路径，SSR 下触发 unused imports，按项目惯例放行。
#[allow(unused_imports)]
use crate::api::posts::{
    batch_purge_posts, batch_restore_posts, empty_trash, list_deleted_posts, purge_post,
    restore_post, PostListResponse,
};
#[allow(unused_imports)]
use crate::api::settings::{get_trash_settings, update_trash_settings};
use crate::components::empty_state::EmptyState;
use crate::components::forms::ToggleSwitch;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::posts_trash_skeleton::PostsTrashTableSkeleton;
use crate::components::ui::{
    Checkbox, CollapsibleSettingsCard, LoadingButton, Pagination, Popover, BTN_DANGER_OUTLINE,
    BTN_GHOST, BTN_ICON, BTN_SOLID_GREEN, BTN_SOLID_RED,
};
use crate::hooks::query::use_paginated;
use crate::models::post::PostListItem;
use crate::models::settings::TrashSettings;
/// 每页展示的回收站文章数量。
const TRASH_PER_PAGE: i32 = 20;

/// 回收站页面：列表 + 批量操作 + 自动清理配置。
///
/// 独立路由 `/admin/posts/trash` 的页面组件（原 `posts.rs::Posts` 的回收站 tab
/// 提升而来）。翻页用客户端 signal 驱动（`current_page` signal + `use_paginated`
/// 闭包内读取建立依赖），不走路由参数。支持单条/批量恢复与彻底删除、一键清空，
/// 以及内联自动清理配置。header 副标题的删除计数取自 `use_paginated` 的 `total`。
#[allow(unused_mut, unused_variables)]
#[component]
pub fn PostsTrash() -> Element {
    let current_page = use_signal(|| 1);
    let mut selected_ids: Signal<HashSet<i32>> = use_signal(HashSet::new);

    // 分页列表加载（loading / posts / total / error）由 use_paginated 统一管理。
    // 闭包内读取 current_page（.with）建立 reactive 依赖，翻页时自动重新请求。
    let paginated = use_paginated(
        move || current_page.with(|p| *p),
        TRASH_PER_PAGE,
        |p, pp| async move {
            list_deleted_posts(p, pp)
                .await
                .map(|PostListResponse { posts, total }| (posts, total))
        },
    );
    let mut posts = paginated.items;
    let mut total = paginated.total;
    let loading = paginated.loading;
    let mut error = paginated.error;

    // 自动清理配置：由子组件 AutoPurgeSettings 写入（加载/保存），本组件读取
    // retention_days 供 TrashRow 的「剩余天数」展示。
    let mut settings: Signal<TrashSettings> = use_signal(TrashSettings::default);

    // 本地移除一篇文章（乐观更新）。
    let mut remove_post = move |id: i32| {
        posts.with_mut(|list| list.retain(|p| p.id != id));
        total.with_mut(|t| *t = t.saturating_sub(1));
        selected_ids.with_mut(|s| {
            s.remove(&id);
        });
    };

    // 首次加载完成前不显示数量，避免「(0)」闪烁；翻页重载时 total 仍为上页值，计数保留。
    let subtitle = if total() > 0 || !loading() {
        format!("已删除文章 ({})", total())
    } else {
        "已删除文章".to_string()
    };

    rsx! {
        div { class: "animate-page-enter w-full max-w-7xl mx-auto space-y-6",
            // 页面页头：标题 + 副标题 + 返回列表与清空回收站入口
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70",
                div {
                    h1 { class: "text-3xl sm:text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                        "回收站"
                    }
                    p { class: "text-sm text-[var(--color-paper-secondary)] mt-1.5",
                        "{subtitle} · 可随时恢复或彻底删除"
                    }
                }
                div { class: "flex items-center gap-3",
                    Link {
                        class: "inline-flex items-center gap-1.5 px-4 py-2 rounded-full text-sm font-medium text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                        to: Route::Posts {},
                        svg {
                            class: "w-4 h-4",
                            xmlns: "http://www.w3.org/2000/svg",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M19 12H5M12 19l-7-7 7-7" }
                        }
                        "全部文章"
                    }
                    if total() > 0 {
                        button {
                            class: "{BTN_DANGER_OUTLINE} inline-flex items-center gap-1.5",
                            onclick: move |_| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    if web_sys::window()
                                        .and_then(|w| {
                                            w.confirm_with_message(
                                                    "确定要清空回收站吗？所有已删除文章将被彻底移除，此操作不可恢复。",
                                                )
                                                .ok()
                                        })
                                        .unwrap_or(false)
                                    {
                                        spawn(async move {
                                            let _ = empty_trash().await;
                                        });
                                        posts.set(Vec::new());
                                        total.set(0);
                                        selected_ids.set(HashSet::new());
                                    }
                                }
                            },
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                polyline { points: "3 6 5 6 21 6" }
                                path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }
                            }
                            "清空回收站"
                        }
                    }
                }
            }

            div { class: "space-y-6",
                // 自动清理配置卡片
                AutoPurgeSettings { settings }

                // 批量操作栏（浮动卡片风格）
                if !selected_ids().is_empty() {
                    div { class: "animate-row-enter flex flex-wrap items-center justify-between gap-3 p-3.5 bg-[var(--color-paper-entry)] rounded-2xl border border-[var(--color-paper-border)] shadow-xs",
                        div { class: "flex items-center gap-2 text-sm font-medium text-[var(--color-paper-primary)]",
                            span { class: "w-2 h-2 rounded-full bg-[var(--color-paper-accent)]" }
                            span { "已选中 {selected_ids().len()} 篇文章" }
                        }
                        div { class: "flex items-center gap-2",
                            button {
                                class: "{BTN_SOLID_GREEN} inline-flex items-center gap-1.5",
                                onclick: move |_| {
                                    let ids: Vec<i32> = selected_ids().iter().copied().collect();
                                    spawn(async move {
                                        let _ = batch_restore_posts(ids).await;
                                    });
                                    for id in selected_ids() {
                                        remove_post(id);
                                    }
                                    selected_ids.set(HashSet::new());
                                },
                                svg {
                                    class: "w-3.5 h-3.5",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    polyline { points: "1 4 1 10 7 10" }
                                    path { d: "M3.51 15a9 9 0 1 0 2.13-9.36L1 10" }
                                }
                                "批量恢复"
                            }
                            button {
                                class: "{BTN_SOLID_RED} inline-flex items-center gap-1.5",
                                onclick: move |_| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        if web_sys::window()
                                            .and_then(|w| {
                                                w.confirm_with_message(
                                                        "确定要彻底删除选中的文章吗？此操作不可恢复。",
                                                    )
                                                    .ok()
                                            })
                                            .unwrap_or(false)
                                        {
                                            let ids: Vec<i32> = selected_ids().iter().copied().collect();
                                            spawn(async move {
                                                let _ = batch_purge_posts(ids).await;
                                            });
                                            for id in selected_ids() {
                                                remove_post(id);
                                            }
                                            selected_ids.set(HashSet::new());
                                        }
                                    }
                                },
                                svg {
                                    class: "w-3.5 h-3.5",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    polyline { points: "3 6 5 6 21 6" }
                                    path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }
                                }
                                "批量彻底删除"
                            }
                            button {
                                class: "{BTN_GHOST}",
                                onclick: move |_| selected_ids.set(HashSet::new()),
                                "取消"
                            }
                        }
                    }
                }
                // 主内容：错误 / 加载骨架 / 空态 / 列表
                {
                    if error().is_some() {
                        rsx! {
                            EmptyState {
                                title: "加载失败",
                                description: "获取回收站列表时发生错误，请稍后重试。",
                            }
                        }
                    } else if loading() && posts().is_empty() {
                        rsx! {
                            DelayedSkeleton {
                                PostsTrashTableSkeleton {}
                            }
                        }
                    } else if posts().is_empty() {
                        rsx! {
                            EmptyState {
                                title: "回收站为空",
                                description: "当前没有被软删除的文章。",
                            }
                        }
                    } else {
                        let list = posts();
                        let all_selected = list.iter().all(|p| selected_ids().contains(&p.id));
                        let all_ids: Vec<i32> = list.iter().map(|p| p.id).collect();
                        rsx! {
                            div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 overflow-hidden",
                                div { class: "overflow-x-auto overflow-y-hidden",
                                    table { class: "w-full text-sm",
                                        thead {
                                            tr { class: "bg-[var(--color-paper-entry)]/80 border-b border-[var(--color-paper-border)]/70 text-left text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)] select-none",
                                                th { class: "px-4 py-3.5 w-10 text-center",
                                                    Checkbox {
                                                        checked: all_selected,
                                                        onchange: move |_checked: bool| {
                                                            let mut s = selected_ids();
                                                            if all_selected {
                                                                for id in &all_ids {
                                                                    s.remove(id);
                                                                }
                                                            } else {
                                                                for id in &all_ids {
                                                                    s.insert(*id);
                                                                }
                                                            }
                                                            selected_ids.set(s);
                                                        },
                                                    }
                                                }
                                                th { class: "px-5 py-3.5 font-semibold", "文章标题" }
                                                th { class: "px-4 py-3.5 font-semibold whitespace-nowrap text-center", "原发布状态" }
                                                th { class: "px-4 py-3.5 font-semibold w-32 whitespace-nowrap", "删除日期" }
                                                th { class: "px-4 py-3.5 font-semibold w-24 text-center whitespace-nowrap",
                                                    "剩余保留"
                                                }
                                                th { class: "px-5 py-3.5 font-semibold w-36 text-right whitespace-nowrap",
                                                    "操作"
                                                }
                                            }
                                        }
                                        tbody {
                                            for (idx, post) in list.iter().enumerate() {
                                                TrashRow {
                                                    key: "{post.id}",
                                                    post: post.clone(),
                                                    retention_days: settings().retention_days,
                                                    selected: selected_ids().contains(&post.id),
                                                    stagger_index: idx as u32,
                                                    on_select: {
                                                        let id = post.id;
                                                        move |checked: bool| {
                                                            let mut s = selected_ids();
                                                            if checked {
                                                                s.insert(id);
                                                            } else {
                                                                s.remove(&id);
                                                            }
                                                            selected_ids.set(s);
                                                        }
                                                    },
                                                    on_restore: {
                                                        let id = post.id;
                                                        move |_| {
                                                            spawn(async move {
                                                                let _ = restore_post(id).await;
                                                            });
                                                            remove_post(id);
                                                        }
                                                    },
                                                    on_purge: {
                                                        let id = post.id;
                                                        move |_| {
                                                            #[cfg(target_arch = "wasm32")]
                                                            spawn(async move {
                                                                let _ = purge_post(id).await;
                                                            });
                                                            remove_post(id);
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Pagination {
                                variant: "admin",
                                current_page: current_page(),
                                total: total(),
                                per_page: TRASH_PER_PAGE,
                                unit: "篇",
                                on_prev: {
                                    let mut page = current_page;
                                    move |_| {
                                        page.with_mut(|p| *p = (*p - 1).max(1));
                                    }
                                },
                                on_next: {
                                    let mut page = current_page;
                                    move |_| {
                                        page.with_mut(|p| *p += 1);
                                    }
                                },
                                on_jump: {
                                    let mut page = current_page;
                                    move |p: i32| {
                                        page.set(p);
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 自动清理配置子组件：使用共享可折叠设置卡片。
///
/// 封装自动清理的全部状态：表单草稿（`settings_draft_*`）、保存态、已保存反馈，
/// 以及派生的 `dirty`。面板折叠态与摘要外壳由 `CollapsibleSettingsCard` 统一管理。
/// 配置加载与保存均在组件内部完成。`settings`（已保存的服务端配置）由父组件传入
/// 双向绑定 signal：本组件加载/保存时写入，父组件读取 `retention_days` 供 TrashRow
/// 的「剩余天数」。
///
/// 从 `PostsTrashPage` 抽取以降低 god component 复杂度（见 dioxus-render-purity skill）。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn AutoPurgeSettings(settings: Signal<TrashSettings>) -> Element {
    let mut settings_draft_days: Signal<String> = use_signal(|| "30".to_string());
    let mut settings_draft_enabled: Signal<bool> = use_signal(|| false);
    let mut saving_settings: Signal<bool> = use_signal(|| false);
    // 保存成功后的短暂反馈标记（用户再次编辑时清除）。
    let mut just_saved: Signal<bool> = use_signal(|| false);

    // 首次渲染加载服务端配置：本组件挂载即触发一次，无需 settings_loaded 守卫
    //（父组件每次翻页重渲染的是列表 effect，本组件 effect 只在自身首次挂载跑）。
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            if let Ok(s) = get_trash_settings().await {
                settings_draft_days.set(s.retention_days.to_string());
                settings_draft_enabled.set(s.auto_purge_enabled);
                settings.set(s);
            }
        });
    });

    // 草稿相对已保存配置是否存在差异：控制保存按钮可用性与“未保存”提示。
    // 派生值用 use_memo：依赖信号不变时不重算（避免每次渲染重复 parse 字符串）。
    let dirty = use_memo(move || {
        settings_draft_enabled() != settings().auto_purge_enabled
            || settings_draft_days()
                .trim()
                .parse::<i32>()
                .ok()
                .map(|d| d != settings().retention_days)
                .unwrap_or(true)
    });

    rsx! {
        CollapsibleSettingsCard {
            title: "自动清理".to_string(),
            summary: if settings().auto_purge_enabled { format!(
                "已开启 · 超过 {} 天的文章将被自动删除",
                settings().retention_days,
            ) } else { "已关闭".to_string() },
            enabled: settings().auto_purge_enabled,
            on_toggle: move |_| just_saved.set(false),
            div { class: "border-t border-paper-border p-5 space-y-6",
                // 开关行：启用自动清理
                div { class: "flex items-center justify-between gap-4",
                    div { class: "min-w-0",
                        div { class: "text-sm font-medium text-paper-primary", "启用自动清理" }
                        div { class: "text-xs text-paper-secondary mt-1",
                            "后台任务定期彻底删除超过保留期的文章"
                        }
                    }
                    ToggleSwitch {
                        checked: settings_draft_enabled(),
                        ontoggle: move |_| {
                            settings_draft_enabled.set(!settings_draft_enabled());
                            just_saved.set(false);
                        },
                    }
                }

                // 保留天数行
                div { class: "space-y-3",
                    div { class: "min-w-0",
                        div { class: "text-sm font-medium text-paper-primary", "保留天数" }
                        div { class: "text-xs text-paper-secondary mt-1",
                            "文章删除后保留的时长，到期后自动彻底清除（1–365）"
                        }
                    }
                    // 数字输入 + 步进按钮 + 单位后缀
                    div { class: "flex items-center gap-3",
                        div { class: "flex items-center rounded-lg border border-paper-border bg-paper-entry overflow-hidden",
                            // 减号
                            button {
                                class: "{BTN_ICON}",
                                r#type: "button",
                                aria_label: "减少保留天数",
                                onclick: move |_| {
                                    let cur: i32 = settings_draft_days().trim().parse().unwrap_or(30);
                                    let next = cur.saturating_sub(1).max(1);
                                    settings_draft_days.set(next.to_string());
                                    just_saved.set(false);
                                },
                                "−"
                            }
                            // 数字输入（无边框，衔接步进按钮）
                            input {
                                r#type: "number",
                                min: "1",
                                max: "365",
                                class: "w-14 h-9 px-1 text-center text-sm tabular-nums text-paper-primary bg-transparent border-0 focus:outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none",
                                value: "{settings_draft_days()}",
                                oninput: move |e| {
                                    settings_draft_days.set(e.value());
                                    just_saved.set(false);
                                },
                            }
                            // 加号
                            button {
                                class: "{BTN_ICON}",
                                r#type: "button",
                                aria_label: "增加保留天数",
                                onclick: move |_| {
                                    let cur: i32 = settings_draft_days().trim().parse().unwrap_or(30);
                                    let next = cur.saturating_add(1).min(365);
                                    settings_draft_days.set(next.to_string());
                                    just_saved.set(false);
                                },
                                "+"
                            }
                        }
                        span { class: "text-xs text-paper-secondary", "天" }
                    }
                }

                // 底部操作行：未保存提示 + 保存按钮
                div { class: "flex items-center justify-between gap-4 pt-1",
                    // 草稿状态提示
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
                    // 保存按钮：主题绿主操作，saving 态显示 spinner，just_saved/无改动禁用
                    LoadingButton {
                        label: "保存设置".to_string(),
                        loading: saving_settings(),
                        disabled: just_saved() || !dirty(),
                        variant: "sm",
                        onclick: move |_| {
                            let days: i32 = settings_draft_days().parse().unwrap_or(30);
                            let enabled = settings_draft_enabled();
                            saving_settings.set(true);
                            spawn(async move {
                                if let Ok(s) = update_trash_settings(enabled, days).await {
                                    settings.set(s);
                                    just_saved.set(true);
                                }
                                saving_settings.set(false);
                            });
                        },
                    }
                }
            }
        }
    }
}

/// 计算剩余天数（保留期 - 已删除天数）。
///
/// 返回 (剩余天数, 是否已过期)。基于客户端时钟计算，轻微漂移可接受。
fn remaining_days(post: &PostListItem, retention_days: i32) -> (i64, bool) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(deleted_at) = post.deleted_at {
            let now_ms = js_sys::Date::now() as i64; // 毫秒
            let deleted_ms = deleted_at.timestamp_millis();
            let elapsed_days = (now_ms - deleted_ms) / 86_400_000;
            let remaining = retention_days as i64 - elapsed_days;
            (remaining, remaining <= 0)
        } else {
            (retention_days as i64, false)
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = post;
        (retention_days as i64, false)
    }
}

/// 回收站表格行组件。
#[component]
fn TrashRow(
    post: PostListItem,
    retention_days: i32,
    selected: bool,
    stagger_index: u32,
    on_select: EventHandler<bool>,
    on_restore: EventHandler,
    on_purge: EventHandler,
) -> Element {
    let (remaining, expired) = remaining_days(&post, retention_days);
    // 彻底删除确认浮层：用触发按钮的视口坐标锚定，避免被表格 overflow 裁剪。
    let mut purge_open = use_signal(|| false);
    let mut anchor_x = use_signal(|| 0i32);
    let mut anchor_y = use_signal(|| 0i32);
    // 剩余天数徽章配色：>7 天中性，≤7 天鼠尾草绿(主题色)，≤0/过期琥珀色。
    let badge_class = if expired {
        "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400"
    } else if remaining <= 7 {
        "bg-paper-accent-soft text-paper-accent"
    } else {
        "bg-paper-tertiary text-paper-secondary"
    };
    let badge_text = if expired {
        "待清理".to_string()
    } else {
        format!("{remaining}天")
    };
    let deleted_str = post
        .deleted_at
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "—".to_string());

    rsx! {
        tr {
            class: "animate-row-enter border-b border-[var(--color-paper-border)]/60 last:border-b-0 hover:bg-[var(--color-paper-accent-soft)]/30 transition-colors duration-150 group",
            style: "animation-delay: {stagger_index * 35}ms",
            td { class: "px-4 py-3.5 text-center",
                Checkbox {
                    checked: selected,
                    onchange: move |checked: bool| on_select.call(checked),
                }
            }
            td { class: "px-5 py-3.5",
                div { class: "flex flex-col gap-1",
                    div { class: "font-semibold text-sm text-[var(--color-paper-primary)] leading-snug line-clamp-1",
                        "{post.title}"
                    }
                    div { class: "flex items-center gap-2 text-xs",
                        span { class: "font-mono text-[11px] text-[var(--color-paper-tertiary)]",
                            "/post/{post.slug}"
                        }
                    }
                }
            }
            td { class: "px-4 py-3.5 text-center whitespace-nowrap",
                if post.status == crate::models::post::PostStatus::Published {
                    span { class: "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20",
                        span { class: "w-1.5 h-1.5 rounded-full bg-emerald-500" }
                        "公开"
                    }
                } else {
                    span { class: "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20",
                        span { class: "w-1.5 h-1.5 rounded-full bg-amber-500" }
                        "草稿"
                    }
                }
            }
            td { class: "px-4 py-3.5 text-xs font-mono text-[var(--color-paper-secondary)] whitespace-nowrap",
                "{deleted_str}"
            }
            td { class: "px-4 py-3.5 text-center whitespace-nowrap",
                span { class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium border {badge_class}",
                    "{badge_text}"
                }
            }
            td { class: "px-5 py-3.5 text-right whitespace-nowrap",
                div { class: "flex justify-end items-center gap-2",
                    button {
                        class: "inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium text-emerald-600 dark:text-emerald-400 hover:bg-emerald-50 dark:hover:bg-emerald-900/20 transition-colors cursor-pointer",
                        onclick: move |_| on_restore.call(()),
                        svg {
                            class: "w-3.5 h-3.5",
                            xmlns: "http://www.w3.org/2000/svg",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            polyline { points: "1 4 1 10 7 10" }
                            path { d: "M3.51 15a9 9 0 1 0 2.13-9.36L1 10" }
                        }
                        "恢复"
                    }
                    button {
                        class: "inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium text-red-500 hover:text-red-700 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors cursor-pointer",
                        onclick: move |e| {
                            let coordinates = e.client_coordinates();
                            anchor_x.set(coordinates.x as i32);
                            anchor_y.set(coordinates.y as i32);
                            purge_open.set(true);
                        },
                        svg {
                            class: "w-3.5 h-3.5",
                            xmlns: "http://www.w3.org/2000/svg",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            polyline { points: "3 6 5 6 21 6" }
                            path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }
                        }
                        "彻底删除"
                    }
                }
            }
            Popover {
                open: purge_open(),
                anchor_x: anchor_x(),
                anchor_y: anchor_y(),
                placement: "bottom",
                align: "end",
                on_close: move |_| purge_open.set(false),
                div { class: "w-64 space-y-3",
                    p { class: "text-sm text-paper-primary leading-relaxed",
                        "彻底删除这篇文章？此操作不可恢复。"
                    }
                    div { class: "flex justify-end gap-2 pt-1",
                        button {
                            class: "{BTN_GHOST}",
                            onclick: move |_| purge_open.set(false),
                            "取消"
                        }
                        button {
                            class: "{BTN_DANGER_OUTLINE}",
                            onclick: move |_| {
                                purge_open.set(false);
                                on_purge.call(());
                            },
                            "确认删除"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn settings_pages_use_shared_collapsible_card() {
        let common_code = include_str!("../../components/ui.rs");
        assert!(common_code.contains("grid transition-all duration-300 ease-in-out"));
        assert!(common_code.contains("grid-template-rows: 1fr; opacity: 1;"));
        assert!(common_code.contains("grid-template-rows: 0fr; opacity: 0;"));

        let trash_code = include_str!("posts_trash.rs");
        assert!(trash_code.contains("CollapsibleSettingsCard"));
        let backup_code = include_str!("system/backup.rs");
        assert!(backup_code.contains("CollapsibleSettingsCard"));
    }
}
