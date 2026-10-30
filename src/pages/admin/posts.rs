//! 文章列表（`/admin/posts`）。
//!
//! 标题行（篇数、重建渲染菜单、新建文章）→ 状态选项卡（「回收站」进入独立路由
//! `/admin/posts/trash`）、标题搜索与标签筛选 → 表格（勾选、标题与 slug、状态、标签、
//! 字数、发布时间、「⋯」菜单）与底栏分页。勾选后底部浮起批量操作条。手机端表格行
//! 改为卡片，主操作为右下角浮动按钮。筛选、页码与搜索框内容按历史记录保存，从预览
//! 返回时恢复。数据仅在 WASM 前端通过 server function 加载（后台数据是私有的，
//! SSR 不取数）。样式见 styles/pages/admin-posts.css。

use std::collections::HashSet;

use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::posts::{
    batch_posts, delete_post, list_posts, list_tags, rebuild_content_html,
    rebuild_post_content_html, AdminPostQuery, BatchPostAction, PostListResponse,
};
use crate::components::admin_layout::AdminPageHead;
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::forms::{FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::posts_skeleton::PostsTableSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Checkbox, Icon, IconButton, IconName,
    LoadError, Menu, MenuItem, SegmentedControl, SegmentedItem, StatusDot, TablePager,
};
use crate::hooks::query::use_paginated;
use crate::models::post::{PostListItem, PostStats, PostStatus};
use crate::routes::Route;
use crate::utils::error::failure_message;
use crate::utils::time::{hm, to_local, ymd};

/// 每页展示的文章数量。
const POSTS_PER_PAGE: i32 = 20;
/// 输入停顿多久后自动搜索（毫秒）。
const SEARCH_DEBOUNCE_MS: u32 = 300;

/// `/admin/posts` 路由：纯客户端数据，`data()` 不取数。
pub struct PostsRoute;

#[lazy_route]
impl LazyRoute for PostsRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <Posts/> }.into_any()
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct PostsHistoryState {
    page: i32,
    status: String,
    search_input: String,
    search_query: String,
    #[serde(default)]
    tag: String,
}

impl Default for PostsHistoryState {
    fn default() -> Self {
        Self {
            page: 1,
            status: "all".to_string(),
            search_input: String::new(),
            search_query: String::new(),
            tag: String::new(),
        }
    }
}

/// 状态选项卡的值转成查询条件。
fn status_filter(value: &str) -> Option<PostStatus> {
    match value {
        "published" => Some(PostStatus::Published),
        "draft" => Some(PostStatus::Draft),
        _ => None,
    }
}

/// 标签列：前两个用「·」连接，其余折成「+N」。
fn tag_summary(tags: &[String]) -> (String, Option<String>) {
    let shown = tags.iter().take(2).cloned().collect::<Vec<_>>().join(" · ");
    let more = (tags.len() > 2).then(|| format!("+{}", tags.len() - 2));
    (shown, more)
}

/// 表头勾选框：全部勾选时为选中；点它在「全选本页」与「全不选」之间切换。
fn all_selected(rows: &[PostListItem], selected: &HashSet<i32>) -> bool {
    !rows.is_empty() && rows.iter().all(|p| selected.contains(&p.id))
}

/// 列表区域的四种互斥状态。
#[derive(Clone, Copy, PartialEq)]
enum ListState {
    Error,
    Loading,
    Empty,
    Table,
}

/// 空列表时的三种文案。
#[derive(Clone, Copy, PartialEq)]
enum EmptyKind {
    Filtered,
    Drafts,
    NoPosts,
}

