//! 文章列表（`/admin/posts`）。
//!
//! 标题行（篇数、重建渲染菜单、新建文章）→ 状态选项卡（「回收站」进入独立路由
//! `/admin/posts/trash`）、标题搜索与标签筛选 → 表格（勾选、标题与 slug、状态、标签、
//! 字数、发布时间、「⋯」菜单）与底栏分页。勾选后底部浮起批量操作条。手机端表格行
//! 改为卡片，主操作为右下角浮动按钮。筛选、页码与搜索框内容按历史记录保存，从预览
//! 返回时恢复。数据仅在 WASM 前端通过 server function 加载。样式见
//! styles/pages/admin-posts.css。

use std::collections::HashSet;

use crate::components::admin_layout::AdminPageHead;
use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::api::posts::{
    batch_posts, delete_post, list_posts, rebuild_content_html, rebuild_post_content_html,
    AdminPostQuery, BatchPostAction, PostListResponse,
};
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
use crate::router::Route;
use crate::utils::error::failure_message;
use crate::utils::time::to_local;

/// 每页展示的文章数量。
const POSTS_PER_PAGE: i32 = 20;
/// 输入停顿多久后自动搜索（毫秒）。
const SEARCH_DEBOUNCE_MS: u32 = 300;

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

/// 文章列表页。
#[component]
pub fn Posts() -> Element {
    let mut stats = use_signal(|| None::<PostStats>);
    let mut stats_version = use_signal(|| 0_u32);
    use_effect(move || {
        let _ = stats_version();
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            if let Ok(res) = crate::api::posts::get_post_stats().await {
                stats.set(Some(res.stats));
            }
        });
    });
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &mut stats;
    let summary = stats().map(|s| {
        let mut parts = vec![format!("{} 篇已发布", s.published)];
        if s.drafts > 0 {
            parts.push(format!("草稿 {}", s.drafts));
        }
        if s.trash > 0 {
            parts.push(format!("回收站 {}", s.trash));
        }
        parts.join(" · ")
    });

    rsx! {
        div { class: "admin-posts",
            AdminPageHead {
                title: "文章",
                actions: rsx! {
                    RebuildMenu {}
                    Link {
                        class: "{button_class(ButtonVariant::Primary, ButtonSize::Md)} admin-hide-mobile",
                        to: Route::Write {},
                        Icon { name: IconName::Plus, size: 16 }
                        "新建文章"
                    }
                },
                if let Some(summary) = summary {
                    "{summary}"
                }
            }
            PostsTable { on_changed: move |_| stats_version += 1 }
            Link { class: "admin-fab", to: Route::Write {}, aria_label: "写文章",
                Icon { name: IconName::Plus, size: 18 }
                "写文章"
            }
        }
    }
}

/// 「重建渲染」菜单：只补空缺的渲染，或全部重建（渲染规则升级后用）。结果用提示条告知。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn RebuildMenu() -> Element {
    let toast = use_toast();
    let mut busy = use_signal(|| false);
    let mut run = move |rebuild_all: bool| {
        if busy() {
            return;
        }
        busy.set(true);
        spawn(async move {
            match rebuild_content_html(rebuild_all).await {
                Ok(res) if res.failed > 0 => toast.error(format!(
                    "已重建 {} 篇，失败 {} 篇{}",
                    res.rebuilt,
                    res.failed,
                    res.errors
                        .first()
                        .map(|e| format!("：{e}"))
                        .unwrap_or_default()
                )),
                Ok(res) => toast.success(format!("已重建 {} 篇文章的渲染", res.rebuilt)),
                Err(e) => toast.error(failure_message("重建失败", &e)),
            }
            busy.set(false);
        });
    };
    rsx! {
        Menu {
            label: if busy() { "正在重建渲染…" } else { "更多操作" },
            size: ButtonSize::Md,
            items: vec![
                MenuItem::new("missing", "补全缺失的渲染").icon(IconName::RefreshCw),
                MenuItem::new("all", "重建全部渲染").icon(IconName::RotateCw),
            ],
            on_select: move |id: &'static str| run(id == "all"),
        }
    }
}

