//! 文章回收站（`/admin/posts/trash`，面包屑「文章 / 回收站」）。
//!
//! 标题行（保留期说明、清空回收站）→ 表格（勾选、标题与 slug、原状态、删除日期、
//! 自动清理剩余天数、恢复与彻底删除）与底栏分页。勾选后底部浮起批量操作条。
//! 剩余天数由 `deleted_at` 与保留天数计算，5 天内以 Clay 强调；自动清理关闭时整列
//! 不显示。彻底删除与清空都经危险确认对话框。自动清理的开关与天数在设置页修改。
//! 数据仅在 WASM 前端通过 server function 加载（后台数据是私有的，SSR 不取数）。
//! 样式见 styles/pages/admin-trash.css。

use std::collections::HashSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::posts::{
    batch_purge_posts, batch_restore_posts, empty_trash, list_deleted_posts, purge_post,
    restore_post, PostListResponse,
};
use crate::components::admin_layout::AdminPageHead;
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::posts_trash_skeleton::PostsTrashTableSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Checkbox, ConfirmDialog, IconButton,
    IconName, LoadError, StatusDot, TablePager,
};
use crate::hooks::query::use_paginated;
use crate::models::post::PostListItem;
use crate::models::settings::TrashSettings;
use crate::routes::Route;
use crate::utils::error::failure_message;
use crate::utils::time::{now_millis, to_local};

/// 每页展示的回收站文章数量。
const TRASH_PER_PAGE: i32 = 20;
/// 剩余天数不超过它时以 Clay 强调。
const URGENT_DAYS: i64 = 5;
const DAY_MS: i64 = 86_400_000;

/// `/admin/posts/trash` 路由：纯客户端数据，`data()` 不取数。
pub struct PostsTrashRoute;

#[lazy_route]
impl LazyRoute for PostsTrashRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <PostsTrash/> }.into_any()
    }
}

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

/// 列表区域的四种互斥状态。
#[derive(Clone, Copy, PartialEq)]
enum ListState {
    Error,
    Loading,
    Empty,
    Table,
}

