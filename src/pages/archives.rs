//! 归档与标签详情共用的索引页。
//!
//! `/archives`：衬线页标题、标签筛选（Chip）、按年份分组的文章行；年份大字吸附。
//! `/tags/:tag`（见 `tags.rs`）沿用同一套标签区与年份分组，当前标签以浮起样式标示。
//!
//! 首屏请求轻量归档分页，按需加载更早的文章。标签接口失败时只隐藏标签区。

use chrono::{DateTime, Datelike, Utc};
use dioxus::prelude::*;

use crate::api::posts::{
    list_archive_posts, list_tags, ArchiveResponse, TagListResponse, ARCHIVE_PAGE_SIZE,
};
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::archive_skeleton::ArchiveBodySkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{Button, ButtonVariant, LoadError};
use crate::models::post::Tag;
use crate::router::Route;

/// 标签区默认显示的标签数，其余收进「全部 N 个」。
const TAG_PREVIEW: usize = 16;
/// 每行右侧最多列出的标签数。
const ROW_TAGS: usize = 2;

/// 年份分组里的一行。归档与标签详情的数据源不同，在这里统一形状。
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IndexRow {
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

#[component]
pub fn Archives() -> Element {
    rsx! {
        div { class: "archive",
            header { class: "archive-header page-enter",
                h1 { class: "archive-title", "归档" }
            }
            SuspenseBoundary {
                fallback: move |_| rsx! { DelayedSkeleton { ArchiveBodySkeleton {} } },
                ArchivesBody {}
            }
        }
    }
}

#[component]
fn ArchivesBody() -> Element {
    // SSR 只轮询正在挂起的任务，提前发出的第二个请求会丢失唤醒；逐个等待。
    let mut posts = use_server_future(move || list_archive_posts(1))?;
    let tags = use_server_future(list_tags)?;

    let mut older = use_signal(Vec::<IndexRow>::new);
    let mut next_page = use_signal(|| 2_i32);
    let mut loading_more = use_signal(|| false);
    let mut load_more_failed = use_signal(|| false);
    let mut exhausted = use_signal(|| false);
    let mut retrying = use_signal(|| false);
    let mut retry_failed = use_signal(|| false);

    let tags = tag_list(&tags.read());
    let listing = posts.read().as_ref().map(|r| r.as_ref().ok().cloned());

    match listing {
        Some(Some(ArchiveResponse {
            posts: first,
            total,
        })) => {
            let mut rows: Vec<IndexRow> = first.into_iter().map(archive_row).collect();
            rows.extend(older.read().iter().cloned());
            let has_more = !exhausted() && (rows.len() as i64) < total;
            let summary = archive_summary(&rows, total, has_more);
            rsx! {
                p { class: "archive-summary tabular-nums page-enter", "{summary}" }
                TagFilter { tags, current: None }
                if total == 0 {
                    span { hidden: true, "data-vt-list": "true" }
                    EmptyState { title: "还没有文章", description: "发布文章后，这里会按年份排好。" }
                } else {
                    YearBlocks { rows, hide_tag: None }
                    if has_more {
                        div { class: "archive-more",
                            Button {
                                variant: ButtonVariant::Secondary,
                                loading: loading_more(),
                                onclick: move |_| {
                                    loading_more.set(true);
                                    load_more_failed.set(false);
                                    spawn(async move {
                                        match list_archive_posts(next_page()).await {
                                            Ok(response) => {
                                                exhausted.set(response.posts.len() < ARCHIVE_PAGE_SIZE);
                                                older.write().extend(response.posts.into_iter().map(archive_row));
                                                next_page += 1;
                                            }
                                            Err(_) => load_more_failed.set(true),
                                        }
                                        loading_more.set(false);
                                    });
                                },
                                "加载更早的文章"
                            }
                            if load_more_failed() {
                                p { class: "archive-more-error", role: "alert", "没能加载更早的文章，再试一次。" }
                            }
                        }
                    }
                }
            }
        }
        Some(None) => rsx! {
            TagFilter { tags, current: None }
            div { class: "archive-years", "data-vt-list": "true",
                LoadError {
                    title: "归档暂时没能加载",
                    retrying: retrying(),
                    retry_failed: retry_failed(),
                    on_retry: move |_| {
                        retrying.set(true);
                        retry_failed.set(false);
                        spawn(async move {
                            let result = list_archive_posts(1).await;
                            retrying.set(false);
                            match result {
                                Ok(response) => posts.set(Some(Ok(response))),
                                Err(_) => retry_failed.set(true),
                            }
                        });
                    },
                }
            }
        },
        None => rsx! {},
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

/// 标签接口的结果：失败或未取到时为空，标签区随之隐藏。
pub(crate) fn tag_list(data: &Option<Result<TagListResponse, ServerFnError>>) -> Vec<Tag> {
    data.as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|r| ranked_tags(r.tags.clone()))
        .unwrap_or_default()
}

/// 标签筛选区。点当前标签回到全部归档；展开状态随历史记录恢复。
#[component]
pub(crate) fn TagFilter(tags: Vec<Tag>, current: Option<String>) -> Element {
    let entry_id = use_hook(crate::bridges::navigation::entry_id);
    let mut expanded =
        use_signal(|| crate::bridges::navigation::read_state("archive-tags-open").unwrap_or(false));
    use_effect(move || {
        crate::bridges::navigation::write_state(&entry_id, "archive-tags-open", &expanded());
    });
    if tags.is_empty() {
        return rsx! {};
    }
    let shown = visible_tags(&tags, current.as_deref(), expanded());
    let total = tags.len();

    rsx! {
        section { class: "archive-tags page-enter", aria_labelledby: "archive-tags-title",
            div { class: "archive-tags-head",
                h2 { id: "archive-tags-title", class: "archive-tags-title",
                    "标签 "
                    span { class: "tabular-nums", "{total}" }
                }
                if total > TAG_PREVIEW {
                    button {
                        r#type: "button",
                        class: "archive-tags-toggle",
                        aria_expanded: "{expanded()}",
                        onclick: move |_| expanded.toggle(),
                        if expanded() { "收起" } else { "全部 {total} 个" }
                    }
                }
            }
            div { class: "archive-tags-list",
                for tag in shown {
                    if current.as_deref() == Some(tag.name.as_str()) {
                        Link {
                            key: "{tag.id}",
                            class: "chip is-current",
                            to: Route::Archives {},
                            aria_current: "page",
                            title: "回到全部归档",
                            "{tag.name}"
                            span { class: "chip-count", "{tag.post_count}" }
                        }
                    } else {
                        Link {
                            key: "{tag.id}",
                            class: "chip",
                            to: Route::TagDetail { tag: tag.name.clone() },
                            "{tag.name}"
                            span { class: "chip-count", "{tag.post_count}" }
                        }
                    }
                }
            }
        }
    }
}

/// 按年份分组的文章行。`hide_tag` 为标签详情的当前标签，不在行尾重复列出。
#[component]
pub(crate) fn YearBlocks(rows: Vec<IndexRow>, hide_tag: Option<String>) -> Element {
    let groups = group_by_year(&rows);
    rsx! {
        section { class: "archive-years", "data-vt-list": "true", aria_label: "按年份",
            for (index, group) in groups.into_iter().enumerate() {
                div {
                    key: "{group.year}",
                    class: "archive-year page-enter",
                    style: "--enter-index: {index.min(5)}",
                    div { class: "archive-year-label",
                        h2 { id: "y{group.year}", class: "archive-year-number", "{group.year}" }
                        p { class: "archive-year-count tabular-nums", "{group.rows.len()} 篇" }
                    }
                    ol { class: "archive-rows", aria_labelledby: "y{group.year}",
                        for row in group.rows {
                            li { key: "{row.id}",
                                ArchiveRow { row: row.clone(), hide_tag: hide_tag.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ArchiveRow(row: IndexRow, hide_tag: Option<String>) -> Element {
    let tags = row
        .tags
        .iter()
        .filter(|t| Some(t.as_str()) != hide_tag.as_deref())
        .take(ROW_TAGS)
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ");
    let datetime = row.date.format("%Y-%m-%d").to_string();
    rsx! {
        Link {
            class: "archive-entry",
            to: Route::PostDetail { slug: row.slug.clone() },
            "data-vt-post-link": "{row.id}",
            time { class: "archive-entry-date tabular-nums", datetime: "{datetime}",
                {row.date.format("%m.%d").to_string()}
            }
            span { class: "archive-entry-title", "data-vt-post-id": "{row.id}", "data-vt-role": "title", "{row.title}" }
            if !tags.is_empty() {
                span { class: "archive-entry-tags", "{tags}" }
            }
        }
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
