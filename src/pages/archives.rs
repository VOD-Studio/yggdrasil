//! 归档与标签详情共用的索引页。
//!
//! `/archives`：衬线页标题、标签筛选（Chip）、按年份分组的文章行；年份大字吸附。
//! `/tags/:tag`（见 `tags.rs`）沿用同一套标签区与年份分组，当前标签以浮起样式标示。
//!
//! 首屏请求轻量归档分页，按需加载更早的文章。标签接口失败时只隐藏标签区。

use chrono::{DateTime, Datelike, Utc};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::posts::{
    list_archive_posts, list_tags, ArchiveResponse, TagListResponse, ARCHIVE_PAGE_SIZE,
};
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::archive_skeleton::ArchiveBodySkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{Button, ButtonVariant, LoadError};
use crate::models::post::Tag;
use crate::routes::Route;

/// 标签区默认显示的标签数，其余收进「全部 N 个」。
const TAG_PREVIEW: usize = 16;
/// 每行右侧最多列出的标签数。
const ROW_TAGS: usize = 2;

/// 年份分组里的一行。归档与标签详情的数据源不同，在这里统一形状。
#[derive(Clone, Debug, PartialEq)]
pub struct IndexRow {
    pub id: i32,
    pub slug: String,
    pub title: String,
    pub date: DateTime<Utc>,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct YearGroup {
    pub year: i32,
    pub rows: Vec<IndexRow>,
}

/// 列表已按时间降序；相邻同年的行归为一组。
pub(crate) fn group_by_year(rows: &[IndexRow]) -> Vec<YearGroup> {
    let mut groups: Vec<YearGroup> = Vec::new();
    for row in rows {
        let year = row.date.year();
        match groups.last_mut() {
            Some(group) if group.year == year => group.rows.push(row.clone()),
            _ => groups.push(YearGroup {
                year,
                rows: vec![row.clone()],
            }),
        }
    }
    groups
}

/// 只保留有已发布文章的标签，按篇数降序、同数按名称排列。
pub(crate) fn ranked_tags(mut tags: Vec<Tag>) -> Vec<Tag> {
    tags.retain(|t| t.post_count > 0);
    tags.sort_by(|a, b| {
        b.post_count
            .cmp(&a.post_count)
            .then_with(|| a.name.cmp(&b.name))
    });
    tags
}

/// 收起时显示的标签：前若干个；当前标签排在后面也要露出来。
fn visible_tags<'a>(tags: &'a [Tag], current: Option<&str>, expanded: bool) -> Vec<&'a Tag> {
    if expanded {
        return tags.iter().collect();
    }
    let mut shown: Vec<&Tag> = tags.iter().take(TAG_PREVIEW).collect();
    if let Some(current) = current {
        if !shown.iter().any(|t| t.name == current) {
            if let Some(tag) = tags.iter().find(|t| t.name == current) {
                shown.push(tag);
            }
        }
    }
    shown
}

/// 归档路由，对应 `/archives`（`SsrMode::InOrder`）。
pub struct ArchivesRoute {
    posts: Resource<Result<ArchiveResponse, ServerFnError>>,
    tags: Resource<Result<TagListResponse, ServerFnError>>,
}

#[lazy_route]
impl LazyRoute for ArchivesRoute {
    fn data() -> Self {
        Self {
            posts: Resource::new(|| (), |_| list_archive_posts(1)),
            tags: Resource::new(|| (), |_| list_tags()),
        }
    }

    fn view(this: Self) -> AnyView {
        let ArchivesRoute { posts, tags } = this;
        view! {
            <Title text="归档 · Yggdrasil"/>
            <div class="archive">
                <header class="archive-header page-enter">
                    <h1 class="archive-title">"归档"</h1>
                </header>
                <Suspense fallback=|| {
                    view! {
                        <DelayedSkeleton>
                            <ArchiveBodySkeleton/>
                        </DelayedSkeleton>
                    }
                }>
                    {move || Suspend::new(async move {
                        // 文章失败只让列表区换成错误卡片；标签失败只隐藏标签区。
                        let first = posts.await.ok();
                        let tags = tag_list(tags.await);
                        view! { <ArchivesBody first=first tags=tags/> }
                    })}
                </Suspense>
            </div>
        }
        .into_any()
    }
}