/// 回收站页。
#[component]
fn PostsTrash() -> impl IntoView {
    let toast = use_toast();
    let current_page = RwSignal::new(1);
    let selected = RwSignal::new(HashSet::<i32>::new());
    let busy = RwSignal::new(HashSet::<i32>::new());
    let batch_busy = RwSignal::new(false);
    let settings = RwSignal::new(None::<TrashSettings>);
    // 两种确认对话框标题不同，各用一个对话框，避免按目标重建（标题不是响应式 prop）。
    let confirm_open = RwSignal::new(false);
    let confirm_all_open = RwSignal::new(false);
    let pending = RwSignal::new(None::<PurgeTarget>);
    let refresh = RwSignal::new(0_u32);

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(s) = crate::api::settings::get_trash_settings().await {
                settings.try_set(Some(s));
            }
        });
    });

    let paginated = use_paginated(
        move || {
            refresh.track();
            current_page.get()
        },
        TRASH_PER_PAGE,
        |p, pp| async move {
            list_deleted_posts(p, pp)
                .await
                .map(|PostListResponse { posts, total }| (posts, total))
                .map_err(|e| failure_message("回收站加载失败", &e))
        },
    );
    let posts = paginated.items;
    let total = paginated.total;
    let loading = paginated.loading;
    let error = paginated.error;

    // 操作后取消这些文章的勾选并重新请求；本页全部移走且不是第 1 页时退回上一页。
    let settle = move |removed: &[i32]| {
        selected.try_update(|set| removed.iter().for_each(|id| _ = set.remove(id)));
        let Some((gone, len)) = posts.try_with_untracked(|list| {
            (
                list.iter().filter(|p| removed.contains(&p.id)).count(),
                list.len(),
            )
        }) else {
            return;
        };
        if gone >= len && current_page.try_get_untracked().is_some_and(|p| p > 1) {
            current_page.try_update(|p| *p -= 1);
        } else {
            refresh.try_update(|n| *n += 1);
        }
    };

    let restore = move |ids: Vec<i32>| {
        if ids.is_empty() {
            return;
        }
        let single = ids.len() == 1;
        if single {
            busy.update(|b| _ = b.insert(ids[0]));
        } else {
            batch_busy.set(true);
        }
        spawn_local(async move {
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
                busy.try_update(|b| _ = b.remove(&ids[0]));
            } else {
                batch_busy.try_set(false);
            }
        });
    };

    let ask_purge = move |target: PurgeTarget| {
        let all = matches!(target, PurgeTarget::All(_));
        pending.set(Some(target));
        if all {
            confirm_all_open.set(true);
        } else {
            confirm_open.set(true);
        }
    };

    let purge = move |_: ()| {
        let Some(target) = pending.get_untracked() else {
            return;
        };
        spawn_local(async move {
            let (result, removed) = match &target {
                PurgeTarget::One { id, .. } => {
                    busy.update(|b| _ = b.insert(*id));
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
                        selected.try_update(|s| s.clear());
                        current_page.try_set(1);
                        refresh.try_update(|n| *n += 1);
                    } else {
                        settle(&removed);
                    }
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(failure_message("彻底删除失败", &e)),
            }
            if let PurgeTarget::One { id, .. } = target {
                busy.try_update(|b| _ = b.remove(&id));
            } else {
                batch_busy.try_set(false);
            }
        });
    };

    let on_row_select = Callback::new(move |(id, checked): (i32, bool)| {
        selected.update(|s| {
            if checked {
                s.insert(id);
            } else {
                s.remove(&id);
            }
        });
    });
    let on_row_restore = Callback::new(move |id: i32| restore(vec![id]));
    let on_row_purge =
        Callback::new(move |(id, title): (i32, String)| ask_purge(PurgeTarget::One { id, title }));

    let page_all = Memo::new(move |_| {
        posts.with(|rows| {
            selected.with(|selection| {
                !rows.is_empty() && rows.iter().all(|p| selection.contains(&p.id))
            })
        })
    });
    // 自动清理开启时的保留天数；关闭或配置未取到时为 None。
    let retention = Memo::new(move |_| {
        settings.with(|s| {
            s.as_ref()
                .filter(|s| s.auto_purge_enabled)
                .map(|s| s.retention_days)
        })
    });
    let list_state = Memo::new(move |_| {
        if error.with(Option::is_some) {
            ListState::Error
        } else if posts.with(Vec::is_empty) {
            if loading.get() {
                ListState::Loading
            } else {
                ListState::Empty
            }
        } else {
            ListState::Table
        }
    });
    let selection_count = Memo::new(move |_| selected.with(HashSet::len));
    let has_selection = Memo::new(move |_| selection_count.get() > 0);
    let confirm_description = Signal::derive(move || {
        pending.with(|p| p.as_ref().map(PurgeTarget::description).unwrap_or_default())
    });

    view! {
        <div class="admin-trash">
            <TrashHead
                settings=settings
                total=total
                batch_busy=batch_busy
                on_empty=move |_: ()| ask_purge(PurgeTarget::All(total.get_untracked()))
            />

            {move || {
                (!loading.get() || error.with(Option::is_some))
                    .then(|| view! { <span hidden data-vt-list="true"></span> })
            }}
            {move || match list_state.get() {
                ListState::Error => {
                    view! {
                        <LoadError
                            title="回收站加载失败"
                            on_retry=move |_: ()| {
                                refresh.update(|n| *n += 1);
                            }
                        />
                    }
                        .into_any()
                }
                ListState::Loading => {
                    view! {
                        <DelayedSkeleton>
                            <PostsTrashTableSkeleton/>
                        </DelayedSkeleton>
                    }
                        .into_any()
                }
                ListState::Empty => {
                    view! {
                        <div class="admin-panel">
                            // 说明文字依赖保留天数（配置异步到达），EmptyState 的 description 不是响应式 prop，
                            // 配置变化时按新文案重建。
                            {move || {
                                let description = match retention.get() {
                                    Some(days) => format!("删除的文章会在这里保留 {days} 天。"),
                                    None => "删除的文章会出现在这里，可随时恢复。".to_string(),
                                };
                                view! {
                                    <EmptyState
                                        icon=IconName::Trash
                                        title="回收站是空的"
                                        description=description
                                        action=EmptyStateAction::new(
                                            "返回文章",
                                            move |_| {
                                                crate::navigation::push(&Route::Posts.to_string());
                                            },
                                        )
                                    />
                                }
                            }}
                        </div>
                    }
                        .into_any()
                }
                ListState::Table => {
                    view! {
                        <div
                            class="admin-panel admin-table-wrap"
                            aria-busy=move || loading.get().then_some("true")
                        >
                            <table class="admin-table admin-trash-table">
                                <thead>
                                    <tr>
                                        <th class="admin-table-check">
                                            <Checkbox
                                                checked=page_all
                                                label="选择本页全部文章".to_string()
                                                onchange=move |checked: bool| {
                                                    let ids: Vec<i32> = posts
                                                        .with_untracked(|rows| rows.iter().map(|p| p.id).collect());
                                                    selected
                                                        .update(|set| {
                                                            for id in &ids {
                                                                if checked {
                                                                    set.insert(*id);
                                                                } else {
                                                                    set.remove(id);
                                                                }
                                                            }
                                                        });
                                                }
                                            />
                                        </th>
                                        <th>"标题"</th>
                                        <th>"原状态"</th>
                                        <th class="admin-trash-deleted">"删除于"</th>
                                        {move || {
                                            retention
                                                .get()
                                                .is_some()
                                                .then(|| view! { <th class="admin-trash-left">"自动清理"</th> })
                                        }}
                                        <th class="admin-trash-actions">
                                            <span class="sr-only">"操作"</span>
                                        </th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {move || {
                                        let retention = retention.get();
                                        let now = now_millis();
                                        posts
                                            .get()
                                            .into_iter()
                                            .map(|post| {
                                                let id = post.id;
                                                let left = retention
                                                    .zip(post.deleted_at)
                                                    .map(|(days, at)| {
                                                        (days_left(at.timestamp_millis(), days, now), days)
                                                    });
                                                view! {
                                                    <TrashRow
                                                        selected=Signal::derive(move || {
                                                            selected.with(|s| s.contains(&id))
                                                        })
                                                        busy=Signal::derive(move || {
                                                            batch_busy.get() || busy.with(|b| b.contains(&id))
                                                        })
                                                        left=left
                                                        on_select=on_row_select
                                                        on_restore=on_row_restore
                                                        on_purge=on_row_purge
                                                        post=post
                                                    />
                                                }
                                            })
                                            .collect_view()
                                    }}
                                </tbody>
                            </table>
                            <TablePager
                                page=current_page
                                total=total
                                per_page=TRASH_PER_PAGE
                                unit="篇"
                                on_change=move |page: i32| {
                                    selected.update(|s| s.clear());
                                    current_page.set(page);
                                }
                            />
                        </div>
                    }
                        .into_any()
                }
            }}

            <Show when=move || has_selection.get()>
                <div class="batch-bar" role="toolbar" aria-label="批量操作">
                    <span class="batch-bar-count">
                        {move || format!("已选 {} 篇", selection_count.get())}
                    </span>
                    <button
                        class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                        type="button"
                        disabled=move || batch_busy.get()
                        on:click=move |_| restore(selected.get_untracked().into_iter().collect())
                    >
                        "批量恢复"
                    </button>
                    <button
                        class=format!(
                            "{} batch-bar-danger",
                            button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                        )
                        type="button"
                        disabled=move || batch_busy.get()
                        on:click=move |_| {
                            ask_purge(
                                PurgeTarget::Selected(selected.get_untracked().into_iter().collect()),
                            )
                        }
                    >
                        "彻底删除"
                    </button>
                    <span class="batch-bar-sep" aria-hidden="true"></span>
                    <IconButton
                        icon=IconName::X
                        label="取消选择"
                        size=ButtonSize::Sm
                        onclick=move |_| selected.update(|s| s.clear())
                    />
                </div>
            </Show>

            <ConfirmDialog
                visible=confirm_open
                title="彻底删除文章？"
                description=confirm_description
                confirm_label="彻底删除"
                on_confirm=purge
            />
            <ConfirmDialog
                visible=confirm_all_open
                title="清空回收站？"
                description=confirm_description
                confirm_label="彻底删除"
                on_confirm=purge
            />
        </div>
    }
}

