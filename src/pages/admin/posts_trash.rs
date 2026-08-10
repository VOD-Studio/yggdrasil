//! 回收站页面（`/admin/posts/trash` 独立路由，属侧边栏「内容管理」子菜单）。
//!
//! 展示已软删除文章，支持恢复、彻底删除、批量操作、一键清空，
//! 以及自动清理配置（启用开关 + 保留天数）。
//! 数据加载与操作仅在 WASM 前端通过 Dioxus server functions 交互。
//!
//! 原为 `/admin/posts` 单路由下的客户端 tab（由 `posts.rs::Posts` 容器渲染），
//! issue #17 起提升为独立路由：页面 header 由本组件自带（标题 + 删除计数副标题，
//! 计数取自 `use_paginated` 的 `total`），翻页仍走客户端 signal。

use std::collections::HashSet;

use dioxus::prelude::*;

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
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::posts_trash_skeleton::PostsTrashSkeleton;
use crate::components::ui::{
    LoadingButton, Pagination, Popover, StatusBadge, ADMIN_ROW_HOVER, ADMIN_TABLE_CLASS,
    BTN_DANGER_OUTLINE, BTN_ICON, BTN_SOLID_GREEN, BTN_SOLID_RED, BTN_TEXT_ACCENT, BTN_TEXT_RED,
    CHECKBOX_CLASS,
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
        div { class: "w-full max-w-7xl mx-auto space-y-6",
            // 页面 header：计数取自本组件 use_paginated 的 total（比旧 get_post_stats 角标更准）。
            div { class: "pb-6 border-b border-paper-border mb-6",
                div {
                    h1 { class: "text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                        "回收站"
                    }
                    p { class: "text-base text-[var(--color-paper-secondary)] mt-2",
                        "{subtitle}"
                    }
                }
            }
            div { class: "space-y-6",
                // 自动清理配置卡片（抽取为子组件 AutoPurgeSettings，见文件末尾）。
                AutoPurgeSettings { settings }

                // 批量操作栏（选中时展开，取消时收起过渡）
                div { class: if selected_ids().is_empty() { "batch-bar is-collapsed" } else { "batch-bar" },
                    div { class: "flex items-center gap-3 p-3 bg-paper-theme rounded-lg",
                        span { class: "text-sm text-paper-secondary",
                            "已选择 {selected_ids().len()} 条"
                        }
                        button {
                            class: "{BTN_SOLID_GREEN}",
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
                            "批量恢复"
                        }
                        button {
                            class: "{BTN_SOLID_RED}",
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
                            "批量彻底删除"
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
                                PostsTrashSkeleton {}
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
                            div { class: "{ADMIN_TABLE_CLASS}",
                                div { class: "overflow-x-auto",
                                    table { class: "w-full text-sm",
                                        thead {
                                            tr { class: "border-b border-paper-border text-left text-paper-secondary",
                                                th { class: "px-4 py-3 font-medium w-10",
                                                    input {
                                                        r#type: "checkbox",
                                                        class: "{CHECKBOX_CLASS}",
                                                        checked: all_selected,
                                                        onchange: {
                                                            move |_| {
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
                                                            }
                                                        },
                                                    }
                                                }
                                                th { class: "px-4 py-3 font-medium", "标题" }
                                                th { class: "px-4 py-3 font-medium whitespace-nowrap", "原状态" }
                                                th { class: "px-4 py-3 font-medium w-32 whitespace-nowrap", "删除时间" }
                                                th { class: "px-4 py-3 font-medium w-24 text-center whitespace-nowrap",
                                                    "剩余"
                                                }
                                                th { class: "px-4 py-3 font-medium w-32 text-right whitespace-nowrap",
                                                    "操作"
                                                }
                                            }
                                        }
                                        tbody {
                                            for post in list.iter() {
                                                TrashRow {
                                                    key: "{post.id}",
                                                    post: post.clone(),
                                                    retention_days: settings().retention_days,
                                                    selected: selected_ids().contains(&post.id),
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
                            // 底部：清空回收站 + 分页
                            div { class: "flex items-center justify-between mt-4",
                                button {
                                    class: "{BTN_DANGER_OUTLINE}",
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
                                    "清空回收站"
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

/// 自动清理配置子组件：可折叠的设置面板。
///
/// 封装自动清理的全部状态：表单草稿（`settings_draft_*`）、面板折叠态、保存态、
/// 已保存反馈、以及派生的 `dirty` / `chevron_rotate`。配置加载与保存均在组件
/// 内部完成。`settings`（已保存的服务端配置）由父组件传入双向绑定 signal：
/// 本组件加载/保存时写入，父组件读取 `retention_days` 供 TrashRow 的「剩余天数」。
///
/// 从 `PostsTrashPage` 抽取以降低 god component 复杂度（见 dioxus-render-purity skill）。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn AutoPurgeSettings(settings: Signal<TrashSettings>) -> Element {
    let mut settings_draft_days: Signal<String> = use_signal(|| "30".to_string());
    let mut settings_draft_enabled: Signal<bool> = use_signal(|| false);
    let mut settings_panel_open: Signal<bool> = use_signal(|| false);
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
    // 折叠箭头旋转类（展开时翻转 180°）。
    let chevron_rotate = if settings_panel_open() {
        "rotate-180"
    } else {
        ""
    };

    rsx! {
        div { class: "rounded-2xl border border-paper-border overflow-hidden bg-paper-entry",
            // 顶部可点击摘要条：状态指示灯 + 标题 + 展开箭头
            button {
                class: "w-full flex items-center gap-3 px-5 py-4 text-left cursor-pointer hover:bg-paper-theme focus:outline-none focus-visible:ring-2 focus-visible:ring-paper-accent/40",
                onclick: move |_| {
                    settings_panel_open.set(!settings_panel_open());
                    just_saved.set(false);
                },
                // 状态指示灯
                {
                    let dot_class = if settings().auto_purge_enabled {
                        "w-2 h-2 rounded-full bg-paper-accent shadow-[0_0_0_3px_rgba(64,160,43,0.15)]"
                    } else {
                        "w-2 h-2 rounded-full bg-paper-tertiary"
                    };
                    rsx! {
                        div { class: "w-2 flex-shrink-0 flex items-center justify-center",
                            div { class: "{dot_class}" }
                        }
                    }
                }
                // 标题 + 当前状态描述
                div { class: "flex-1 min-w-0",
                    div { class: "text-sm font-medium text-paper-primary", "自动清理" }
                    div { class: "text-xs text-paper-secondary mt-0.5 truncate",
                        if settings().auto_purge_enabled {
                            "已开启 · 超过 {settings().retention_days} 天的文章将被自动删除"
                        } else {
                            "已关闭"
                        }
                    }
                }
                // 展开箭头（旋转动画）
                svg {
                    class: "w-4 h-4 text-paper-secondary transition-transform duration-200 flex-shrink-0 {chevron_rotate}",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M19 9l-7 7-7-7",
                    }
                }
            }

            // 设置面板（可折叠带平滑动画）
            div {
                class: "grid transition-all duration-300 ease-in-out",
                style: if settings_panel_open() { "grid-template-rows: 1fr; opacity: 1; pointer-events: auto;" } else { "grid-template-rows: 0fr; opacity: 0; pointer-events: none;" },
                div { class: "overflow-hidden min-h-0",
                    div { class: "border-t border-paper-border p-5 space-y-6",
                        // 开关行：启用自动清理
                        div { class: "flex items-center justify-between gap-4",
                            div { class: "min-w-0",
                                div { class: "text-sm font-medium text-paper-primary",
                                    "启用自动清理"
                                }
                                div { class: "text-xs text-paper-secondary mt-1",
                                    "后台任务定期彻底删除超过保留期的文章"
                                }
                            }
                            // 自定义开关（toggle switch）—— 取代原生 checkbox
                            button {
                                role: "switch",
                                aria_checked: "{settings_draft_enabled()}",
                                class: if settings_draft_enabled() { "relative w-11 h-6 flex-shrink-0 rounded-full bg-paper-accent cursor-pointer transition-colors duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-paper-accent/40" } else { "relative w-11 h-6 flex-shrink-0 rounded-full bg-paper-tertiary cursor-pointer transition-colors duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-paper-accent/40" },
                                onclick: move |_| {
                                    settings_draft_enabled.set(!settings_draft_enabled());
                                    just_saved.set(false);
                                },
                                // 滑块圆点
                                span { class: if settings_draft_enabled() { "absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm dark:shadow-black/30 transition-transform duration-200 translate-x-5" } else { "absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm dark:shadow-black/30 transition-transform duration-200" } }
                            }
                        }

                        // 保留天数行
                        div { class: "space-y-3",
                            div { class: "min-w-0",
                                div { class: "text-sm font-medium text-paper-primary",
                                    "保留天数"
                                }
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
                                span { class: "text-xs text-transparent select-none",
                                    "·"
                                }
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
        tr { class: "{ADMIN_ROW_HOVER}",
            td { class: "px-4 py-3",
                input {
                    r#type: "checkbox",
                    class: "{CHECKBOX_CLASS}",
                    checked: selected,
                    onchange: move |e| on_select.call(e.checked()),
                }
            }
            td { class: "px-4 py-3",
                div { class: "text-sm font-medium text-paper-primary truncate max-w-xs",
                    "{post.title}"
                }
            }
            td { class: "px-4 py-3 whitespace-nowrap",
                StatusBadge {
                    color_class: post.status_badge_class(),
                    label: post.status_label().to_string(),
                }
            }
            td { class: "px-4 py-3 text-sm text-paper-secondary whitespace-nowrap",
                "{deleted_str}"
            }
            td { class: "px-4 py-3 text-center whitespace-nowrap",
                StatusBadge { color_class: badge_class, label: badge_text }
            }
            td { class: "px-4 py-3 text-right whitespace-nowrap",
                div { class: "flex justify-end gap-2",
                    button {
                        class: "{BTN_TEXT_ACCENT}",
                        onclick: move |_| on_restore.call(()),
                        "恢复"
                    }
                    button {
                        class: "{BTN_TEXT_RED}",
                        onclick: move |e| {
                            let coordinates = e.client_coordinates();
                            anchor_x.set(coordinates.x as i32);
                            anchor_y.set(coordinates.y as i32);
                            purge_open.set(true);
                        },
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
                            class: "px-3 py-1.5 text-xs text-paper-secondary hover:text-paper-primary transition-colors cursor-pointer",
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
    fn test_auto_purge_settings_has_transition_class() {
        let full_code = include_str!("posts_trash.rs");
        let code = full_code.split("#[cfg(test)]").next().unwrap_or(full_code);
        assert!(code.contains("grid transition-all duration-300 ease-in-out"));
        assert!(code.contains("grid-template-rows: 1fr; opacity: 1;"));
        assert!(code.contains("grid-template-rows: 0fr; opacity: 0;"));
    }
}