/// 归档主体。`first` 为 `None` 表示首屏加载失败；重试与「加载更早」的结果都写进
/// 本地信号，不重取外层 `Resource`（否则整段 DOM 会退回 `Suspense` 的骨架屏）。
#[component]
fn ArchivesBody(first: Option<ArchiveResponse>, tags: Vec<Tag>) -> impl IntoView {
    let listing = RwSignal::new(first);
    let older = RwSignal::new(Vec::<IndexRow>::new());
    let next_page = RwSignal::new(2_i32);
    let loading_more = RwSignal::new(false);
    let load_more_failed = RwSignal::new(false);
    let exhausted = RwSignal::new(false);
    let retrying = RwSignal::new(false);
    let retry_failed = RwSignal::new(false);

    let loaded = Memo::new(move |_| listing.with(Option::is_some));
    let total = move || listing.with(|l| l.as_ref().map_or(0, |r| r.total));
    let rows = Signal::derive(move || {
        let mut rows: Vec<IndexRow> = listing.with(|l| {
            l.as_ref()
                .map(|r| r.posts.iter().cloned().map(archive_row).collect())
                .unwrap_or_default()
        });
        older.with(|older| rows.extend(older.iter().cloned()));
        rows
    });
    let has_more = move || !exhausted.get() && (rows.with(Vec::len) as i64) < total();

    let on_more = move |_| {
        loading_more.set(true);
        load_more_failed.set(false);
        let page = next_page.get_untracked();
        leptos::task::spawn_local(async move {
            match list_archive_posts(page).await {
                Ok(response) => {
                    let _ = exhausted.try_set(response.posts.len() < ARCHIVE_PAGE_SIZE);
                    let _ = older.try_update(|older| {
                        older.extend(response.posts.into_iter().map(archive_row));
                    });
                    let _ = next_page.try_update(|page| *page += 1);
                }
                Err(_) => {
                    let _ = load_more_failed.try_set(true);
                }
            }
            let _ = loading_more.try_set(false);
        });
    };

    let on_retry = Callback::new(move |()| {
        retrying.set(true);
        retry_failed.set(false);
        leptos::task::spawn_local(async move {
            let result = list_archive_posts(1).await;
            let _ = retrying.try_set(false);
            match result {
                Ok(response) => {
                    let _ = listing.try_set(Some(response));
                }
                Err(_) => {
                    let _ = retry_failed.try_set(true);
                }
            }
        });
    });

    view! {
        {move || {
            loaded
                .get()
                .then(|| {
                    view! {
                        <p class="archive-summary tabular-nums page-enter">
                            {move || archive_summary(&rows.get(), total(), has_more())}
                        </p>
                    }
                })
        }}
        <TagFilter tags=tags current=None/>
        {move || {
            if !loaded.get() {
                return view! {
                    <div class="archive-years" data-vt-list="true">
                        <LoadError
                            title="归档暂时没能加载"
                            retrying=retrying
                            retry_failed=retry_failed
                            on_retry=on_retry
                        />
                    </div>
                }
                    .into_any();
            }
            if total() == 0 {
                return view! {
                    <span hidden data-vt-list="true"></span>
                    <EmptyState title="还没有文章" description="发布文章后，这里会按年份排好。"/>
                }
                    .into_any();
            }
            view! {
                <YearBlocks rows=rows/>
                {move || {
                    has_more()
                        .then(|| {
                            view! {
                                <div class="archive-more">
                                    <Button
                                        variant=ButtonVariant::Secondary
                                        loading=loading_more
                                        onclick=on_more
                                    >
                                        "加载更早的文章"
                                    </Button>
                                    {move || {
                                        load_more_failed
                                            .get()
                                            .then(|| {
                                                view! {
                                                    <p class="archive-more-error" role="alert">
                                                        "没能加载更早的文章，再试一次。"
                                                    </p>
                                                }
                                            })
                                    }}
                                </div>
                            }
                        })
                }}
            }
                .into_any()
        }}
    }
}

fn archive_row(post: crate::models::archive::ArchivePost) -> IndexRow {
    IndexRow {
        id: post.id,
        slug: post.slug,
        title: post.title,
        date: post.published_at,
        tags: post.tags,
    }
}

/// 「41 篇文章，写于 2020 — 2026。」；还有未加载的更早文章时不写起始年份。
fn archive_summary(rows: &[IndexRow], total: i64, has_more: bool) -> String {
    match (rows.first(), rows.last()) {
        (Some(newest), Some(oldest)) if !has_more => {
            let (from, to) = (oldest.date.year(), newest.date.year());
            if from == to {
                format!("{total} 篇文章，写于 {to}。")
            } else {
                format!("{total} 篇文章，写于 {from} — {to}。")
            }
        }
        (Some(newest), _) => format!(
            "{total} 篇文章，最近一篇写于 {}。",
            newest.date.format("%Y.%m.%d")
        ),
        _ => "还没有文章。".to_string(),
    }
}