/// 标题行：保留期说明与「清空回收站」。回收站为空时不渲染操作区，故按「有无文章」切换两种结构。
#[component]
fn TrashHead(
    settings: RwSignal<Option<TrashSettings>>,
    total: RwSignal<i64>,
    batch_busy: RwSignal<bool>,
    #[prop(into)] on_empty: Callback<()>,
) -> impl IntoView {
    let has_items = Memo::new(move |_| total.get() > 0);
    // 说明文字在两种结构里相同。
    let note = move || {
        view! {
            {move || {
                settings
                    .with(|s| {
                        s.as_ref()
                            .filter(|s| s.auto_purge_enabled)
                            .map(|s| format!("已删除的文章保留 {} 天，", s.retention_days))
                    })
            }}
            {move || settings.with(|s| retention_note(s.as_ref()))}
            {move || {
                settings
                    .with(Option::is_some)
                    .then(|| {
                        view! {
                            <a
                                class="admin-trash-settings"
                                href=Route::SiteSettingsPage.to_string()
                            >
                                "可在设置中修改"
                            </a>
                            "。"
                        }
                    })
            }}
        }
    };
    move || {
        if has_items.get() {
            view! {
                <AdminPageHead
                    title="回收站"
                    actions=move || {
                        view! {
                            <button
                                class=format!(
                                    "{} admin-trash-empty",
                                    button_class(ButtonVariant::Outline, ButtonSize::Md),
                                )
                                type="button"
                                disabled=move || batch_busy.get()
                                on:click=move |_| on_empty.run(())
                            >
                                "清空回收站"
                            </button>
                        }
                    }
                >
                    {note()}
                </AdminPageHead>
            }
            .into_any()
        } else {
            view! { <AdminPageHead title="回收站">{note()}</AdminPageHead> }.into_any()
        }
    }
}