/// 文章列表页。
#[component]
fn Posts() -> impl IntoView {
    let stats = RwSignal::new(None::<PostStats>);
    let stats_version = RwSignal::new(0_u32);
    // 请求世代号：连续刷新篇数时只有最后一次响应落地。
    let stats_generation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        stats_version.track();
        let Some(id) = stats_generation.try_update_value(|g| {
            *g = g.wrapping_add(1);
            *g
        }) else {
            return;
        };
        spawn_local(async move {
            if let Ok(res) = crate::api::posts::get_post_stats().await {
                if stats_generation.try_get_value() == Some(id) {
                    stats.try_set(Some(res.stats));
                }
            }
        });
    });
    let summary = Memo::new(move |_| {
        stats.get().map(|s| {
            let mut parts = vec![format!("{} 篇已发布", s.published)];
            if s.drafts > 0 {
                parts.push(format!("草稿 {}", s.drafts));
            }
            if s.trash > 0 {
                parts.push(format!("回收站 {}", s.trash));
            }
            parts.join(" · ")
        })
    });

    view! {
        <div class="admin-posts">
            <AdminPageHead
                title="文章"
                actions=move || {
                    view! {
                        <RebuildMenu/>
                        <a
                            class=format!(
                                "{} admin-hide-mobile",
                                button_class(ButtonVariant::Primary, ButtonSize::Md),
                            )
                            href=Route::Write.to_string()
                        >
                            <Icon name=IconName::Plus size=16/>
                            "新建文章"
                        </a>
                    }
                }
            >
                {move || summary.get()}
            </AdminPageHead>
            <PostsTable on_changed=move |_: ()| stats_version.update(|n| *n += 1)/>
            <a class="admin-fab" href=Route::Write.to_string() aria-label="写文章">
                <Icon name=IconName::Plus size=18/>
                "写文章"
            </a>
        </div>
    }
}

/// 「重建渲染」菜单：只补空缺的渲染，或全部重建（渲染规则升级后用）。结果用提示条告知。
#[component]
fn RebuildMenu() -> impl IntoView {
    let toast = use_toast();
    let busy = RwSignal::new(false);
    let run = move |rebuild_all: bool| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            // 服务端每批只处理一部分，按 next_after_id 游标续传直到处理完。
            let mut after_id = 0_i32;
            let mut rebuilt = 0_u64;
            let mut failed = 0_u64;
            let mut first_error: Option<String> = None;
            let mut interrupted = None;
            loop {
                match rebuild_content_html(rebuild_all, after_id).await {
                    Ok(res) => {
                        rebuilt += res.rebuilt;
                        failed += res.failed;
                        if first_error.is_none() {
                            first_error = res.errors.into_iter().next();
                        }
                        match res.next_after_id {
                            Some(next) => after_id = next,
                            None => break,
                        }
                    }
                    Err(e) => {
                        interrupted = Some(e);
                        break;
                    }
                }
            }
            if let Some(e) = interrupted {
                let prefix = if rebuilt > 0 {
                    format!("已重建 {rebuilt} 篇后中断")
                } else {
                    "重建失败".to_string()
                };
                toast.error(failure_message(&prefix, &e));
            } else if failed > 0 {
                toast.error(format!(
                    "已重建 {rebuilt} 篇，失败 {failed} 篇{}",
                    first_error.map(|e| format!("：{e}")).unwrap_or_default()
                ));
            } else {
                toast.success(format!("已重建 {rebuilt} 篇文章的渲染"));
            }
            busy.try_set(false);
        });
    };
    // Menu 的 label 不是响应式 prop：忙碌态变化时按新文案重建菜单。
    view! {
        {move || {
            let label = if busy.get() { "正在重建渲染…" } else { "更多操作" };
            view! {
                <Menu
                    label=label
                    size=ButtonSize::Md
                    items=vec![
                        MenuItem::new("missing", "补全缺失的渲染").icon(IconName::RefreshCw),
                        MenuItem::new("all", "重建全部渲染").icon(IconName::RotateCw),
                    ]
                    on_select=move |id: &'static str| run(id == "all")
                />
            }
        }}
    }
}

