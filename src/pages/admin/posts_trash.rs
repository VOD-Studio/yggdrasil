//! 文章回收站（`/admin/posts/trash`，面包屑「文章 / 回收站」）。
//!
//! 标题行（保留期说明、清空回收站）→ 表格（勾选、标题与 slug、原状态、删除日期、
//! 自动清理剩余天数、恢复与彻底删除）与底栏分页。勾选后底部浮起批量操作条。
//! 剩余天数由 `deleted_at` 与保留天数计算，5 天内以 Clay 强调；自动清理关闭时整列
//! 不显示。彻底删除与清空都经危险确认对话框。自动清理的开关与天数在设置页修改。
//! 数据仅在 WASM 前端通过 server function 加载。样式见 styles/pages/admin-trash.css。

use std::collections::HashSet;

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::api::posts::{
    batch_purge_posts, batch_restore_posts, empty_trash, list_deleted_posts, purge_post,
    restore_post, PostListResponse,
};
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::posts_trash_skeleton::PostsTrashTableSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Checkbox, ConfirmDialog, IconButton,
    IconName, LoadError, TablePager,
};
use crate::hooks::query::use_paginated;
use crate::models::post::{PostListItem, PostStatus};
use crate::models::settings::TrashSettings;
use crate::router::Route;
use crate::utils::error::failure_message;
use crate::utils::time::{now_millis, to_local};

/// 每页展示的回收站文章数量。
const TRASH_PER_PAGE: i32 = 20;
/// 剩余天数不超过它时以 Clay 强调。
const URGENT_DAYS: i64 = 5;
const DAY_MS: i64 = 86_400_000;

/// 距自动清理还剩几天：向上取整，已到期为 0（后台任务每小时清理一次）。
fn days_left(deleted_at_ms: i64, retention_days: i32, now_ms: i64) -> i64 {
    let remaining = deleted_at_ms + retention_days as i64 * DAY_MS - now_ms;
    if remaining <= 0 {
        0
    } else {
        (remaining + DAY_MS - 1) / DAY_MS
    }
}

/// 标题下的说明；配置未取到时不提保留天数。
fn retention_note(settings: Option<&TrashSettings>) -> &'static str {
    match settings {
        Some(s) if s.auto_purge_enabled => "到期自动彻底删除。",
        Some(_) => "自动清理已关闭，文章会一直保留到手动彻底删除。",
        None => "可随时恢复或彻底删除。",
    }
}

/// 等待确认的彻底删除。
#[derive(Clone, PartialEq)]
enum PurgeTarget {
    One { id: i32, title: String },
    Selected(Vec<i32>),
    All(i64),
}

impl PurgeTarget {
    fn title(&self) -> &'static str {
        match self {
            PurgeTarget::All(_) => "清空回收站？",
            _ => "彻底删除文章？",
        }
    }

    fn description(&self) -> String {
        const REFS: &str = "被这些文章引用的素材会变为未引用。";
        match self {
            PurgeTarget::One { title, .. } => {
                format!("《{title}》会被彻底删除，无法恢复。它引用的素材会变为未引用。")
            }
            PurgeTarget::Selected(ids) => {
                format!("选中的 {} 篇文章会被彻底删除，无法恢复。{REFS}", ids.len())
            }
            PurgeTarget::All(n) => format!("{n} 篇文章会被彻底删除，无法恢复。{REFS}"),
        }
    }
}

