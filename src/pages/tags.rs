//! 标签详情 `/tags/:tag`：与归档同一套版式，只列出该标签下的文章。
//! 文章取自 `get_posts_by_tag`（最近 200 篇）；每个标签拥有独立的请求。

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::posts::{get_posts_by_tag, PostListResponse, TagListResponse};
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::archive_skeleton::ArchiveBodySkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{Icon, IconName, LoadError};
use crate::hooks::ssr_resource::{route_data, PageJson, RouteData};
use crate::models::post::PostListItem;
use crate::pages::archives::{fetch_tags, tag_list, IndexRow, TagFilter, YearBlocks};
use crate::routes::Route;

/// 标签详情路由，对应 `/tags/:tag`（`SsrMode::Async`：`<Title>` 随标签而定，
/// 且需要在流式输出前确定响应头）。
pub struct TagRoute {
    tag: Memo<String>,
    posts: RouteData<PostListResponse>,
    tags: RouteData<TagListResponse>,
}

#[leptos::lazy]
async fn fetch_tag_posts(tag: String) -> PageJson<PostListResponse> {
    PageJson::new(get_posts_by_tag(tag).await)
}

#[lazy_route]
impl LazyRoute for TagRoute {
    fn data() -> Self {
        let params = use_params_map();
        let tag = Memo::new(move |_| params.read().get("tag").unwrap_or_default());
        Self {
            tag,
            posts: route_data(move || tag.get(), fetch_tag_posts),
            tags: route_data(|| (), |_| fetch_tags()),
        }
    }

    fn view(this: Self) -> AnyView {
        let TagRoute { tag, posts, tags } = this;
        // 按标签重建整页（含 Suspense），旧结果不会在新标题下短暂出现。
        (move || {
            let current = tag.get();
            view! {
                <Title text=format!("{current} · 标签 · Yggdrasil")/>
                <div class="archive">
                    <TagIntro tag=current.clone()/>
                    <Suspense fallback=|| {
                        view! {
                            <DelayedSkeleton>
                                <ArchiveBodySkeleton/>
                            </DelayedSkeleton>
                        }
                    }>
                        {
                            let current = current.clone();
                            move || {
                                let current = current.clone();
                                Suspend::new(async move {
                                    let listing = posts.await.ok();
                                    let tags = tag_list(tags.await);
                                    view! { <TagBody tag=current initial=listing tags=tags/> }
                                })
                            }
                        }
                    </Suspense>
                </div>
            }
        })
        .into_any()
    }
}

/// 路由兜底和实际页面共用开场；标签名无需等待文章请求。
#[component]
pub fn TagIntro(#[prop(into)] tag: String) -> impl IntoView {
    view! {
        <header class="archive-header page-enter">
            <a class="archive-back" href=Route::Archives.to_string()>
                <Icon name=IconName::ArrowLeft size=14/>
                "全部归档"
            </a>
            <h1 class="archive-title">
                <span class="archive-title-hash" aria-hidden="true">
                    "# "
                </span>
                {tag}
            </h1>
        </header>
    }
}

/// 标签详情主体。`initial` 为 `None` 表示文章加载失败；重试结果写进本地信号，
/// 不重取外层 `Resource`（否则整段 DOM 会退回 `Suspense` 的骨架屏）。
#[component]
fn TagBody(
    tag: String,
    initial: Option<PostListResponse>,
    tags: Vec<crate::models::post::Tag>,
) -> impl IntoView {
    let listing = RwSignal::new(initial);
    let retrying = RwSignal::new(false);
    let retry_failed = RwSignal::new(false);

    let request_tag = tag.clone();
    let on_retry = Callback::new(move |()| {
        retrying.set(true);
        retry_failed.set(false);
        let tag = request_tag.clone();
        leptos::task::spawn_local(async move {
            let result = get_posts_by_tag(tag).await;
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

    let body_tag = tag.clone();
    view! {
        {move || {
            listing
                .with(|listing| {
                    listing
                        .as_ref()
                        .map(|PostListResponse { posts, total }| tag_summary(posts, *total))
                })
                .map(|summary| {
                    view! { <p class="archive-summary tabular-nums page-enter">{summary}</p> }
                })
        }}
        <TagFilter tags=tags current=Some(tag.clone())/>
        {move || {
            let Some(PostListResponse { posts, total }) = listing.get() else {
                return view! {
                    <div class="archive-years" data-vt-list="true">
                        <LoadError
                            title="这个标签下的文章暂时没能加载"
                            retrying=retrying
                            retry_failed=retry_failed
                            on_retry=on_retry
                        />
                    </div>
                }
                    .into_any();
            };
            let rows: Vec<IndexRow> = posts.iter().map(tag_row).collect();
            let truncated = total > posts.len() as i64;
            let listed = posts.len();
            if rows.is_empty() {
                return view! {
                    <span hidden data-vt-list="true"></span>
                    <EmptyState
                        title="这个标签下还没有文章"
                        description="换一个标签，或者回到全部归档看看。"
                        icon=IconName::Tag
                    />
                }
                    .into_any();
            }
            view! {
                <YearBlocks rows=rows hide_tag=body_tag.clone()/>
                {truncated
                    .then(|| {
                        view! {
                            <p class="archive-limit-note tabular-nums">
                                {format!("共 {total} 篇，这里列出最近的 {listed} 篇。")}
                            </p>
                        }
                    })}
            }
                .into_any()
        }}
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