/// 文章表格：筛选、勾选、批量操作与分页。`on_changed` 在增删改后通知页头刷新篇数。
#[component]
fn PostsTable(#[prop(into)] on_changed: Callback<()>) -> impl IntoView {
    let entry_id = crate::bridges::navigation::entry_id();
    let saved = crate::bridges::navigation::read_state::<PostsHistoryState>("admin-posts")
        .unwrap_or_default();
    let toast = use_toast();
    let current_page = RwSignal::new(saved.page.max(1));
    let status = RwSignal::new(saved.status.clone());
    let tag = RwSignal::new(saved.tag.clone());
    // 输入框内容与已提交的搜索词分开：停顿 300ms 或回车才提交。
    let search_input = RwSignal::new(saved.search_input.clone());
    let search_query = RwSignal::new(saved.search_query.clone());
    let search_seq = RwSignal::new(0_u64);
    let selected = RwSignal::new(HashSet::<i32>::new());
    let busy = RwSignal::new(HashSet::<i32>::new());
    let batch_busy = RwSignal::new(false);
    let tags = RwSignal::new(Vec::<String>::new());
    // 原地重新请求当前页（批量操作或重试后）。
    let refresh = RwSignal::new(0_u32);

    Effect::new(move |_| {
        crate::bridges::navigation::write_state(
            &entry_id,
            "admin-posts",
            &PostsHistoryState {
                page: current_page.get(),
                status: status.get(),
                search_input: search_input.get(),
                search_query: search_query.get(),
                tag: tag.get(),
            },
        );
    });
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(res) = list_tags().await {
                tags.try_set(res.tags.into_iter().map(|t| t.name).collect());
            }
        });
    });

    let paginated = use_paginated(
        move || {
            search_query.track();
            status.track();
            tag.track();
            refresh.track();
            current_page.get()
        },
        POSTS_PER_PAGE,
        move |p, pp| {
            let query = AdminPostQuery {
                search: Some(search_query.get()).filter(|q| !q.is_empty()),
                status: status_filter(&status.get()),
                tag: Some(tag.get()).filter(|t| !t.is_empty()),
            };
            async move {
                list_posts(p, pp, query)
                    .await
                    .map(|PostListResponse { posts, total }| (posts, total))
                    .map_err(|e| failure_message("文章列表加载失败", &e))
            }
        },
    );
    let posts = paginated.items;
    let total = paginated.total;
    let loading = paginated.loading;
    let error = paginated.error;
    let reload = move || {
        refresh.try_update(|n| *n += 1);
    };

    // 换筛选条件时清掉勾选，回到第 1 页。
    let reset_view = move || {
        selected.update(|s| s.clear());
        current_page.set(1);
    };
    let submit_search = move || {
        let q = search_input.get_untracked().trim().to_string();
        if q != search_query.get_untracked() {
            search_query.set(q);
            reset_view();
        }
    };
    let schedule_search = move || {
        search_seq.update(|s| *s += 1);
        let id = search_seq.get_untracked();
        spawn_local(async move {
            crate::utils::time::sleep_ms(SEARCH_DEBOUNCE_MS).await;
            if search_seq.try_get_untracked() == Some(id) {
                submit_search();
            }
        });
    };

    let run_batch = move |action: BatchPostAction| {
        let ids: Vec<i32> = selected.get_untracked().into_iter().collect();
        if ids.is_empty() || batch_busy.get_untracked() {
            return;
        }
        batch_busy.set(true);
        spawn_local(async move {
            match batch_posts(ids, action).await {
                Ok(res) => {
                    let verb = match action {
                        BatchPostAction::Publish => "发布",
                        BatchPostAction::Unpublish => "转为草稿",
                        BatchPostAction::Trash => "移到回收站",
                    };
                    if res.failed > 0 {
                        toast.error(format!("已{verb} {} 篇，{} 篇失败", res.done, res.failed));
                    } else {
                        toast.success(format!("已{verb} {} 篇", res.done));
                    }
                    selected.try_update(|s| s.clear());
                    reload();
                    on_changed.try_run(());
                }
                Err(e) => toast.error(failure_message("操作失败", &e)),
            }
            batch_busy.try_set(false);
        });
    };

    let on_row_action =
        Callback::new(
            move |(post, action): (PostListItem, &'static str)| match action {
                "edit" => {
                    crate::navigation::push(&Route::WriteEdit { id: post.id }.to_string());
                }
                "preview" => {
                    crate::navigation::push(&Route::PostPreview { slug: post.slug }.to_string());
                }
                "copy" => {
                    #[cfg(target_arch = "wasm32")]
                    spawn_local(async move {
                        let origin = web_sys::window()
                            .and_then(|w| w.location().origin().ok())
                            .unwrap_or_default();
                        if crate::utils::js::copy_text(&format!("{origin}/post/{}", post.slug))
                            .await
                        {
                            toast.success("已复制文章链接");
                        } else {
                            toast.error("复制失败，请手动复制地址栏中的链接");
                        }
                    });
                }
                "rebuild" => {
                    busy.update(|b| _ = b.insert(post.id));
                    spawn_local(async move {
                        match rebuild_post_content_html(post.id).await {
                            Ok(res) if res.success => {
                                toast.success(format!("已重建《{}》的渲染", post.title))
                            }
                            Ok(res) => toast.error(res.message),
                            Err(e) => toast.error(failure_message("重建失败", &e)),
                        }
                        busy.try_update(|b| _ = b.remove(&post.id));
                    });
                }
                "trash" => {
                    busy.update(|b| _ = b.insert(post.id));
                    spawn_local(async move {
                        match delete_post(post.id).await {
                            Ok(res) if res.success => {
                                posts.try_update(|list| list.retain(|p| p.id != post.id));
                                total.try_update(|t| *t = t.saturating_sub(1));
                                selected.try_update(|s| _ = s.remove(&post.id));
                                toast.success(format!("已把《{}》移到回收站", post.title));
                                on_changed.try_run(());
                            }
                            Ok(res) => toast.error(res.message),
                            Err(e) => toast.error(failure_message("移到回收站失败", &e)),
                        }
                        busy.try_update(|b| _ = b.remove(&post.id));
                    });
                }
                _ => {}
            },
        );
    let on_row_select = Callback::new(move |(id, checked): (i32, bool)| {
        selected.update(|s| {
            if checked {
                s.insert(id);
            } else {
                s.remove(&id);
            }
        });
    });

    let page_all = Memo::new(move |_| {
        posts.with(|rows| selected.with(|selection| all_selected(rows, selection)))
    });
    let filtering =
        Memo::new(move |_| !search_query.with(|q| q.is_empty()) || !tag.with(|t| t.is_empty()));
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
    let empty_kind = Memo::new(move |_| {
        if filtering.get() {
            EmptyKind::Filtered
        } else if status.with(|s| s == "draft") {
            EmptyKind::Drafts
        } else {
            EmptyKind::NoPosts
        }
    });
    let tag_options = Signal::derive(move || {
        let mut options: Vec<(String, String)> = vec![(String::new(), "全部标签".to_string())];
        options.extend(tags.get().into_iter().map(|t| (t.clone(), t)));
        options
    });
    let selection_count = Memo::new(move |_| selected.with(HashSet::len));
    let has_selection = Memo::new(move |_| selection_count.get() > 0);

    view! {
        {move || {
            (!loading.get() || error.with(Option::is_some))
                .then(|| view! { <span hidden data-vt-list="true"></span> })
        }}
        <div class="admin-toolbar">
            <SegmentedControl
                aria_label="文章状态"
                items=vec![
                    SegmentedItem::new("all".to_string(), "全部"),
                    SegmentedItem::new("published".to_string(), "已发布"),
                    SegmentedItem::new("draft".to_string(), "草稿"),
                    SegmentedItem::new("trash".to_string(), "回收站"),
                ]
                value=status
                on_change=move |value: String| {
                    if value == "trash" {
                        crate::navigation::push(&Route::PostsTrash.to_string());
                    } else {
                        status.set(value);
                        reset_view();
                    }
                }
            />
            <div class="admin-toolbar-filters">
                <label class="field field-sm admin-filter-search">
                    <Icon name=IconName::Search size=14/>
                    <span class="sr-only">"按标题搜索"</span>
                    <input
                        type="search"
                        placeholder="按标题搜索"
                        prop:value=move || search_input.get()
                        on:input=move |e| {
                            search_input.set(event_target_value(&e));
                            schedule_search();
                        }
                        on:keydown=move |e: KeyboardEvent| {
                            let key = e.key();
                            if key == "Enter" && !e.is_composing() {
                                search_seq.update(|s| *s += 1);
                                submit_search();
                            } else if key == "Escape"
                                && search_input.with_untracked(|s| !s.is_empty())
                            {
                                e.prevent_default();
                                search_input.set(String::new());
                                search_seq.update(|s| *s += 1);
                                submit_search();
                            }
                        }
                    />
                </label>
                <FormSelect
                    id="admin-posts-tag"
                    value=tag
                    options=tag_options
                    trigger_class=FORM_SELECT_COMPACT_CLASS
                    aria_label="按标签筛选"
                    onchange=move |value: String| {
                        tag.set(value);
                        reset_view();
                    }
                />
            </div>
        </div>

        {move || match list_state.get() {
            ListState::Error => {
                view! { <LoadError title="文章列表加载失败" on_retry=move |_: ()| reload()/> }
                    .into_any()
            }
            ListState::Loading => {
                view! {
                    <DelayedSkeleton>
                        <PostsTableSkeleton/>
                    </DelayedSkeleton>
                }
                    .into_any()
            }
            ListState::Empty => {
                view! {
                    <div class="admin-panel">
                        {move || match empty_kind.get() {
                            EmptyKind::Filtered => {
                                view! {
                                    <EmptyState
                                        icon=IconName::SearchX
                                        title="没有符合条件的文章"
                                        description="换个标题关键词，或清除标签筛选再试。"
                                        action=EmptyStateAction::new(
                                            "清除筛选",
                                            move |_| {
                                                search_input.set(String::new());
                                                search_query.set(String::new());
                                                tag.set(String::new());
                                                reset_view();
                                            },
                                        )
                                    />
                                }
                                    .into_any()
                            }
                            EmptyKind::Drafts => {
                                view! {
                                    <EmptyState
                                        title="没有草稿"
                                        description="未发布的文章会出现在这里。"
                                    />
                                }
                                    .into_any()
                            }
                            EmptyKind::NoPosts => {
                                view! {
                                    <EmptyState
                                        title="还没有文章"
                                        description="写下第一篇，它会出现在这里。"
                                        action=EmptyStateAction::new(
                                            "写文章",
                                            move |_| {
                                                crate::navigation::push(&Route::Write.to_string());
                                            },
                                        )
                                    />
                                }
                                    .into_any()
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
                        <table class="admin-table admin-posts-table">
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
                                    <th>"状态"</th>
                                    <th class="admin-posts-tags">"标签"</th>
                                    <th class="admin-table-num admin-posts-words">"字数"</th>
                                    <th>"发布时间"</th>
                                    <th class="admin-table-actions">
                                        <span class="sr-only">"操作"</span>
                                    </th>
                                </tr>
                            </thead>
                            <tbody>
                                {move || {
                                    posts
                                        .get()
                                        .into_iter()
                                        .map(|post| {
                                            let id = post.id;
                                            view! {
                                                <PostRow
                                                    selected=Signal::derive(move || {
                                                        selected.with(|s| s.contains(&id))
                                                    })
                                                    busy=Signal::derive(move || busy.with(|b| b.contains(&id)))
                                                    on_select=on_row_select
                                                    on_action=on_row_action
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
                            per_page=POSTS_PER_PAGE
                            unit="篇"
                            on_change=move |page: i32| {
                                current_page.set(page);
                                #[cfg(target_arch = "wasm32")]
                                if let Some(scroller) = web_sys::window()
                                    .and_then(|w| w.document())
                                    .and_then(|d| d.query_selector("[data-vt-scroll]").ok().flatten())
                                {
                                    scroller.set_scroll_top(0);
                                }
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
                    on:click=move |_| run_batch(BatchPostAction::Unpublish)
                >
                    "转为草稿"
                </button>
                <button
                    class=format!(
                        "{} batch-bar-danger",
                        button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                    )
                    type="button"
                    disabled=move || batch_busy.get()
                    on:click=move |_| run_batch(BatchPostAction::Trash)
                >
                    "移到回收站"
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
    }
}

/// 表格的一行。标题进入后台预览（保留列表来源与侧栏）；「⋯」菜单回传操作 id。
#[component]
fn PostRow(
    post: PostListItem,
    #[prop(into)] selected: Signal<bool>,
    #[prop(into)] busy: Signal<bool>,
    #[prop(into)] on_select: Callback<(i32, bool)>,
    #[prop(into)] on_action: Callback<(PostListItem, &'static str)>,
) -> impl IntoView {
    let published = post.status == PostStatus::Published;
    let date = post.published_at.map(|d| {
        let t = to_local(d);
        format!("{} {}", ymd(&t, '-'), hm(&t))
    });
    let (tags, more) = tag_summary(&post.tags);
    let mut copy = MenuItem::new("copy", "复制链接").icon(IconName::Link);
    if !published {
        copy = copy.disabled("草稿还没有公开地址");
    }
    let items = vec![
        MenuItem::new("edit", "编辑").icon(IconName::Pencil),
        MenuItem::new("preview", "预览").icon(IconName::Eye),
        copy,
        MenuItem::new("rebuild", "重建渲染").icon(IconName::RefreshCw),
        MenuItem::new("trash", "移到回收站")
            .icon(IconName::Trash)
            .danger()
            .separated(),
    ];
    let id = post.id;
    let menu_post = post.clone();
    let checkbox_label = format!("选择《{}》", post.title);
    let preview_href = Route::PostPreview {
        slug: post.slug.clone(),
    }
    .to_string();
    let tags_cell = if tags.is_empty() {
        view! { <span class="admin-table-muted">"—"</span> }.into_any()
    } else {
        view! {
            {tags}
            {more.map(|more| view! { <span class="admin-table-muted">{format!(" {more}")}</span> })}
        }
        .into_any()
    };
    let date_cell = match date {
        Some(date) => view! { <time>{date}</time> }.into_any(),
        None => view! { <span class="admin-table-muted">"未发布"</span> }.into_any(),
    };
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
            <td class="admin-posts-title">
                <a
                    href=preview_href
                    data-vt-post-link=id.to_string()
                    data-vt-post-id=id.to_string()
                    data-vt-role="title"
                >
                    {post.title}
                </a>
                <span class="admin-posts-slug">{format!("/{}", post.slug)}</span>
            </td>
            <td class="admin-posts-status">
                <StatusDot label=post.status.label() tone=post.status.tone()/>
            </td>
            <td class="admin-posts-tags">{tags_cell}</td>
            <td class="admin-table-num admin-posts-words">{post.word_count}</td>
            <td class="admin-posts-date">{date_cell}</td>
            <td class="admin-table-actions">
                <Menu
                    label="更多操作"
                    items=items
                    on_select=move |action: &'static str| {
                        on_action.run((menu_post.clone(), action))
                    }
                />
            </td>
        </tr>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_summary_folds_extra_tags() {
        let tags: Vec<String> = ["Rust", "架构", "Leptos"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            tag_summary(&tags),
            ("Rust · 架构".to_string(), Some("+1".to_string()))
        );
        assert_eq!(tag_summary(&tags[..1]), ("Rust".to_string(), None));
        assert_eq!(tag_summary(&[]), (String::new(), None));
    }

    #[test]
    fn status_filter_maps_tabs() {
        assert_eq!(status_filter("draft"), Some(PostStatus::Draft));
        assert_eq!(status_filter("published"), Some(PostStatus::Published));
        assert_eq!(status_filter("all"), None);
    }
}
