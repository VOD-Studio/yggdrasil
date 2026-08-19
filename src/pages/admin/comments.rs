//! 评论管理页面。
//!
//! 提供评论列表、状态筛选（全部 / 待审核 / 已通过 / 垃圾箱）、批量操作与单条操作。
//! 数据加载与状态变更仅在 WASM 前端通过 Dioxus server functions 交互。

use std::collections::HashSet;

use dioxus::prelude::*;
use dioxus::router::components::Link;

// 仅在 WASM 前端使用的评论管理接口。
#[cfg(target_arch = "wasm32")]
use crate::api::comments::trash_comment;
use crate::api::comments::{approve_comment, batch_update_comment_status, spam_comment};
#[cfg(target_arch = "wasm32")]
use crate::api::comments::{get_all_comments, AllCommentsResponse};
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::admin_comments_skeleton::AdminCommentsSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{
    Checkbox, FilterTabs, Pagination, UserAvatar, BTN_GHOST, BTN_SOLID_AMBER, BTN_SOLID_GREEN,
    BTN_SOLID_RED,
};
use crate::models::comment::{AdminComment, CommentStatus};
use crate::router::Route;

/// 每页展示的评论数量。
const COMMENTS_PER_PAGE: i32 = 20;

/// 评论管理入口组件，默认展示第 1 页。
#[component]
pub fn AdminComments() -> Element {
    rsx! {
        AdminCommentsPage { page: 1 }
    }
}