/// 标签接口的结果：失败时为空，标签区随之隐藏。
pub(crate) fn tag_list(data: Result<TagListResponse, ServerFnError>) -> Vec<Tag> {
    data.map(|r| ranked_tags(r.tags)).unwrap_or_default()
}

/// 标签筛选区。点当前标签回到全部归档；展开状态随历史记录恢复。
///
/// `current` 直接传 `Option<String>`（`None` 或 `Some(..)`）。
#[component]
pub(crate) fn TagFilter(tags: Vec<Tag>, current: Option<String>) -> impl IntoView {
    let entry_id = crate::bridges::navigation::entry_id();
    // 服务端没有历史状态，水合首帧必须与之一致；恢复在挂载后的 Effect 里完成。
    let expanded = RwSignal::new(false);
    Effect::new(move |previous: Option<()>| {
        if previous.is_none() {
            if let Some(saved) = crate::bridges::navigation::read_state::<bool>("archive-tags-open")
            {
                if saved != expanded.get_untracked() {
                    expanded.set(saved);
                }
            }
        }
        crate::bridges::navigation::write_state(&entry_id, "archive-tags-open", &expanded.get());
    });
    if tags.is_empty() {
        return ().into_any();
    }
    let total = tags.len();
    let shown_tags = tags.clone();
    let list_current = current.clone();

    view! {
        <section class="archive-tags page-enter" aria-labelledby="archive-tags-title">
            <div class="archive-tags-head">
                <h2 id="archive-tags-title" class="archive-tags-title">
                    "标签 " <span class="tabular-nums">{total}</span>
                </h2>
                {(total > TAG_PREVIEW)
                    .then(|| {
                        view! {
                            <button
                                type="button"
                                class="archive-tags-toggle"
                                aria-expanded=move || expanded.get().to_string()
                                on:click=move |_| expanded.update(|open| *open = !*open)
                            >
                                {move || {
                                    if expanded.get() { "收起".to_string() } else { format!("全部 {total} 个") }
                                }}
                            </button>
                        }
                    })}
            </div>
            <div class="archive-tags-list">
                {move || {
                    visible_tags(&shown_tags, list_current.as_deref(), expanded.get())
                        .into_iter()
                        .map(|tag| {
                            if list_current.as_deref() == Some(tag.name.as_str()) {
                                view! {
                                    <a
                                        class="chip is-current"
                                        href=Route::Archives.to_string()
                                        aria-current="page"
                                        title="回到全部归档"
                                    >
                                        {tag.name.clone()}
                                        <span class="chip-count">{tag.post_count}</span>
                                    </a>
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <a
                                        class="chip"
                                        href=Route::TagDetail {
                                            tag: tag.name.clone(),
                                        }
                                            .to_string()
                                    >
                                        {tag.name.clone()}
                                        <span class="chip-count">{tag.post_count}</span>
                                    </a>
                                }
                                    .into_any()
                            }
                        })
                        .collect_view()
                }}
            </div>
        </section>
    }
    .into_any()
}

/// 按年份分组的文章行。`hide_tag` 为标签详情的当前标签，不在行尾重复列出（缺省则都列出）。
///
/// 年份组按年份做 key：加载更早的文章只追加新年份、更新已有年份的行，
/// 已显示的年份组不重建（不重播入场动画）。
#[component]
pub(crate) fn YearBlocks(
    #[prop(into)] rows: Signal<Vec<IndexRow>>,
    #[prop(optional)] hide_tag: Option<String>,
) -> impl IntoView {
    // 年份只会向后追加，序号即首次出现的位置。
    let years = Memo::new(move |_| {
        rows.with(|rows| {
            group_by_year(rows)
                .into_iter()
                .enumerate()
                .map(|(index, group)| (group.year, index))
                .collect::<Vec<_>>()
        })
    });

    view! {
        <section class="archive-years" data-vt-list="true" aria-label="按年份">
            <For
                each=move || years.get()
                key=|(year, _)| *year
                children=move |(year, index)| {
                    view! { <YearBlock year=year index=index rows=rows hide_tag=hide_tag.clone()/> }
                }
            />
        </section>
    }
}

