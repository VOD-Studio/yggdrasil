//! 首页「书桌」与分页首页。
//!
//! 第 1 页：状态行、站点标语、搜索与常读标签、最新一篇（右侧目录预览）、按「年 · 月」
//! 分组的文章索引、尾部三格。第 2 页起只有页标题与索引。数据都来自已缓存的公开接口，
//! 在 `LazyRoute::data` 里一次建好，SSR 一次取齐。
//!
//! [`HomeRoute`] 同时服务 `/`（第 1 页）与 `/page/:page`。

use chrono::{DateTime, Datelike, Utc};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::notes::list_notes;
use crate::api::posts::{
    latest_post_outline, list_published_posts, list_tags, PostListResponse, PostOutline,
    TagListResponse,
};
use crate::api::settings::get_site_settings;
use crate::components::empty_state::EmptyState;
use crate::components::post::post_meta::group_digits;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::home_skeleton::HomeSkeleton;
use crate::components::ui::{
    Composer, ComposerInput, ComposerSend, Icon, IconName, LoadError, Pagination,
};
use crate::models::note::{Note, NoteFilter, NotePage};
use crate::models::post::{PostListItem, Tag};
use crate::models::settings::{HomeTagline, SiteSettings};
use crate::pages::archives::ranked_tags;
use crate::pages::not_found::NotFound;
use crate::routes::Route;

const POSTS_PER_PAGE: i32 = 10;
/// 搜索框下「常读」标签数。
const TOP_TAGS: usize = 6;
/// 尾部「常去标签」数。
const TAG_CLOUD: usize = 10;
/// 尾部「最近笔记」数。
const RECENT_NOTES: usize = 4;

/// 首页路由，对应 `/` 与 `/page/:page`（无 `page` 参数视为第 1 页，`SsrMode::InOrder`）。
///
/// 文章列表随页码重取；其余四个数据源只有第 1 页用到，翻到后面的页码时 source 为
/// `false`，fetcher 直接返回 `None`，不再发请求。
pub struct HomeRoute {
    /// 页码（至少为 1）；`page` 参数不是整数时为 `None`，渲染 404。
    page: Memo<Option<i32>>,
    posts: Resource<Result<PostListResponse, ServerFnError>>,
    tags: Resource<Result<Option<TagListResponse>, ServerFnError>>,
    settings: Resource<Result<Option<SiteSettings>, ServerFnError>>,
    outline: Resource<Result<Option<PostOutline>, ServerFnError>>,
    notes: Resource<Result<Option<NotePage>, ServerFnError>>,
}

#[lazy_route]
impl LazyRoute for HomeRoute {
    fn data() -> Self {
        let params = use_params_map();
        let page = Memo::new(move |_| match params.read().get("page") {
            None => Some(1),
            Some(page) => page.parse::<i32>().ok().map(|page| page.max(1)),
        });
        let is_first = Memo::new(move |_| page.get() == Some(1));
        Self {
            page,
            posts: Resource::new(
                move || page.get().unwrap_or(1),
                |page| list_published_posts(page, POSTS_PER_PAGE),
            ),
            tags: Resource::new(
                move || is_first.get(),
                |first| async move {
                    if first {
                        list_tags().await.map(Some)
                    } else {
                        Ok(None)
                    }
                },
            ),
            settings: Resource::new(
                move || is_first.get(),
                |first| async move {
                    if first {
                        get_site_settings().await.map(Some)
                    } else {
                        Ok(None)
                    }
                },
            ),
            outline: Resource::new(
                move || is_first.get(),
                |first| async move {
                    if first {
                        latest_post_outline().await
                    } else {
                        Ok(None)
                    }
                },
            ),
            notes: Resource::new(
                move || is_first.get(),
                |first| async move {
                    if first {
                        list_notes(NoteFilter::default()).await.map(Some)
                    } else {
                        Ok(None)
                    }
                },
            ),
        }
    }