/// 文章表格：筛选、勾选、批量操作与分页。`on_changed` 在增删改后通知页头刷新篇数。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn PostsTable(on_changed: EventHandler<()>) -> Element {
    let entry_id = use_hook(crate::bridges::navigation::entry_id);
    let saved = use_hook(|| {
        crate::bridges::navigation::read_state::<PostsHistoryState>("admin-posts")
            .unwrap_or_default()
    });
    let navigator = use_navigator();
    let toast = use_toast();
    let mut current_page = use_signal(|| saved.page.max(1));
    let mut status = use_signal(|| saved.status.clone());
    let mut tag = use_signal(|| saved.tag.clone());
    // 输入框内容与已提交的搜索词分开：停顿 300ms 或回车才提交。
    let mut search_input = use_signal(|| saved.search_input.clone());
    let mut search_query = use_signal(|| saved.search_query.clone());
    let mut search_seq = use_signal(|| 0_u64);
    let mut selected = use_signal(HashSet::<i32>::new);
    let mut busy = use_signal(HashSet::<i32>::new);
    let mut batch_busy = use_signal(|| false);
    let mut tags = use_signal(Vec::<String>::new);
    // 原地重新请求当前页（批量操作或重试后）。
    let mut refresh = use_signal(|| 0_u32);

    use_effect(move || {
        crate::bridges::navigation::write_state(
            &entry_id,
            "admin-posts",
            &PostsHistoryState {
                page: current_page(),
                status: status(),
                search_input: search_input(),
                search_query: search_query(),
                tag: tag(),
            },
        );
    });
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            if let Ok(res) = crate::api::posts::list_tags().await {
                tags.set(res.tags.into_iter().map(|t| t.name).collect());
            }
        });
    });

    let paginated = use_paginated(
        move || {
            let _ = (search_query(), status(), tag(), refresh());
            current_page.with(|p| *p)
        },
        POSTS_PER_PAGE,
        move |p, pp| {
            let query = AdminPostQuery {
                search: Some(search_query()).filter(|q| !q.is_empty()),
                status: status_filter(&status()),
                tag: Some(tag()).filter(|t| !t.is_empty()),
            };
            async move {
                list_posts(p, pp, query)
                    .await
                    .map(|PostListResponse { posts, total }| (posts, total))
                    .map_err(|e| e.to_string())
            }
        },
    );
    let mut posts = paginated.items;
    let mut total = paginated.total;
    let loading = paginated.loading;
    let error = paginated.error;
    let mut reload = move || refresh += 1;

    // 换筛选条件时清掉勾选，回到第 1 页。
    let mut reset_view = move || {
        selected.write().clear();
        current_page.set(1);
    };
    let mut submit_search = move || {
        let q = search_input.peek().trim().to_string();
        if q != *search_query.peek() {
            search_query.set(q);
            reset_view();
        }
    };
    let mut schedule_search = move || {
        search_seq += 1;
        let id = search_seq();
        spawn(async move {
            crate::utils::time::sleep_ms(SEARCH_DEBOUNCE_MS).await;
            if search_seq() == id {
                submit_search();
            }
        });
    };

    let mut run_batch = move |action: BatchPostAction| {
        let ids: Vec<i32> = selected().into_iter().collect();
        if ids.is_empty() || batch_busy() {
            return;
        }
        batch_busy.set(true);
        spawn(async move {
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
                    selected.write().clear();
                    reload();
                    on_changed.call(());
                }
                Err(e) => toast.error(failure_message("操作失败", &e)),
            }
            batch_busy.set(false);
        });
    };

    let on_row_action = move |(post, action): (PostListItem, &'static str)| match action {
        "edit" => {
            navigator.push(Route::WriteEdit { id: post.id });
        }
        "preview" => {
            navigator.push(Route::PostPreview { slug: post.slug });
        }
        "copy" => {
            #[cfg(target_arch = "wasm32")]
            spawn(async move {
                let origin = web_sys::window()
                    .and_then(|w| w.location().origin().ok())
                    .unwrap_or_default();
                if crate::utils::js::copy_text(&format!("{origin}/post/{}", post.slug)).await {
                    toast.success("已复制文章链接");
                } else {
                    toast.error("复制失败，请手动复制地址栏中的链接");
                }
            });
        }
        "rebuild" => {
            busy.write().insert(post.id);
            spawn(async move {
                match rebuild_post_content_html(post.id).await {
                    Ok(res) if res.success => {
                        toast.success(format!("已重建《{}》的渲染", post.title))
                    }
                    Ok(res) => toast.error(res.message),
                    Err(e) => toast.error(failure_message("重建失败", &e)),
                }
                busy.write().remove(&post.id);
            });
        }
        "trash" => {
            busy.write().insert(post.id);
            spawn(async move {
                match delete_post(post.id).await {
                    Ok(res) if res.success => {
                        posts.with_mut(|list| list.retain(|p| p.id != post.id));
                        total.with_mut(|t| *t = t.saturating_sub(1));
                        selected.write().remove(&post.id);
                        toast.success(format!("已把《{}》移到回收站", post.title));
                        on_changed.call(());
                    }
                    Ok(res) => toast.error(res.message),
                    Err(e) => toast.error(failure_message("移到回收站失败", &e)),
                }
                busy.write().remove(&post.id);
            });
        }
        _ => {}
    };

    let rows = posts();
    let selection = selected();
    let page_all = all_selected(&rows, &selection);
    let filtering = !search_query().is_empty() || !tag().is_empty();
    let mut tag_options: Vec<(String, String)> = vec![(String::new(), "全部标签".to_string())];
    tag_options.extend(tags().into_iter().map(|t| (t.clone(), t)));

    rsx! {
        if !loading() || error().is_some() {
            span { hidden: true, "data-vt-list": "true" }
        }
        div { class: "admin-toolbar",
            SegmentedControl {
                aria_label: "文章状态",
                items: vec![
                    SegmentedItem::new("all".to_string(), "全部"),
                    SegmentedItem::new("published".to_string(), "已发布"),
                    SegmentedItem::new("draft".to_string(), "草稿"),
                    SegmentedItem::new("trash".to_string(), "回收站"),
                ],
                value: status(),
                on_change: move |value: String| {
                    if value == "trash" {
                        navigator.push(Route::PostsTrash {});
                    } else {
                        status.set(value);
                        reset_view();
                    }
                },
            }
            div { class: "admin-toolbar-filters",
                label { class: "field field-sm admin-filter-search",
                    Icon { name: IconName::Search, size: 14 }
                    span { class: "sr-only", "按标题搜索" }
                    input {
                        r#type: "search",
                        placeholder: "按标题搜索",
                        value: "{search_input}",
                        oninput: move |e| {
                            search_input.set(e.value());
                            schedule_search();
                        },
                        onkeydown: move |e| {
                            if e.key() == Key::Enter && !e.data().is_composing() {
                                search_seq += 1;
                                submit_search();
                            } else if e.key() == Key::Escape && !search_input().is_empty() {
                                e.prevent_default();
                                search_input.set(String::new());
                                search_seq += 1;
                                submit_search();
                            }
                        },
                    }
                }
                FormSelect {
                    id: "admin-posts-tag",
                    value: tag(),
                    options: tag_options,
                    trigger_class: FORM_SELECT_COMPACT_CLASS,
                    aria_label: "按标签筛选",
                    onchange: move |value: String| {
                        tag.set(value);
                        reset_view();
                    },
                }
            }
        }

        if error().is_some() {
            LoadError { title: "文章列表加载失败", on_retry: move |_| reload() }
        } else if loading() && rows.is_empty() {
            DelayedSkeleton { PostsTableSkeleton {} }
        } else if rows.is_empty() {
            div { class: "admin-panel",
                if filtering {
                    EmptyState {
                        icon: IconName::SearchX,
                        title: "没有符合条件的文章",
                        description: "换个标题关键词，或清除标签筛选再试。",
                        action: EmptyStateAction {
                            label: "清除筛选".to_string(),
                            onclick: Callback::new(move |_| {
                                search_input.set(String::new());
                                search_query.set(String::new());
                                tag.set(String::new());
                                reset_view();
                            }),
                        },
                    }
                } else if status() == "draft" {
                    EmptyState { title: "没有草稿", description: "未发布的文章会出现在这里。" }
                } else {
                    EmptyState {
                        title: "还没有文章",
                        description: "写下第一篇，它会出现在这里。",
                        action: EmptyStateAction {
                            label: "写文章".to_string(),
                            onclick: Callback::new(move |_| {
                                navigator.push(Route::Write {});
                            }),
                        },
                    }
                }
            }
        } else {
            div { class: "admin-panel admin-table-wrap", "aria-busy": loading().then_some("true"),
                table { class: "admin-table admin-posts-table",
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
                            th { "状态" }
                            th { class: "admin-posts-tags", "标签" }
                            th { class: "admin-table-num admin-posts-words", "字数" }
                            th { "发布时间" }
                            th { class: "admin-table-actions", span { class: "sr-only", "操作" } }
                        }
                    }
                    tbody {
                        for post in rows.iter().cloned() {
                            PostRow {
                                key: "{post.id}",
                                selected: selection.contains(&post.id),
                                busy: busy().contains(&post.id),
                                on_select: move |(id, checked): (i32, bool)| {
                                    if checked {
                                        selected.write().insert(id);
                                    } else {
                                        selected.write().remove(&id);
                                    }
                                },
                                on_action: on_row_action,
                                post,
                            }
                        }
                    }
                }
                TablePager {
                    page: current_page(),
                    total: total(),
                    per_page: POSTS_PER_PAGE,
                    unit: "篇",
                    on_change: move |page| {
                        current_page.set(page);
                        #[cfg(target_arch = "wasm32")]
                        if let Some(scroller) = web_sys::window()
                            .and_then(|w| w.document())
                            .and_then(|d| d.query_selector("[data-vt-scroll]").ok().flatten())
                        {
                            scroller.set_scroll_top(0);
                        }
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
                    onclick: move |_| run_batch(BatchPostAction::Unpublish),
                    "转为草稿"
                }
                button {
                    class: "{button_class(ButtonVariant::Ghost, ButtonSize::Sm)} batch-bar-danger",
                    r#type: "button",
                    disabled: batch_busy(),
                    onclick: move |_| run_batch(BatchPostAction::Trash),
                    "移到回收站"
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
    }
}

/// 表格的一行。标题进入后台预览（保留列表来源与侧栏）；「⋯」菜单回传操作 id。
#[component]
fn PostRow(
    post: PostListItem,
    selected: bool,
    busy: bool,
    on_select: EventHandler<(i32, bool)>,
    on_action: EventHandler<(PostListItem, &'static str)>,
) -> Element {
    let published = post.status == PostStatus::Published;
    let date = post
        .published_at
        .map(|d| to_local(d).format("%Y-%m-%d %H:%M").to_string());
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
    let menu_post = post.clone();
    rsx! {
        tr { "data-selected": selected.then_some("true"), "aria-busy": busy.then_some("true"),
            td { class: "admin-table-check",
                Checkbox {
                    checked: selected,
                    label: format!("选择《{}》", post.title),
                    onchange: move |checked| on_select.call((post.id, checked)),
                }
            }
            td { class: "admin-posts-title",
                Link {
                    to: Route::PostPreview { slug: post.slug.clone() },
                    "data-vt-post-link": "{post.id}",
                    "data-vt-post-id": "{post.id}",
                    "data-vt-role": "title",
                    "{post.title}"
                }
                span { class: "admin-posts-slug", "/{post.slug}" }
            }
            td { class: "admin-posts-status",
                StatusDot { label: post.status.label(), tone: post.status.tone() }
            }
            td { class: "admin-posts-tags",
                if tags.is_empty() {
                    span { class: "admin-table-muted", "—" }
                } else {
                    "{tags}"
                    if let Some(more) = more {
                        span { class: "admin-table-muted", " {more}" }
                    }
                }
            }
            td { class: "admin-table-num admin-posts-words", "{post.word_count}" }
            td { class: "admin-posts-date",
                if let Some(date) = date {
                    time { "{date}" }
                } else {
                    span { class: "admin-table-muted", "未发布" }
                }
            }
            td { class: "admin-table-actions",
                Menu {
                    label: "更多操作",
                    items,
                    on_select: move |id: &'static str| on_action.call((menu_post.clone(), id)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_summary_folds_extra_tags() {
        let tags: Vec<String> = ["Rust", "架构", "Dioxus"]
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