/// 回收站页。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn PostsTrash() -> Element {
    let toast = use_toast();
    let navigator = use_navigator();
    let mut current_page = use_signal(|| 1);
    let mut selected = use_signal(HashSet::<i32>::new);
    let mut busy = use_signal(HashSet::<i32>::new);
    let mut batch_busy = use_signal(|| false);
    let mut settings = use_signal(|| None::<TrashSettings>);
    let mut confirm_open = use_signal(|| false);
    let mut pending = use_signal(|| None::<PurgeTarget>);
    let mut refresh = use_signal(|| 0_u32);

    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            if let Ok(s) = crate::api::settings::get_trash_settings().await {
                settings.set(Some(s));
            }
        });
    });

    let paginated = use_paginated(
        move || {
            let _ = refresh();
            current_page.with(|p| *p)
        },
        TRASH_PER_PAGE,
        |p, pp| async move {
            list_deleted_posts(p, pp)
                .await
                .map(|PostListResponse { posts, total }| (posts, total))
                .map_err(|e| e.to_string())
        },
    );
    let posts = paginated.items;
    let total = paginated.total;
    let loading = paginated.loading;
    let error = paginated.error;

    // 操作后取消这些文章的勾选并重新请求；本页全部移走且不是第 1 页时退回上一页。
    let mut settle = move |removed: &[i32]| {
        selected.with_mut(|set| removed.iter().for_each(|id| _ = set.remove(id)));
        let gone = posts
            .peek()
            .iter()
            .filter(|p| removed.contains(&p.id))
            .count();
        if gone >= posts.peek().len() && *current_page.peek() > 1 {
            current_page -= 1;
        } else {
            refresh += 1;
        }
    };

    let mut restore = move |ids: Vec<i32>| {
        if ids.is_empty() {
            return;
        }
        let single = ids.len() == 1;
        if single {
            busy.write().insert(ids[0]);
        } else {
            batch_busy.set(true);
        }
        spawn(async move {
            let result = if single {
                restore_post(ids[0]).await
            } else {
                batch_restore_posts(ids.clone()).await
            };
            match result {
                Ok(res) if res.success => {
                    toast.success(if single {
                        "已恢复，文章回到原来的状态".to_string()
                    } else {
                        res.message
                    });
                    settle(&ids);
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(failure_message("恢复失败", &e)),
            }
            if single {
                busy.write().remove(&ids[0]);
            } else {
                batch_busy.set(false);
            }
        });
    };

    let mut ask_purge = move |target: PurgeTarget| {
        pending.set(Some(target));
        confirm_open.set(true);
    };

    let purge = move |_| {
        let Some(target) = pending() else {
            return;
        };
        spawn(async move {
            let (result, removed) = match &target {
                PurgeTarget::One { id, .. } => {
                    busy.write().insert(*id);
                    (purge_post(*id).await, vec![*id])
                }
                PurgeTarget::Selected(ids) => {
                    batch_busy.set(true);
                    (batch_purge_posts(ids.clone()).await, ids.clone())
                }
                PurgeTarget::All(_) => {
                    batch_busy.set(true);
                    (empty_trash().await, Vec::new())
                }
            };
            match result {
                Ok(res) if res.success => {
                    toast.success(match &target {
                        PurgeTarget::One { title, .. } => format!("已彻底删除《{title}》"),
                        PurgeTarget::All(_) => "回收站已清空".to_string(),
                        PurgeTarget::Selected(_) => res.message,
                    });
                    if matches!(target, PurgeTarget::All(_)) {
                        selected.write().clear();
                        current_page.set(1);
                        refresh += 1;
                    } else {
                        settle(&removed);
                    }
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(failure_message("彻底删除失败", &e)),
            }
            if let PurgeTarget::One { id, .. } = target {
                busy.write().remove(&id);
            } else {
                batch_busy.set(false);
            }
        });
    };

    let rows = posts();
    let selection = selected();
    let page_all = !rows.is_empty() && rows.iter().all(|p| selection.contains(&p.id));
    let cfg = settings();
    let retention = cfg
        .as_ref()
        .filter(|s| s.auto_purge_enabled)
        .map(|s| s.retention_days);
    let now = now_millis();
    let confirm = pending();

    rsx! {
        div { class: "admin-trash",
            div { class: "admin-page-head",
                div {
                    h1 { "回收站" }
                    p {
                        if let Some(days) = retention {
                            "已删除的文章保留 {days} 天，"
                        }
                        "{retention_note(cfg.as_ref())}"
                        if cfg.is_some() {
                            Link { class: "admin-trash-settings", to: Route::SiteSettingsPage {},
                                "可在设置中修改"
                            }
                            "。"
                        }
                    }
                }
                if total() > 0 {
                    div { class: "admin-page-actions",
                        button {
                            class: "{button_class(ButtonVariant::Outline, ButtonSize::Md)} admin-trash-empty",
                            r#type: "button",
                            disabled: batch_busy(),
                            onclick: move |_| ask_purge(PurgeTarget::All(total())),
                            "清空回收站"
                        }
                    }
                }
            }

            if !loading() || error().is_some() {
                span { hidden: true, "data-vt-list": "true" }
            }
            if error().is_some() {
                LoadError { title: "回收站加载失败", on_retry: move |_| refresh += 1 }
            } else if loading() && rows.is_empty() {
                DelayedSkeleton { PostsTrashTableSkeleton {} }
            } else if rows.is_empty() {
                div { class: "admin-panel",
                    EmptyState {
                        icon: IconName::Trash,
                        title: "回收站是空的",
                        description: match retention {
                            Some(days) => format!("删除的文章会在这里保留 {days} 天。"),
                            None => "删除的文章会出现在这里，可随时恢复。".to_string(),
                        },
                        action: EmptyStateAction {
                            label: "返回文章".to_string(),
                            onclick: Callback::new(move |_| {
                                navigator.push(Route::Posts {});
                            }),
                        },
                    }
                }
            } else {
                div { class: "admin-panel admin-table-wrap", "aria-busy": loading().then_some("true"),
                    table { class: "admin-table admin-trash-table",
                        thead {
                            tr {
                                th { class: "admin-table-check",
                                    Checkbox {
                                        checked: page_all,
                                        label: "选择本页全部文章".to_string(),
                                        onchange: {
                                            let ids: Vec<i32> = rows.iter().map(|p| p.id).collect();
                                            move |checked: bool| {
                                                let mut set = selected.write();
                                                for id in &ids {
                                                    if checked {
                                                        set.insert(*id);
                                                    } else {
                                                        set.remove(id);
                                                    }
                                                }
                                            }
                                        },
                                    }
                                }
                                th { "标题" }
                                th { "原状态" }
                                th { class: "admin-trash-deleted", "删除于" }
                                if retention.is_some() {
                                    th { class: "admin-trash-left", "自动清理" }
                                }
                                th { class: "admin-trash-actions", span { class: "sr-only", "操作" } }
                            }
                        }
                        tbody {
                            for post in rows.iter().cloned() {
                                TrashRow {
                                    key: "{post.id}",
                                    selected: selection.contains(&post.id),
                                    busy: busy().contains(&post.id) || batch_busy(),
                                    left: retention.zip(post.deleted_at).map(|(days, at)| {
                                        (days_left(at.timestamp_millis(), days, now), days)
                                    }),
                                    on_select: move |(id, checked): (i32, bool)| {
                                        if checked {
                                            selected.write().insert(id);
                                        } else {
                                            selected.write().remove(&id);
                                        }
                                    },
                                    on_restore: move |id| restore(vec![id]),
                                    on_purge: move |(id, title)| ask_purge(PurgeTarget::One { id, title }),
                                    post,
                                }
                            }
                        }
                    }
                    TablePager {
                        page: current_page(),
                        total: total(),
                        per_page: TRASH_PER_PAGE,
                        unit: "篇",
                        on_change: move |page| {
                            selected.write().clear();
                            current_page.set(page);
                        },
                    }
                }
            }

            if !selection.is_empty() {
                div { class: "batch-bar", role: "toolbar", aria_label: "批量操作",
                    span { class: "batch-bar-count", "已选 {selection.len()} 篇" }
                    button {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                        r#type: "button",
                        disabled: batch_busy(),
                        onclick: move |_| restore(selected().into_iter().collect()),
                        "批量恢复"
                    }
                    button {
                        class: "{button_class(ButtonVariant::Ghost, ButtonSize::Sm)} batch-bar-danger",
                        r#type: "button",
                        disabled: batch_busy(),
                        onclick: move |_| ask_purge(PurgeTarget::Selected(selected().into_iter().collect())),
                        "彻底删除"
                    }
                    span { class: "batch-bar-sep", aria_hidden: "true" }
                    IconButton {
                        icon: IconName::X,
                        label: "取消选择",
                        size: ButtonSize::Sm,
                        onclick: move |_| selected.write().clear(),
                    }
                }
            }

            ConfirmDialog {
                visible: confirm_open,
                title: confirm.as_ref().map_or("彻底删除文章？", PurgeTarget::title),
                description: confirm.as_ref().map(PurgeTarget::description).unwrap_or_default(),
                confirm_label: "彻底删除",
                on_confirm: purge,
            }
        }
    }
}

/// 表格的一行。`left` 为（剩余天数，保留天数），自动清理关闭时为 None。
#[component]
fn TrashRow(
    post: PostListItem,
    selected: bool,
    busy: bool,
    left: Option<(i64, i32)>,
    on_select: EventHandler<(i32, bool)>,
    on_restore: EventHandler<i32>,
    on_purge: EventHandler<(i32, String)>,
) -> Element {
    let published = post.status == PostStatus::Published;
    let deleted = post
        .deleted_at
        .map(|d| to_local(d).format("%Y-%m-%d").to_string());
    let id = post.id;
    let title = post.title.clone();
    rsx! {
        tr { "data-selected": selected.then_some("true"), "aria-busy": busy.then_some("true"),
            td { class: "admin-table-check",
                Checkbox {
                    checked: selected,
                    label: format!("选择《{}》", post.title),
                    onchange: move |checked| on_select.call((id, checked)),
                }
            }
            td { class: "admin-trash-title",
                span { class: "admin-trash-name", "{post.title}" }
                span { class: "admin-posts-slug", "/{post.slug}" }
            }
            td { class: "admin-trash-status",
                span { class: "status-dot", "data-tone": if published { "clay" } else { "muted" },
                    "{post.status.label()}"
                }
            }
            td { class: "admin-trash-deleted",
                if let Some(deleted) = deleted {
                    time { "{deleted}" }
                } else {
                    span { class: "admin-table-muted", "—" }
                }
            }
            if let Some((days, retention)) = left {
                td { class: "admin-trash-left",
                    div {
                        class: "trash-left",
                        "data-urgent": (days <= URGENT_DAYS).then_some("true"),
                        span { class: "trash-left-track", aria_hidden: "true",
                            span {
                                class: "trash-left-bar",
                                style: "width: {(days as f64 / retention.max(1) as f64 * 100.0).min(100.0):.0}%",
                            }
                        }
                        span { class: "trash-left-label",
                            if days == 0 { "即将清理" } else { "剩 {days} 天" }
                        }
                    }
                }
            }
            td { class: "admin-trash-actions",
                div {
                    button {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                        r#type: "button",
                        disabled: busy,
                        onclick: move |_| on_restore.call(id),
                        "恢复"
                    }
                    IconButton {
                        icon: IconName::Trash,
                        label: "彻底删除",
                        size: ButtonSize::Sm,
                        disabled: busy,
                        tooltip_align: "end",
                        class: "admin-trash-purge",
                        onclick: move |_| on_purge.call((id, title.clone())),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_760_000_000_000;

    #[test]
    fn days_left_rounds_up_partial_days() {
        // 刚删除：整段保留期。
        assert_eq!(days_left(NOW, 30, NOW), 30);
        // 删除 1 小时：仍按 30 天显示。
        assert_eq!(days_left(NOW - 3_600_000, 30, NOW), 30);
        // 删除 25 天又 1 小时：还剩不到 5 天，显示 5。
        assert_eq!(days_left(NOW - 25 * DAY_MS - 3_600_000, 30, NOW), 5);
    }

    #[test]
    fn days_left_is_zero_once_expired() {
        assert_eq!(days_left(NOW - 30 * DAY_MS, 30, NOW), 0);
        assert_eq!(days_left(NOW - 40 * DAY_MS, 30, NOW), 0);
    }

    #[test]
    fn retention_note_follows_auto_purge() {
        let on = TrashSettings {
            auto_purge_enabled: true,
            retention_days: 30,
        };
        let off = TrashSettings {
            auto_purge_enabled: false,
            ..on.clone()
        };
        assert_eq!(retention_note(Some(&on)), "到期自动彻底删除。");
        assert!(retention_note(Some(&off)).contains("已关闭"));
        assert_eq!(retention_note(None), "可随时恢复或彻底删除。");
    }

    #[test]
    fn purge_target_describes_scope() {
        let one = PurgeTarget::One {
            id: 1,
            title: "标题".to_string(),
        };
        assert!(one.description().starts_with("《标题》"));
        assert_eq!(PurgeTarget::All(4).title(), "清空回收站？");
        assert!(PurgeTarget::Selected(vec![1, 2])
            .description()
            .starts_with("选中的 2 篇"));
    }
}