    fn view(this: Self) -> AnyView {
        let HomeRoute {
            page,
            posts,
            tags,
            settings,
            outline,
            notes,
        } = this;
        let title = move || match page.get().unwrap_or(1) {
            1 => "Yggdrasil Blog".to_string(),
            current_page => format!("第 {current_page} 页 · Yggdrasil"),
        };
        view! {
            <Title text=title/>
            <Suspense fallback=move || {
                let current_page = page.get().unwrap_or(1);
                view! {
                    <DelayedSkeleton>
                        <HomeSkeleton current_page=current_page/>
                    </DelayedSkeleton>
                }
            }>
                {move || {
                    let current_page = page.get();
                    Suspend::new(async move {
                        match current_page {
                            None => view! { <NotFound/> }.into_any(),
                            Some(1) => {
                                // 文章列表失败只把文章区换成错误卡片；其余数据失败退回空值。
                                let posts = posts.await.ok();
                                let tags = ranked_tags(
                                    tags.await.ok().flatten().map(|r| r.tags).unwrap_or_default(),
                                );
                                let tagline = settings
                                    .await
                                    .ok()
                                    .flatten()
                                    .unwrap_or_default()
                                    .home_tagline();
                                let outline = outline.await.ok().flatten();
                                let notes: Vec<Note> = notes
                                    .await
                                    .ok()
                                    .flatten()
                                    .map(|page| {
                                        page.notes.into_iter().take(RECENT_NOTES).collect()
                                    })
                                    .unwrap_or_default();
                                view! {
                                    <HomeDesk
                                        posts=posts
                                        tags=tags
                                        tagline=tagline
                                        outline=outline
                                        notes=notes
                                    />
                                }
                                    .into_any()
                            }
                            Some(current_page) => {
                                let posts = posts.await.ok();
                                view! { <HomeArchive current_page=current_page posts=posts/> }
                                    .into_any()
                            }
                        }
                    })
                }}
            </Suspense>
        }
        .into_any()
    }
}

/// 第 1 页。文章列表失败时只把文章区换成错误卡片，开场照常显示。
///
/// `posts` 为 `None` 表示列表请求失败；重试成功后写进本地信号，不重取外层 `Resource`
/// （否则整页会退回骨架屏）。
#[component]
fn HomeDesk(
    posts: Option<PostListResponse>,
    tags: Vec<Tag>,
    tagline: HomeTagline,
    outline: Option<PostOutline>,
    notes: Vec<Note>,
) -> impl IntoView {
    let listing = RwSignal::new(posts);
    let latest = move || listing.with(|l| l.as_ref().and_then(|r| r.posts.first().cloned()));
    let updated = Signal::derive(move || latest().as_ref().map(post_date));
    let total = Signal::derive(move || listing.with(|l| l.as_ref().map(|r| r.total)));
    // 两次读取之间可能有新文章发布；slug 不一致时不显示目录。
    let outline_for_latest =
        move || latest().and_then(|post| outline.clone().filter(|o| o.slug == post.slug));

    let top_tags = (!tags.is_empty()).then(|| {
        let chips = tags
            .iter()
            .take(TOP_TAGS)
            .map(|tag| {
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
            })
            .collect_view();
        view! {
            <div class="home-top-tags">
                <span class="home-top-tags-label">"常读"</span>
                {chips}
            </div>
        }
    });
    let tag_count = tags.len();
    let subtitle = (!tagline.subtitle.is_empty()).then(|| {
        view! {
            <br/>
            <span class="home-title-muted">{tagline.subtitle.clone()}</span>
        }
    });
    let cloud = tags.into_iter().take(TAG_CLOUD).collect::<Vec<_>>();

    view! {
        <div class="home">
            <section class="home-hero home-enter" aria-labelledby="home-title">
                <HomeStatus updated=updated total=total tag_count=tag_count/>
                <h1 id="home-title" class="home-title">
                    {tagline.title.clone()} {subtitle}
                </h1>
                <p class="home-intro">{tagline.intro.clone()}</p>
                <HomeSearch/>
                {top_tags}
            </section>
            {move || match listing.get() {
                Some(PostListResponse { posts, total }) => {
                    let latest_section = posts
                        .first()
                        .cloned()
                        .map(|latest| {
                            view! {
                                <section
                                    class="home-section home-enter"
                                    aria-labelledby="home-latest-label"
                                >
                                    <h2 id="home-latest-label" class="section-label">
                                        "最新一篇"
                                    </h2>
                                    <LatestPanel post=latest outline=outline_for_latest()/>
                                </section>
                            }
                        });
                    let rest = posts.into_iter().skip(1).collect::<Vec<_>>();
                    view! {
                        {latest_section}
                        <HomeIndex current_page=1 posts=rest total=total/>
                    }
                        .into_any()
                }
                None => {
                    view! {
                        <section
                            id="home-posts"
                            class="home-index"
                            tabindex="-1"
                            aria-labelledby="home-index-title"
                        >
                            <span hidden data-vt-list="true"></span>
                            <HomeIndexHeading total=None/>
                            <HomePostsError
                                current_page=1
                                on_recovered=move |response| listing.set(Some(response))
                            />
                        </section>
                    }
                        .into_any()
                }
            }}
            <HomeCorners notes=notes tags=cloud/>
        </div>
    }
}

