//! 标签详情 `/tags/:tag`：与归档同一套版式，只列出该标签下的文章。
//! 文章取自 `get_posts_by_tag`（最近 200 篇）；每个标签拥有独立的请求。

use dioxus::prelude::*;

use crate::api::posts::{get_posts_by_tag, list_tags, PostListResponse};
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::archive_skeleton::ArchiveBodySkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{Icon, IconName, LoadError};
use crate::models::post::PostListItem;
use crate::pages::archives::{tag_list, IndexRow, TagFilter, YearBlocks};
use crate::router::Route;

#[component]
pub fn TagDetail(tag: String) -> Element {
    rsx! {
        div { class: "archive",
            TagIntro { tag: tag.clone() }
            // 单元素 keyed 列表使同路由变体切换时卸载旧请求；
            // key 放在 Suspense 外面，旧结果不会在新标题下短暂出现。
            for current_tag in std::iter::once(tag) {
                SuspenseBoundary {
                    key: "{current_tag}",
                    fallback: move |_| rsx! { DelayedSkeleton { ArchiveBodySkeleton {} } },
                    TagBody { tag: current_tag }
                }
            }
        }
    }
}

/// 路由兜底和实际页面共用开场；标签名无需等待文章请求。
#[component]
pub(crate) fn TagIntro(tag: String) -> Element {
    rsx! {
        header { class: "archive-header page-enter",
            Link { class: "archive-back", to: Route::Archives {},
                Icon { name: IconName::ArrowLeft, size: 14 }
                "全部归档"
            }
            h1 { class: "archive-title",
                span { class: "archive-title-hash", aria_hidden: "true", "# " }
                "{tag}"
            }
        }
    }
}

#[component]
fn TagBody(tag: String) -> Element {
    // 父组件按标签 remount，因此闭包中的 tag 在此作用域内不会过期。
    let request_tag = tag.clone();
    let posts = use_server_future(move || get_posts_by_tag(request_tag.clone()));
    let tags = use_server_future(list_tags);
    let mut posts = posts?;
    let tags = tags?;
    let mut retrying = use_signal(|| false);
    let mut retry_failed = use_signal(|| false);

    let tags = tag_list(&tags.read());
    let listing = posts.read().as_ref().map(|r| r.as_ref().ok().cloned());

    match listing {
        Some(Some(PostListResponse { posts: list, total })) => {
            let rows: Vec<IndexRow> = list.iter().map(tag_row).collect();
            let summary = tag_summary(&list, total);
            let truncated = total > list.len() as i64;
            rsx! {
                p { class: "archive-summary tabular-nums page-enter", "{summary}" }
                TagFilter { tags, current: Some(tag.clone()) }
                if rows.is_empty() {
                    span { hidden: true, "data-vt-list": "true" }
                    EmptyState {
                        title: "这个标签下还没有文章",
                        description: "换一个标签，或者回到全部归档看看。",
                        icon: IconName::Tag,
                    }
                } else {
                    YearBlocks { rows, hide_tag: Some(tag.clone()) }
                    if truncated {
                        p { class: "archive-limit-note tabular-nums",
                            "共 {total} 篇，这里列出最近的 {list.len()} 篇。"
                        }
                    }
                }
            }
        }
        Some(None) => rsx! {
            TagFilter { tags, current: Some(tag.clone()) }
            div { class: "archive-years", "data-vt-list": "true",
                LoadError {
                    title: "这个标签下的文章暂时没能加载",
                    retrying: retrying(),
                    retry_failed: retry_failed(),
                    on_retry: move |_| {
                        retrying.set(true);
                        retry_failed.set(false);
                        let tag = tag.clone();
                        spawn(async move {
                            let result = get_posts_by_tag(tag).await;
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

fn tag_row(post: &PostListItem) -> IndexRow {
    IndexRow {
        id: post.id,
        slug: post.slug.clone(),
        title: post.title.clone(),
        date: post.published_at.unwrap_or(post.created_at),
        tags: post.tags.clone(),
    }
}

/// 「16 篇文章，最近一篇写于 2026.10.15。」
fn tag_summary(posts: &[PostListItem], total: i64) -> String {
    match posts
        .iter()
        .map(|p| p.published_at.unwrap_or(p.created_at))
        .max()
    {
        Some(latest) => format!(
            "{total} 篇文章，最近一篇写于 {}。",
            latest.format("%Y.%m.%d")
        ),
        None => "还没有文章。".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::post::PostStatus;
    use chrono::{TimeZone, Utc};

    fn post(id: i32, created_day: u32, published_day: Option<u32>) -> PostListItem {
        let date = |day| Utc.with_ymd_and_hms(2026, 7, day, 0, 0, 0).unwrap();
        PostListItem {
            id,
            author_id: 1,
            title: format!("Article {id}"),
            slug: format!("article-{id}"),
            summary: None,
            status: PostStatus::Published,
            published_at: published_day.map(date),
            created_at: date(created_day),
            updated_at: date(created_day),
            deleted_at: None,
            tags: Vec::new(),
            cover_image: None,
            reading_time: 1,
            word_count: 0,
        }
    }

    #[test]
    fn summary_uses_the_latest_publication_or_creation_date() {
        let posts = [post(1, 1, Some(10)), post(2, 20, None), post(3, 2, Some(5))];
        assert_eq!(
            tag_summary(&posts, 3),
            "3 篇文章，最近一篇写于 2026.07.20。"
        );
        assert_eq!(tag_summary(&[], 0), "还没有文章。");
    }

    #[test]
    fn rows_fall_back_to_creation_date() {
        let row = tag_row(&post(7, 3, None));
        assert_eq!(row.date, Utc.with_ymd_and_hms(2026, 7, 3, 0, 0, 0).unwrap());
        assert_eq!(row.slug, "article-7");
    }
}