/// 评论管理分页组件。
///
/// 支持按状态筛选、全选 / 单选、批量审批 / 标记垃圾 / 删除，以及单条评论状态操作。
#[component]
pub fn AdminCommentsPage(page: i32) -> Element {
    let current_page = page.max(1);
    // 当前筛选状态：优先从 URL 查询参数 `?status=` 读取（仅 WASM 前端）。
    let mut active_filter = use_signal(|| {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .and_then(|w| w.location().search().ok())
                .and_then(|s| {
                    let params = s.trim_start_matches('?');
                    for pair in params.split('&') {
                        if let Some(val) = pair.strip_prefix("status=") {
                            return Some(val.to_string());
                        }
                    }
                    None
                })
                .unwrap_or_default()
        }
        #[cfg(not(target_arch = "wasm32"))]
        String::new()
    });
    // 已选中的评论 ID 集合、评论列表、总数、加载与错误状态。
    let mut selected_ids: Signal<HashSet<i64>> = use_signal(HashSet::new);
    let mut comments: Signal<Vec<AdminComment>> = use_signal(Vec::new);
    let mut total: Signal<i64> = use_signal(|| 0);
    #[allow(unused_mut)]
    let mut loading: Signal<bool> = use_signal(|| true);
    #[allow(unused_mut)]
    let mut error: Signal<Option<String>> = use_signal(|| None);

    // 将当前筛选字符串转换为接口所需的 status 参数。
    #[allow(unused_variables)]
    let filter_status = move || {
        let f = active_filter();
        if f.is_empty() {
            None
        } else {
            Some(f)
        }
    };

    // 客户端（CSR）加载数据：筛选或页码变化时触发。
    use_effect(move || {
        let _ = active_filter();
        let _ = current_page;

        // 仅在 WASM 前端发起评论列表请求。
        #[cfg(target_arch = "wasm32")]
        {
            let page = current_page;
            let status = filter_status();
            spawn(async move {
                loading.set(true);
                error.set(None);
                match get_all_comments(page, status).await {
                    Ok(AllCommentsResponse {
                        comments: list,
                        total: t,
                    }) => {
                        comments.set(list);
                        total.set(t);
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                loading.set(false);
            });
        }
    });

    #[allow(unused_mut)]
    let mut set_comment_status = move |id: i64, status: CommentStatus| {
        comments.with_mut(|list| {
            if let Some(c) = list.iter_mut().find(|c| c.id == id) {
                c.status = status;
            }
        });
    };

    #[allow(unused_mut, unused_variables)]
    let mut remove_comment = move |id: i64| {
        comments.with_mut(|list| list.retain(|c| c.id != id));
        total.with_mut(|t| *t = t.saturating_sub(1));
    };

    rsx! {
        div { class: "animate-page-enter w-full max-w-7xl mx-auto space-y-6",
            // 页头：标题与副标题
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70",
                div {
                    h1 { class: "text-3xl sm:text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                        "评论管理"
                    }
                    p { class: "text-sm text-[var(--color-paper-secondary)] mt-1.5",
                        "所有文章评论 ({total()}) · 审核读者互动与拦截垃圾内容"
                    }
                }
            }

            // 状态筛选 Tab 胶囊
            FilterTabs {
                items: vec![
                    ("", "全部"),
                    ("pending", "待审核"),
                    ("approved", "已通过"),
                    ("spam", "垃圾箱"),
                ],
                active_value: active_filter(),
                on_change: move |v| active_filter.set(v),
            }

            // 批量操作栏（选中时浮动展开）
            if !selected_ids().is_empty() {
                div { class: "animate-row-enter flex flex-wrap items-center justify-between gap-3 p-3.5 bg-[var(--color-paper-entry)] rounded-2xl border border-[var(--color-paper-border)] shadow-xs",
                    div { class: "flex items-center gap-2 text-sm font-medium text-[var(--color-paper-primary)]",
                        span { class: "w-2 h-2 rounded-full bg-[var(--color-paper-accent)]" }
                        span { "已选中 {selected_ids().len()} 条评论" }
                    }
                    div { class: "flex items-center gap-2",
                        button {
                            class: "{BTN_SOLID_GREEN} inline-flex items-center gap-1.5",
                            onclick: move |_| {
                                let ids: Vec<i64> = selected_ids().iter().copied().collect();
                                let ids_for_api = ids.clone();
                                spawn(async move {
                                    let _ = batch_update_comment_status(ids_for_api, "approved".to_string())
                                        .await;
                                });
                                for id in &ids {
                                    set_comment_status(*id, CommentStatus::Approved);
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
                                polyline { points: "20 6 9 17 4 12" }
                            }
                            "批量通过"
                        }
                        button {
                            class: "{BTN_SOLID_AMBER} inline-flex items-center gap-1.5",
                            onclick: move |_| {
                                let ids: Vec<i64> = selected_ids().iter().copied().collect();
                                let ids_for_api = ids.clone();
                                spawn(async move {
                                    let _ = batch_update_comment_status(ids_for_api, "spam".to_string()).await;
                                });
                                for id in &ids {
                                    set_comment_status(*id, CommentStatus::Spam);
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
                                path { d: "M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" }
                                line { x1: "12", y1: "9", x2: "12", y2: "13" }
                                line { x1: "12", y1: "17", x2: "12.01", y2: "17" }
                            }
                            "批量垃圾"
                        }
                        button {
                            class: "{BTN_SOLID_RED} inline-flex items-center gap-1.5",
                            onclick: move |_| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    if web_sys::window()
                                        .and_then(|w| {
                                            w.confirm_with_message("确定要删除这些评论吗？").ok()
                                        })
                                        .unwrap_or(false)
                                    {
                                        let ids: Vec<i64> = selected_ids().iter().copied().collect();
                                        let ids_for_api = ids.clone();
                                        spawn(async move {
                                            let _ = batch_update_comment_status(ids_for_api, "trash".to_string())
                                                .await;
                                        });
                                        for id in &ids {
                                            remove_comment(*id);
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
                            "批量删除"
                        }
                        button {
                            class: "{BTN_GHOST}",
                            onclick: move |_| selected_ids.set(HashSet::new()),
                            "取消"
                        }
                    }
                }
            }
            {
                if error().is_some() {
                    rsx! {
                        EmptyState {
                            title: "加载失败",
                            description: "获取评论列表时发生错误，请稍后重试。",
                        }
                    }
                } else if loading() && comments().is_empty() {
                    rsx! {
                        DelayedSkeleton { AdminCommentsSkeleton {} }
                    }
                } else if comments().is_empty() {
                    rsx! {
                        EmptyState {
                            title: "暂无评论",
                            description: "当前分类下还没有任何评论。",
                        }
                    }
                } else {
                    let list = comments();
                    let all_selected = list.iter().all(|c| selected_ids().contains(&c.id));
                    let all_ids: Vec<i64> = list.iter().map(|c| c.id).collect();
                    rsx! {
                        div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 overflow-hidden",
                            div { class: "overflow-x-auto",
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
                                            th { class: "px-5 py-3.5 font-semibold w-48", "评论作者" }
                                            th { class: "px-5 py-3.5 font-semibold", "评论内容" }
                                            th { class: "px-5 py-3.5 font-semibold w-56", "关联文章" }
                                            th { class: "px-4 py-3.5 font-semibold text-center w-24 whitespace-nowrap",
                                                "状态"
                                            }
                                            th { class: "px-4 py-3.5 font-semibold w-28 whitespace-nowrap", "发表日期" }
                                            th { class: "px-5 py-3.5 font-semibold w-36 text-right whitespace-nowrap",
                                                "操作"
                                            }
                                        }
                                    }
                                    tbody {
                                        for (idx, comment) in list.iter().enumerate() {
                                            CommentRow {
                                                key: "{comment.id}",
                                                comment: comment.clone(),
                                                selected: selected_ids().contains(&comment.id),
                                                stagger_index: idx as u32,
                                                on_select: {
                                                    let id = comment.id;
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
                                                on_approve: {
                                                    let id = comment.id;
                                                    move |_| {
                                                        spawn(async move {
                                                            let _ = approve_comment(id).await;
                                                        });
                                                        set_comment_status(id, CommentStatus::Approved);
                                                    }
                                                },
                                                on_spam: {
                                                    let id = comment.id;
                                                    move |_| {
                                                        spawn(async move {
                                                            let _ = spam_comment(id).await;
                                                        });
                                                        set_comment_status(id, CommentStatus::Spam);
                                                    }
                                                },
                                                on_trash: {
                                                    let _id = comment.id;
                                                    move |_| {
                                                        #[cfg(target_arch = "wasm32")]
                                                        {
                                                            if web_sys::window()
                                                                .and_then(|w| {
                                                                    w.confirm_with_message("确定要删除这条评论吗？").ok()
                                                                })
                                                                .unwrap_or(false)
                                                            {
                                                                spawn(async move {
                                                                    let _ = trash_comment(_id).await;
                                                                });
                                                                remove_comment(_id);
                                                            }
                                                        }
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
                            current_page,
                            total: total(),
                            per_page: COMMENTS_PER_PAGE,
                            prev_route: if current_page - 1 <= 1 { Route::AdminComments {} } else { Route::AdminCommentsPage {
                                page: current_page - 1,
                            } },
                            next_route: Route::AdminCommentsPage {
                                page: current_page + 1,
                            },
                            unit: "条",
                        }
                    }
                }
            }
        }
    }
}

/// 评论表格行组件，展示单条评论的作者、内容、所属文章、状态与操作按钮。
#[component]
fn CommentRow(
    comment: AdminComment,
    selected: bool,
    stagger_index: u32,
    on_select: EventHandler<bool>,
    on_approve: EventHandler,
    on_spam: EventHandler,
    on_trash: EventHandler,
) -> Element {
    let date_str = comment.created_at.format("%Y-%m-%d").to_string();
    let preview = if comment.content_md.len() > 100 {
        format!(
            "{}...",
            &comment.content_md[..comment.content_md.ceil_char_boundary(100)]
        )
    } else {
        comment.content_md.clone()
    };

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
            // 作者信息：头像 + 姓名 + 邮箱
            td { class: "px-5 py-3.5",
                div { class: "flex items-center gap-2.5",
                    UserAvatar {
                        name: comment.author_name.clone(),
                        avatar_url: if comment.avatar_url.is_empty() { None } else { Some(comment.avatar_url.clone()) },
                        class: "w-8 h-8 rounded-full border border-[var(--color-paper-border)]/60 text-xs shrink-0",
                    }
                    div { class: "min-w-0",
                        div { class: "text-sm font-semibold text-[var(--color-paper-primary)] truncate",
                            "{comment.author_name}"
                        }
                        div { class: "text-xs font-mono text-[var(--color-paper-tertiary)] truncate",
                            "{comment.author_email}"
                        }
                    }
                }
            }
            // 评论内容
            td { class: "px-5 py-3.5 max-w-sm",
                p { class: "text-sm text-[var(--color-paper-primary)] leading-relaxed line-clamp-2",
                    "{preview}"
                }
            }
            // 关联文章
            td { class: "px-5 py-3.5 max-w-xs",
                Link {
                    class: "inline-flex items-center gap-1.5 text-xs font-medium text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-accent)] transition-colors line-clamp-1 leading-normal",
                    to: NavigationTarget::<Route>::External(format!("/post/{}", comment.post_slug)),
                    svg {
                        class: "w-3.5 h-3.5 shrink-0 opacity-70",
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                        polyline { points: "14 2 14 8 20 8" }
                    }
                    "{comment.post_title}"
                }
            }
            // 状态胶囊
            td { class: "px-4 py-3.5 text-center whitespace-nowrap",
                match &comment.status {
                    CommentStatus::Approved => rsx! {
                        span { class: "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20",
                            span { class: "w-1.5 h-1.5 rounded-full bg-emerald-500" }
                            "已通过"
                        }
                    },
                    CommentStatus::Pending => rsx! {
                        span { class: "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20",
                            span { class: "w-1.5 h-1.5 rounded-full bg-amber-500" }
                            "待审核"
                        }
                    },
                    CommentStatus::Spam => rsx! {
                        span { class: "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-red-500/10 text-red-600 dark:text-red-400 border border-red-500/20",
                            span { class: "w-1.5 h-1.5 rounded-full bg-red-500" }
                            "垃圾"
                        }
                    },
                    CommentStatus::Trash => rsx! {
                        span { class: "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-gray-500/10 text-gray-600 dark:text-gray-400 border border-gray-500/20",
                            span { class: "w-1.5 h-1.5 rounded-full bg-gray-400" }
                            "已删除"
                        }
                    },
                }
            }
            // 发表日期
            td { class: "px-4 py-3.5 text-xs font-mono text-[var(--color-paper-secondary)] whitespace-nowrap",
                "{date_str}"
            }
            // 操作按钮
            td { class: "px-5 py-3.5 text-right whitespace-nowrap",
                div { class: "flex justify-end items-center gap-1.5",
                    if !matches!(comment.status, CommentStatus::Approved) {
                        button {
                            class: "inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium text-emerald-600 dark:text-emerald-400 hover:bg-emerald-50 dark:hover:bg-emerald-900/20 transition-colors cursor-pointer",
                            onclick: move |_| on_approve.call(()),
                            svg {
                                class: "w-3.5 h-3.5",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                polyline { points: "20 6 9 17 4 12" }
                            }
                            "通过"
                        }
                    }
                    if !matches!(comment.status, CommentStatus::Spam) {
                        button {
                            class: "inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium text-amber-600 dark:text-amber-400 hover:bg-amber-50 dark:hover:bg-amber-900/20 transition-colors cursor-pointer",
                            onclick: move |_| on_spam.call(()),
                            svg {
                                class: "w-3.5 h-3.5",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" }
                                line { x1: "12", y1: "9", x2: "12", y2: "13" }
                                line { x1: "12", y1: "17", x2: "12.01", y2: "17" }
                            }
                            "垃圾"
                        }
                    }
                    if !matches!(comment.status, CommentStatus::Trash) {
                        button {
                            class: "inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium text-red-500 hover:text-red-700 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors cursor-pointer",
                            onclick: move |_| on_trash.call(()),
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
                            "删除"
                        }
                    }
                }
            }
        }
    }
}