/// 第 2 页起：页标题与索引。
#[component]
fn HomeArchive(current_page: i32, posts: Option<PostListResponse>) -> impl IntoView {
    let listing = RwSignal::new(posts);
    view! {
        <div class="home home-paged">
            <header class="home-paged-header home-enter">
                <a class="home-back" href=Route::Home.to_string()>
                    <Icon name=IconName::ArrowLeft size=15/>
                    "回到首页"
                </a>
                <h1 class="home-paged-title">"全部文章"</h1>
                <p class="home-paged-page">{format!("第 {current_page} 页")}</p>
            </header>
            {move || match listing.get() {
                Some(PostListResponse { posts, total }) => {
                    view! { <HomeIndex current_page=current_page posts=posts total=total/> }
                        .into_any()
                }
                None => {
                    view! {
                        <section
                            id="home-posts"
                            class="home-index"
                            tabindex="-1"
                            aria-label="文章列表"
                        >
                            <span hidden data-vt-list="true"></span>
                            <HomePostsError
                                current_page=current_page
                                on_recovered=move |response| listing.set(Some(response))
                            />
                        </section>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

/// 状态行：Clay 呼吸点、最近更新、篇数与标签数。篇数链到下方索引。
#[component]
fn HomeStatus(
    #[prop(into)] updated: Signal<Option<String>>,
    #[prop(into)] total: Signal<Option<i64>>,
    tag_count: usize,
) -> impl IntoView {
    view! {
        <p class="home-status">
            <span class="home-pulse" aria-hidden="true"></span>
            {move || {
                updated
                    .get()
                    .map(|date| {
                        view! {
                            <span>
                                "最近更新 " <time class="tabular-nums">{date}</time>
                            </span>
                        }
                    })
            }}
            {move || {
                total
                    .get()
                    .map(|total| {
                        view! {
                            <span class="home-status-sep" aria-hidden="true">
                                "·"
                            </span>
                            <a class="home-status-link" href="#home-posts">
                                <span class="tabular-nums">{total}</span>
                                " 篇文章"
                            </a>
                        }
                    })
            }}
            {(tag_count > 0)
                .then(|| {
                    view! {
                        <span class="home-status-sep" aria-hidden="true">
                            "·"
                        </span>
                        <span>
                            <span class="tabular-nums">{tag_count}</span>
                            " 个标签"
                        </span>
                    }
                })}
        </p>
    }
}

/// 开场搜索框：提交进入全文搜索页。没有脚本时表单按 GET 提交到同一地址。
#[component]
fn HomeSearch() -> impl IntoView {
    let query = RwSignal::new(String::new());
    view! {
        <form
            class="home-search"
            role="search"
            action="/search"
            method="get"
            on:submit=move |ev| {
                ev.prevent_default();
                let route = Route::Search {
                    q: query.get_untracked().trim().to_string(),
                };
                crate::navigation::push(&route.to_string());
            }
        >
            <Composer>
                <label class="sr-only" for="home-search-input">
                    "搜索文章"
                </label>
                <Icon name=IconName::Search size=18/>
                <ComposerInput
                    id="home-search-input"
                    search=true
                    placeholder="搜索文章、标签，或一个念头…"
                    value=query
                    oninput=move |value: String| query.set(value)
                />
                <kbd class="kbd home-search-kbd" aria-hidden="true">
                    "⌘K"
                </kbd>
                <ComposerSend label="搜索"/>
            </Composer>
        </form>
    }
}

/// 最新一篇面板。整卡可点；有小节时右栏列出前几节，否则改为单栏并放大标题。
#[component]
fn LatestPanel(post: PostListItem, outline: Option<PostOutline>) -> impl IntoView {
    let class = if outline.is_some() {
        "home-latest"
    } else {
        "home-latest is-single"
    };
    let date = post_date(&post);
    let words = group_digits(post.word_count);
    let summary = post
        .summary
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(|summary| view! { <p class="home-latest-summary">{summary.clone()}</p> });
    let tags = (!post.tags.is_empty())
        .then(|| view! { <p class="home-latest-tags">{post.tags.join(" · ")}</p> });
    let count = outline.as_ref().map(|outline| {
        view! { <span class="home-latest-count">{format!("本文 {} 节 · ", outline.total)}</span> }
    });
    let aside = outline.map(|outline| {
        let rest = (outline.total as usize > outline.headings.len()).then(|| {
            view! {
                <li class="home-latest-outline-rest">
                    {format!("还有 {} 节", outline.total as usize - outline.headings.len())}
                </li>
            }
        });
        let headings = outline
            .headings
            .iter()
            .enumerate()
            .map(|(i, heading)| {
                view! {
                    <li>
                        <span class="home-latest-outline-num" aria-hidden="true">
                            {format!("{:02}", i + 1)}
                        </span>
                        <span>{heading.clone()}</span>
                    </li>
                }
            })
            .collect_view();
        view! {
            <aside class="home-latest-outline" aria-label="本文目录">
                <p class="home-latest-outline-total tabular-nums">
                    {format!("本文 {} 节", outline.total)}
                </p>
                <ol>{headings} {rest}</ol>
            </aside>
        }
    });

    view! {
        <article class=class>
            <div class="home-latest-main">
                <p class="home-latest-meta tabular-nums">
                    <time>{date}</time>
                    {format!(" · {} 分钟 · {words} 字", post.reading_time)}
                </p>
                <h3 class="home-latest-title">
                    <a
                        class="home-latest-link"
                        href=Route::PostDetail {
                            slug: post.slug.clone(),
                        }
                            .to_string()
                        data-vt-post-link=post.id.to_string()
                    >
                        <span data-vt-post-id=post.id.to_string() data-vt-role="title">
                            {post.title.clone()}
                        </span>
                    </a>
                </h3>
                {summary}
                <div class="home-latest-foot">
                    {tags} <span class="home-latest-more" aria-hidden="true">
                        {count} "阅读全文" <Icon name=IconName::ArrowRight size=15/>
                    </span>
                </div>
            </div>
            {aside}
        </article>
    }
}

#[component]
fn HomeIndexHeading(total: Option<i64>) -> impl IntoView {
    view! {
        <div class="home-index-heading">
            <h2 id="home-index-title">
                "全部文章"
                {total
                    .map(|total| {
                        view! { <span class="home-index-total tabular-nums">{total}</span> }
                    })}
            </h2>
            <a class="home-index-archive" href=Route::Archives.to_string()>
                "归档"
                <Icon name=IconName::ArrowUpRight size=14/>
            </a>
        </div>
    }
}

/// 按「年 · 月」分组的文章索引与分页。第 1 页传入的列表已去掉最新一篇。
#[component]
fn HomeIndex(current_page: i32, posts: Vec<PostListItem>, total: i64) -> impl IntoView {
    let first_row = (i64::from(current_page) - 1) * i64::from(POSTS_PER_PAGE);
    let heading = if current_page == 1 {
        view! { <HomeIndexHeading total=Some(total)/> }.into_any()
    } else {
        view! {
            <h2 id="home-index-title" class="sr-only">
                {format!("第 {current_page} 页文章")}
            </h2>
        }
        .into_any()
    };
    let body = if total == 0 {
        view! { <EmptyState title="还没有文章" description="写下第一篇后，它会出现在这里。"/> }
            .into_any()
    } else if posts.is_empty() && first_row >= total {
        view! {
            <div class="home-index-end">
                <EmptyState
                    title="已经翻到最后了"
                    description="这一页没有文章，回到首页看看最近写下的吧。"
                />
                <a class="btn btn-secondary" href=Route::Home.to_string()>
                    "回到首页"
                </a>
            </div>
        }
        .into_any()
    } else {
        let groups = group_by_month(&posts)
            .into_iter()
            .enumerate()
            .map(|(index, group)| {
                let rows = group
                    .posts
                    .into_iter()
                    .map(|post| view! { <li><HomeRow post=post/></li> })
                    .collect_view();
                view! {
                    <div
                        class="home-month home-enter"
                        style=format!("--home-delay: {}ms", index.min(5) * 45)
                    >
                        <h3 class="home-month-label">
                            <span class="tabular-nums">{group.label}</span>
                        </h3>
                        <ul>{rows}</ul>
                    </div>
                }
            })
            .collect_view();
        let prev_route = if current_page - 1 <= 1 {
            Route::Home.to_string()
        } else {
            Route::HomePage {
                page: current_page - 1,
            }
            .to_string()
        };
        let next_route = Route::HomePage {
            page: current_page + 1,
        }
        .to_string();
        view! {
            {groups}
            <Pagination
                current_page=current_page
                total=total
                per_page=POSTS_PER_PAGE
                prev_route=prev_route
                next_route=next_route
            />
        }
        .into_any()
    };
    view! {
        <section
            id="home-posts"
            class="home-index"
            tabindex="-1"
            aria-labelledby="home-index-title"
        >
            <span hidden data-vt-list="true"></span>
            {heading} {body}
        </section>
    }
}

#[component]
fn HomeRow(post: PostListItem) -> impl IntoView {
    let date = post
        .published_at
        .unwrap_or(post.created_at)
        .format("%m.%d")
        .to_string();
    let summary = post
        .summary
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(|summary| view! { <span class="home-row-summary">{summary.clone()}</span> });
    view! {
        <a
            class="home-row"
            href=Route::PostDetail {
                slug: post.slug.clone(),
            }
                .to_string()
            data-vt-post-link=post.id.to_string()
        >
            <time class="home-row-date tabular-nums">{date}</time>
            <span class="home-row-main">
                <span
                    class="home-row-title"
                    data-vt-post-id=post.id.to_string()
                    data-vt-role="title"
                >
                    {post.title.clone()}
                </span>
                {summary}
            </span>
            <span class="home-row-meta tabular-nums">
                {format!("{} 分钟", post.reading_time)}
                <span class="home-row-arrow">
                    <Icon name=IconName::ArrowRight size=15/>
                </span>
            </span>
        </a>
    }
}

/// 尾部三格：最近笔记、常去标签、去别处走走。
#[component]
fn HomeCorners(notes: Vec<Note>, tags: Vec<Tag>) -> impl IntoView {
    let notes_body = if notes.is_empty() {
        view! { <p class="home-corner-empty">"还没有公开的笔记。"</p> }.into_any()
    } else {
        let items = notes
            .into_iter()
            .map(|note| {
                let href = Route::NoteDetail {
                    slug: note.slug.clone(),
                    book: String::new(),
                }
                .to_string();
                let date = note
                    .published_at
                    .unwrap_or(note.updated_at)
                    .format("%m.%d")
                    .to_string();
                view! {
                    <li>
                        <a href=href>
                            <span class="home-corner-note-title">{note.display_title()}</span>
                            <time class="tabular-nums">{date}</time>
                        </a>
                    </li>
                }
            })
            .collect_view();
        view! { <ul class="home-corner-notes">{items}</ul> }.into_any()
    };
    let tags_body = if tags.is_empty() {
        view! { <p class="home-corner-empty">"还没有标签。"</p> }.into_any()
    } else {
        let items = tags
            .into_iter()
            .map(|tag| {
                view! {
                    <a
                        href=Route::TagDetail {
                            tag: tag.name.clone(),
                        }
                            .to_string()
                    >
                        {tag.name.clone()}
                        <span class="tabular-nums">{tag.post_count}</span>
                    </a>
                }
            })
            .collect_view();
        view! { <div class="home-corner-tags">{items}</div> }.into_any()
    };

    view! {
        <section class="home-corners home-enter" aria-label="更多入口">
            <div class="home-corner">
                <div class="home-corner-head">
                    <h2>"最近笔记"</h2>
                    <a class="home-corner-more" href=Route::Notes.to_string()>
                        "全部"
                    </a>
                </div>
                {notes_body}
            </div>
            <div class="home-corner">
                <div class="home-corner-head">
                    <h2>"常去标签"</h2>
                    <a class="home-corner-more" href=Route::Archives.to_string()>
                        "全部"
                    </a>
                </div>
                {tags_body}
            </div>
            <div class="home-corner">
                <div class="home-corner-head">
                    <h2>"去别处走走"</h2>
                </div>
                <ul class="home-corner-links">
                    <li>
                        <HomeCornerLink to=Route::Friends label="友链"/>
                    </li>
                    <li>
                        <HomeCornerLink to=Route::About label="关于"/>
                    </li>
                    <li>
                        <HomeCornerLink to=Route::Changelog label="更新日志"/>
                    </li>
                    <li>
                        <HomeCornerLink to=Route::WritingGuide label="写作指南"/>
                    </li>
                </ul>
            </div>
        </section>
    }
}

#[component]
fn HomeCornerLink(to: Route, label: &'static str) -> impl IntoView {
    view! {
        <a class="home-corner-link" href=to.to_string()>
            {label}
            <Icon name=IconName::ArrowUpRight size=14/>
        </a>
    }
}

/// 文章区加载失败。重试在卡片内完成，成功后回填，不触发整页骨架。
#[component]
fn HomePostsError(
    current_page: i32,
    #[prop(into)] on_recovered: Callback<PostListResponse>,
) -> impl IntoView {
    let retrying = RwSignal::new(false);
    let retry_failed = RwSignal::new(false);
    let on_retry = Callback::new(move |()| {
        retrying.set(true);
        retry_failed.set(false);
        leptos::task::spawn_local(async move {
            let result = list_published_posts(current_page, POSTS_PER_PAGE).await;
            // 路由切走后本卡片已卸载，信号随之释放，try_* 静默跳过。
            if retrying.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(response) => on_recovered.run(response),
                Err(_) => {
                    let _ = retry_failed.try_set(true);
                }
            }
        });
    });
    view! {
        <LoadError
            title="文章暂时没能加载"
            class="home-error"
            retrying=retrying
            retry_failed=retry_failed
            on_retry=on_retry
        />
    }
}

fn post_time(post: &PostListItem) -> DateTime<Utc> {
    post.published_at.unwrap_or(post.created_at)
}

fn post_date(post: &PostListItem) -> String {
    post_time(post).format("%Y.%m.%d").to_string()
}

#[derive(Debug, PartialEq)]
struct MonthGroup {
    /// 「2026 · 10」。
    label: String,
    posts: Vec<PostListItem>,
}

/// 列表已按发布时间降序；相邻同月的文章归为一组。
fn group_by_month(posts: &[PostListItem]) -> Vec<MonthGroup> {
    let mut groups: Vec<MonthGroup> = Vec::new();
    for post in posts {
        let time = post_time(post);
        let label = format!("{} · {:02}", time.year(), time.month());
        match groups.last_mut() {
            Some(group) if group.label == label => group.posts.push(post.clone()),
            _ => groups.push(MonthGroup {
                label,
                posts: vec![post.clone()],
            }),
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn post(id: i32, y: i32, m: u32, d: u32) -> PostListItem {
        let at = Utc.with_ymd_and_hms(y, m, d, 8, 0, 0).unwrap();
        PostListItem {
            id,
            author_id: 1,
            title: format!("文章 {id}"),
            slug: format!("post-{id}"),
            summary: None,
            status: crate::models::post::PostStatus::Published,
            published_at: Some(at),
            created_at: at,
            updated_at: at,
            deleted_at: None,
            tags: Vec::new(),
            cover_image: None,
            reading_time: 3,
            word_count: 900,
        }
    }

    #[test]
    fn groups_adjacent_posts_by_month() {
        let posts = [
            post(1, 2026, 10, 15),
            post(2, 2026, 9, 16),
            post(3, 2026, 9, 10),
            post(4, 2025, 9, 1),
        ];
        let groups = group_by_month(&posts);
        let labels: Vec<_> = groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["2026 · 10", "2026 · 09", "2025 · 09"]);
        assert_eq!(groups[1].posts.len(), 2);
    }
}
