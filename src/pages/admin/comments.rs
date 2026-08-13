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
    Checkbox, FilterTabs, Pagination, StatusBadge, ADMIN_ROW_HOVER, ADMIN_TABLE_CLASS,
    BTN_SOLID_AMBER, BTN_SOLID_GREEN, BTN_SOLID_RED, BTN_TEXT_AMBER, BTN_TEXT_GREEN, BTN_TEXT_RED,
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
            div { class: "flex flex-col md:flex-row md:items-end justify-between gap-6 pb-6 border-b border-[var(--color-paper-border)] mb-6",
                div {
                    h1 { class: "text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                        "评论管理"
                    }
                    p { class: "text-base text-[var(--color-paper-secondary)] mt-2",
                        "所有文章评论"
                    }
                }
            }

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

            div { class: if selected_ids().is_empty() { "batch-bar is-collapsed" } else { "batch-bar" },
                div { class: "flex items-center gap-3 p-3 bg-paper-theme rounded-lg",
                    span { class: "text-sm text-paper-secondary", "已选择 {selected_ids().len()} 条" }
                    button {
                        class: "{BTN_SOLID_GREEN}",
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
                        "批量通过"
                    }
                    button {
                        class: "{BTN_SOLID_AMBER}",
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
                        "批量垃圾"
                    }
                    button {
                        class: "{BTN_SOLID_RED}",
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
                        "批量删除"
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
                        div { class: "{ADMIN_TABLE_CLASS}",
                            div { class: "overflow-x-auto",
                                table { class: "w-full text-sm",
                                    thead {
                                        tr { class: "border-b border-paper-border text-left text-paper-secondary",
                                            th { class: "px-4 py-3 font-medium w-10",
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
                                            th { class: "px-4 py-3 font-medium", "作者" }
                                            th { class: "px-4 py-3 font-medium", "内容" }
                                            th { class: "px-4 py-3 font-medium", "文章" }
                                            th { class: "px-4 py-3 font-medium text-center w-24 whitespace-nowrap",
                                                "状态"
                                            }
                                            th { class: "px-4 py-3 font-medium w-32 whitespace-nowrap", "日期" }
                                            th { class: "px-4 py-3 font-medium w-32 text-right whitespace-nowrap",
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
    let status_label = match &comment.status {
        CommentStatus::Pending => "待审核".to_string(),
        CommentStatus::Approved => "已通过".to_string(),
        CommentStatus::Spam => "垃圾".to_string(),
        CommentStatus::Trash => "已删除".to_string(),
    };
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
            class: "animate-row-enter {ADMIN_ROW_HOVER}",
            style: "animation-delay: {stagger_index * 35}ms",
            td { class: "px-4 py-3",
                Checkbox {
                    checked: selected,
                    onchange: move |checked: bool| on_select.call(checked),
                }
            }
            td { class: "px-4 py-3",
                div { class: "flex items-center gap-2",
                    img {
                        class: "w-8 h-8 rounded-full flex-shrink-0",
                        src: "{comment.avatar_url}",
                        alt: "{comment.author_name}",
                    }
                    div { class: "min-w-0",
                        div { class: "text-sm font-medium text-paper-primary truncate",
                            "{comment.author_name}"
                        }
                        div { class: "text-xs text-paper-secondary truncate", "{comment.author_email}" }
                    }
                }
            }
            td { class: "px-4 py-3 max-w-xs",
                p { class: "text-sm text-paper-secondary truncate", "{preview}" }
            }
            td { class: "px-4 py-3",
                Link {
                    class: "text-sm text-paper-primary hover:text-paper-accent transition-colors",
                    to: Route::PostDetail {
                        slug: comment.post_slug.clone(),
                    },
                    "{comment.post_title}"
                }
            }
            td { class: "px-4 py-3 text-center whitespace-nowrap",
                StatusBadge {
                    // badge_class 是 &'static str 字面量匹配，转为静态生命周期。
                    color_class: match &comment.status {
                        CommentStatus::Pending => {
                            "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400"
                        }
                        CommentStatus::Approved => {
                            "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400"
                        }
                        CommentStatus::Spam => {
                            "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400"
                        }
                        CommentStatus::Trash => {
                            "bg-gray-100 text-gray-700 dark:bg-gray-900/30 dark:text-gray-400"
                        }
                    },
                    label: status_label,
                }
            }
            td { class: "px-4 py-3 text-sm text-paper-secondary whitespace-nowrap",
                "{date_str}"
            }
            td { class: "px-4 py-3 text-right whitespace-nowrap",
                div { class: "flex justify-end gap-2",
                    if !matches!(comment.status, CommentStatus::Approved) {
                        button {
                            class: "{BTN_TEXT_GREEN}",
                            onclick: move |_| on_approve.call(()),
                            "通过"
                        }
                    }
                    if !matches!(comment.status, CommentStatus::Spam) {
                        button {
                            class: "{BTN_TEXT_AMBER}",
                            onclick: move |_| on_spam.call(()),
                            "垃圾"
                        }
                    }
                    if !matches!(comment.status, CommentStatus::Trash) {
                        button {
                            class: "{BTN_TEXT_RED}",
                            onclick: move |_| on_trash.call(()),
                            "删除"
                        }
                    }
                }
            }
        }
    }
}