/// 表格的一行。`left` 为（剩余天数，保留天数），自动清理关闭时为 None。
#[component]
fn TrashRow(
    post: PostListItem,
    #[prop(into)] selected: Signal<bool>,
    #[prop(into)] busy: Signal<bool>,
    left: Option<(i64, i32)>,
    #[prop(into)] on_select: Callback<(i32, bool)>,
    #[prop(into)] on_restore: Callback<i32>,
    #[prop(into)] on_purge: Callback<(i32, String)>,
) -> impl IntoView {
    let deleted = post
        .deleted_at
        .map(|d| to_local(d).format("%Y-%m-%d").to_string());
    let id = post.id;
    let title = post.title.clone();
    let checkbox_label = format!("选择《{}》", post.title);
    let deleted_cell = match deleted {
        Some(deleted) => view! { <time>{deleted}</time> }.into_any(),
        None => view! { <span class="admin-table-muted">"—"</span> }.into_any(),
    };
    let left_cell = left.map(|(days, retention)| {
        let width = (days as f64 / retention.max(1) as f64 * 100.0).min(100.0);
        view! {
            <td class="admin-trash-left">
                <div class="trash-left" data-urgent=(days <= URGENT_DAYS).then_some("true")>
                    <span class="trash-left-track" aria-hidden="true">
                        <span class="trash-left-bar" style=format!("width: {width:.0}%")></span>
                    </span>
                    <span class="trash-left-label">
                        {if days == 0 { "即将清理".to_string() } else { format!("剩 {days} 天") }}
                    </span>
                </div>
            </td>
        }
    });
    view! {
        <tr
            data-selected=move || selected.get().then_some("true")
            aria-busy=move || busy.get().then_some("true")
        >
            <td class="admin-table-check">
                <Checkbox
                    checked=selected
                    label=checkbox_label
                    onchange=move |checked: bool| on_select.run((id, checked))
                />
            </td>
            <td class="admin-trash-title">
                <span class="admin-trash-name">{post.title}</span>
                <span class="admin-posts-slug">{format!("/{}", post.slug)}</span>
            </td>
            <td class="admin-trash-status">
                <StatusDot label=post.status.label() tone=post.status.tone()/>
            </td>
            <td class="admin-trash-deleted">{deleted_cell}</td>
            {left_cell}
            <td class="admin-trash-actions">
                <div>
                    <button
                        class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                        type="button"
                        disabled=move || busy.get()
                        on:click=move |_| on_restore.run(id)
                    >
                        "恢复"
                    </button>
                    <IconButton
                        icon=IconName::Trash
                        label="彻底删除"
                        size=ButtonSize::Sm
                        disabled=busy
                        tooltip_align="end"
                        class="admin-trash-purge"
                        onclick=move |_| on_purge.run((id, title.clone()))
                    />
                </div>
            </td>
        </tr>
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
        assert!(PurgeTarget::All(4).description().starts_with("4 篇文章"));
        assert!(PurgeTarget::Selected(vec![1, 2])
            .description()
            .starts_with("选中的 2 篇"));
    }
}