#[component]
fn YearBlock(
    year: i32,
    index: usize,
    rows: Signal<Vec<IndexRow>>,
    hide_tag: Option<String>,
) -> impl IntoView {
    let group_rows = Memo::new(move |_| {
        rows.with(|rows| {
            rows.iter()
                .filter(|row| row.date.year() == year)
                .cloned()
                .collect::<Vec<_>>()
        })
    });

    view! {
        <div
            class="archive-year page-enter"
            style=format!("--enter-index: {}", index.min(5))
        >
            <div class="archive-year-label">
                <h2 id=format!("y{year}") class="archive-year-number">
                    {year}
                </h2>
                <p class="archive-year-count tabular-nums">
                    {move || format!("{} 篇", group_rows.with(Vec::len))}
                </p>
            </div>
            <ol class="archive-rows" aria-labelledby=format!("y{year}")>
                <For
                    each=move || group_rows.get()
                    key=|row| row.id
                    children=move |row| {
                        view! {
                            <li>
                                <ArchiveRow row=row hide_tag=hide_tag.clone()/>
                            </li>
                        }
                    }
                />
            </ol>
        </div>
    }
}

#[component]
fn ArchiveRow(row: IndexRow, hide_tag: Option<String>) -> impl IntoView {
    let tags = row
        .tags
        .iter()
        .filter(|t| Some(t.as_str()) != hide_tag.as_deref())
        .take(ROW_TAGS)
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ");
    let datetime = row.date.format("%Y-%m-%d").to_string();
    let date = row.date.format("%m.%d").to_string();
    let tags = (!tags.is_empty()).then(|| view! { <span class="archive-entry-tags">{tags}</span> });
    view! {
        <a
            class="archive-entry"
            href=Route::PostDetail {
                slug: row.slug.clone(),
            }
                .to_string()
            data-vt-post-link=row.id.to_string()
        >
            <time class="archive-entry-date tabular-nums" datetime=datetime>
                {date}
            </time>
            <span
                class="archive-entry-title"
                data-vt-post-id=row.id.to_string()
                data-vt-role="title"
            >
                {row.title}
            </span>
            {tags}
        </a>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn row(id: i32, y: i32, m: u32, d: u32) -> IndexRow {
        IndexRow {
            id,
            slug: format!("post-{id}"),
            title: format!("文章 {id}"),
            date: Utc.with_ymd_and_hms(y, m, d, 8, 0, 0).unwrap(),
            tags: Vec::new(),
        }
    }

    fn tag(id: i32, name: &str, post_count: i64) -> Tag {
        Tag {
            id,
            name: name.into(),
            post_count,
        }
    }

    #[test]
    fn groups_adjacent_rows_by_year() {
        let rows = [
            row(1, 2026, 10, 15),
            row(2, 2026, 1, 2),
            row(3, 2024, 5, 22),
        ];
        let groups = group_by_year(&rows);
        let years: Vec<_> = groups.iter().map(|g| (g.year, g.rows.len())).collect();
        assert_eq!(years, [(2026, 2), (2024, 1)]);
        assert!(group_by_year(&[]).is_empty());
    }

    #[test]
    fn summary_names_the_year_range_only_when_everything_is_loaded() {
        let rows = [row(1, 2026, 10, 15), row(2, 2020, 12, 11)];
        assert_eq!(
            archive_summary(&rows, 2, false),
            "2 篇文章，写于 2020 — 2026。"
        );
        assert_eq!(
            archive_summary(&rows, 150, true),
            "150 篇文章，最近一篇写于 2026.10.15。"
        );
        assert_eq!(
            archive_summary(&rows[..1], 1, false),
            "1 篇文章，写于 2026。"
        );
        assert_eq!(archive_summary(&[], 0, false), "还没有文章。");
    }

    #[test]
    fn ranks_tags_by_count_and_drops_empty_ones() {
        let ranked = ranked_tags(vec![
            tag(1, "Go", 4),
            tag(2, "Rust", 16),
            tag(3, "草稿", 0),
            tag(4, "CSS", 4),
        ]);
        let names: Vec<_> = ranked.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Rust", "CSS", "Go"]);
    }

    #[test]
    fn collapsed_tags_still_show_the_current_one() {
        let tags: Vec<Tag> = (0..20)
            .map(|i| tag(i, &format!("t{i}"), 100 - i64::from(i)))
            .collect();
        let names = |shown: Vec<&Tag>| shown.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
        assert_eq!(visible_tags(&tags, None, false).len(), TAG_PREVIEW);
        let shown = names(visible_tags(&tags, Some("t19"), false));
        assert_eq!(shown.len(), TAG_PREVIEW + 1);
        assert_eq!(shown.last().map(String::as_str), Some("t19"));
        assert_eq!(visible_tags(&tags, Some("t3"), false).len(), TAG_PREVIEW);
        assert_eq!(visible_tags(&tags, None, true).len(), 20);
    }
}
