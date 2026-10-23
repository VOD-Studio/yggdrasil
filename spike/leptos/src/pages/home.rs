//! 首页「书桌」与分页首页。
//!
//! 第 1 页：状态行、站点标语、搜索与常读标签、最新一篇（右侧目录预览）、按「年 · 月」
//! 分组的文章索引、尾部三格。第 2 页起只有页标题与索引。
//!
//! 路由数据（[`HomeRoute::data`]）在主包里同步建好全部资源，视图在懒加载分包里；
//! SSR 一次取齐，水合时直接复用序列化结果。`/` 与 `/page/:page` 共用本路由，
//! 页码从路径参数读取，缺省为 1。

use std::fmt::Write as _;

use chrono::{DateTime, Datelike, Utc};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::hooks::{use_location, use_navigate, use_params_map};
use leptos_router::{lazy_route, LazyRoute};

use crate::api::home::{
    get_site_settings, latest_post_outline, list_notes, list_published_posts, list_tags,
};
use crate::components::empty_state::EmptyState;
use crate::components::post::post_meta::group_digits;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::home_skeleton::HomeSkeleton;
use crate::components::ui::{
    Composer, ComposerInput, ComposerSend, Icon, IconName, LoadError, Pagination,
};
use crate::error::AppError;
use crate::models::home::{
    HomeTagline, PostListResponse, PostOutline, SiteSettings, TagListResponse,
};
use crate::models::note::{Note, NoteFilter, NotePage};
use crate::models::post::{PostListItem, Tag};

const POSTS_PER_PAGE: i32 = 10;
/// 搜索框下「常读」标签数。
const TOP_TAGS: usize = 6;
/// 尾部「常去标签」数。
const TAG_CLOUD: usize = 10;
/// 尾部「最近笔记」数。
const RECENT_NOTES: usize = 4;

/// 首页路由。第 1 页的开场资源（标签、站点配置、目录预览、笔记）只在第 1 页请求，
/// 其它页返回 `None`，不多查库。
pub struct HomeRoute {
    page: Memo<i32>,
    posts: Resource<Result<PostListResponse, AppError>>,
    tags: Resource<Option<Result<TagListResponse, AppError>>>,
    settings: Resource<Option<Result<SiteSettings, AppError>>>,
    outline: Resource<Option<Result<Option<PostOutline>, AppError>>>,
    notes: Resource<Option<Result<NotePage, AppError>>>,
}

#[lazy_route]
impl LazyRoute for HomeRoute {
    fn data() -> Self {
        let params = use_params_map();
        let page = Memo::new(move |_| {
            params
                .read()
                .get("page")
                .and_then(|p| p.parse::<i32>().ok())
                .unwrap_or(1)
                .max(1)
        });
        let desk = move || page.get() == 1;
        Self {
            page,
            posts: Resource::new(
                move || page.get(),
                |page| list_published_posts(page, POSTS_PER_PAGE),
            ),
            tags: Resource::new(desk, |desk| async move {
                if desk {
                    Some(list_tags().await)
                } else {
                    None
                }
            }),
            settings: Resource::new(desk, |desk| async move {
                if desk {
                    Some(get_site_settings().await)
                } else {
                    None
                }
            }),
            outline: Resource::new(desk, |desk| async move {
                if desk {
                    Some(latest_post_outline().await)
                } else {
                    None
                }
            }),
            notes: Resource::new(desk, |desk| async move {
                if desk {
                    Some(list_notes(NoteFilter::default()).await)
                } else {
                    None
                }
            }),
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
        let title = move || {
            let current_page = page.get();
            if current_page == 1 {
                "Yggdrasil Blog".to_string()
            } else {
                format!("第 {current_page} 页 · Yggdrasil")
            }
        };
        view! {
            <Title text=title/>
            // 原型专用：分包内的发布标识，滚动发布验证据此判断加载的是哪一版分包。
            <span hidden data-spike-build=crate::BUILD_ID></span>
            <Suspense fallback=move || view! {
                <DelayedSkeleton>
                    <HomeSkeleton current_page=page.get_untracked()/>
                </DelayedSkeleton>
            }>
                {move || Suspend::new(async move {
                    let current_page = page.get();
                    let listing = posts.await;
                    if current_page != 1 {
                        // 页码变化时资源重新挂起，整块重建，旧页的重试状态随之卸载。
                        return view! { <HomeArchive current_page=current_page listing=listing/> }.into_any();
                    }
                    // 开场资源失败时退回默认值：标签与笔记为空，标语用默认文案。
                    let tags = ranked_tags(
                        tags.await
                            .and_then(Result::ok)
                            .map(|r| r.tags)
                            .unwrap_or_default(),
                    );
                    let tagline = settings
                        .await
                        .and_then(Result::ok)
                        .unwrap_or_default()
                        .home_tagline();
                    let outline = outline.await.and_then(Result::ok).flatten();
                    let notes: Vec<Note> = notes
                        .await
                        .and_then(Result::ok)
                        .map(|page| page.notes.into_iter().take(RECENT_NOTES).collect())
                        .unwrap_or_default();
                    view! {
                        <HomeDesk listing=listing tags=tags tagline=tagline outline=outline notes=notes/>
                    }
                    .into_any()
                })}
            </Suspense>
        }
        .into_any()
    }
}

/// 第 1 页。文章列表失败时只把文章区换成错误卡片，开场照常显示。
#[component]
fn HomeDesk(
    listing: Result<PostListResponse, AppError>,
    tags: Vec<Tag>,
    tagline: HomeTagline,
    outline: Option<PostOutline>,
    notes: Vec<Note>,
) -> impl IntoView {
    // None 为请求失败；重试成功后回填，状态行与文章区随之更新。
    let response = RwSignal::new(listing.ok());
    let tag_count = tags.len();

    let status = move || {
        response.with(|r| {
            let latest = r.as_ref().and_then(|r| r.posts.first());
            view! {
                <HomeStatus
                    updated=latest.map(post_date)
                    total=r.as_ref().map(|r| r.total)
                    tag_count=tag_count
                />
            }
        })
    };

    let subtitle = (!tagline.subtitle.is_empty()).then_some(tagline.subtitle);
    let top_tags = (!tags.is_empty()).then(|| {
        let chips = tags
            .iter()
            .take(TOP_TAGS)
            .map(|tag| {
                view! {
                    <a class="chip" href=tag_href(&tag.name)>
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

    let listing_view = move || {
        match response.get() {
        Some(PostListResponse { posts, total }) => {
            // 两次读取之间可能有新文章发布；slug 不一致时不显示目录。
            let outline = outline
                .clone()
                .filter(|o| posts.first().is_some_and(|p| p.slug == o.slug));
            let latest = posts.first().cloned().map(|latest| {
                view! {
                    <section class="home-section home-enter" aria-labelledby="home-latest-label">
                        <h2 id="home-latest-label" class="section-label">"最新一篇"</h2>
                        <LatestPanel post=latest outline=outline/>
                    </section>
                }
            });
            let rest: Vec<PostListItem> = posts.into_iter().skip(1).collect();
            view! {
                {latest}
                <HomeIndex current_page=1 posts=rest total=total/>
            }
            .into_any()
        }
        None => view! {
            <section id="home-posts" class="home-index" tabindex="-1" aria-labelledby="home-index-title">
                <span hidden=true data-vt-list="true"></span>
                <HomeIndexHeading total=None/>
                <HomePostsError
                    current_page=1
                    on_recovered=Callback::new(move |r: PostListResponse| response.set(Some(r)))
                />
            </section>
        }
        .into_any(),
    }
    };

    let cloud: Vec<Tag> = tags.into_iter().take(TAG_CLOUD).collect();

    view! {
        <div class="home">
            <section class="home-hero home-enter" aria-labelledby="home-title">
                {status}
                <h1 id="home-title" class="home-title">
                    {tagline.title}
                    {subtitle.map(|s| view! {
                        <br/>
                        <span class="home-title-muted">{s}</span>
                    })}
                </h1>
                <p class="home-intro">{tagline.intro}</p>
                <HomeSearch/>
                {top_tags}
            </section>
            {listing_view}
            <HomeCorners notes=notes tags=cloud/>
        </div>
    }
}

/// 第 2 页起：页标题与索引。
#[component]
fn HomeArchive(current_page: i32, listing: Result<PostListResponse, AppError>) -> impl IntoView {
    let response = RwSignal::new(listing.ok());
    let listing_view = move || match response.get() {
        Some(PostListResponse { posts, total }) => {
            view! { <HomeIndex current_page=current_page posts=posts total=total/> }.into_any()
        }
        None => view! {
            <section id="home-posts" class="home-index" tabindex="-1" aria-label="文章列表">
                <span hidden=true data-vt-list="true"></span>
                <HomePostsError
                    current_page=current_page
                    on_recovered=Callback::new(move |r: PostListResponse| response.set(Some(r)))
                />
            </section>
        }
        .into_any(),
    };

    view! {
        <div class="home home-paged">
            <header class="home-paged-header home-enter">
                <a class="home-back" href="/">
                    <Icon name=IconName::ArrowLeft size=15/>
                    "回到首页"
                </a>
                <h1 class="home-paged-title">"全部文章"</h1>
                <p class="home-paged-page">{format!("第 {current_page} 页")}</p>
            </header>
            {listing_view}
        </div>
    }
}

/// 状态行：Clay 呼吸点、最近更新、篇数与标签数。篇数链到下方索引。
#[component]
fn HomeStatus(updated: Option<String>, total: Option<i64>, tag_count: usize) -> impl IntoView {
    view! {
        <p class="home-status">
            <span class="home-pulse" aria-hidden="true"></span>
            {updated.map(|date| view! {
                <span>"最近更新 "<time class="tabular-nums">{date}</time></span>
            })}
            {total.map(|total| view! {
                <span class="home-status-sep" aria-hidden="true">"·"</span>
                <a class="home-status-link" href="#home-posts">
                    <span class="tabular-nums">{total}</span>
                    " 篇文章"
                </a>
            })}
            {(tag_count > 0).then(|| view! {
                <span class="home-status-sep" aria-hidden="true">"·"</span>
                <span><span class="tabular-nums">{tag_count}</span>" 个标签"</span>
            })}
        </p>
    }
}

/// 开场搜索框：提交进入全文搜索页。没有脚本时表单按 GET 提交到同一地址。
#[component]
fn HomeSearch() -> impl IntoView {
    let navigate = use_navigate();
    let query = RwSignal::new(String::new());
    view! {
        <form
            class="home-search"
            role="search"
            action="/search"
            method="get"
            on:submit=move |ev| {
                ev.prevent_default();
                let q = query.get_untracked();
                navigate(
                    &format!("/search?q={}", encode_component(q.trim())),
                    Default::default(),
                );
            }
        >
            <Composer>
                <label class="sr-only" for="home-search-input">"搜索文章"</label>
                <Icon name=IconName::Search size=18/>
                <ComposerInput
                    id="home-search-input"
                    search=true
                    placeholder="搜索文章、标签，或一个念头…"
                    value=query
                    oninput=move |value: String| query.set(value)
                />
                <kbd class="kbd home-search-kbd" aria-hidden="true">"⌘K"</kbd>
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
    let id = post.id.to_string();
    let summary = post.summary.clone().filter(|s| !s.trim().is_empty());
    let tags = (!post.tags.is_empty()).then(|| post.tags.join(" · "));
    let count = outline.as_ref().map(|o| {
        view! { <span class="home-latest-count">{format!("本文 {} 节 · ", o.total)}</span> }
    });
    let aside = outline.map(|outline| {
        let rest = (outline.total as usize).saturating_sub(outline.headings.len());
        let items = outline
            .headings
            .into_iter()
            .enumerate()
            .map(|(i, heading)| {
                view! {
                    <li>
                        <span class="home-latest-outline-num" aria-hidden="true">{format!("{:02}", i + 1)}</span>
                        <span>{heading}</span>
                    </li>
                }
            })
            .collect_view();
        view! {
            <aside class="home-latest-outline" aria-label="本文目录">
                <p class="home-latest-outline-total tabular-nums">{format!("本文 {} 节", outline.total)}</p>
                <ol>
                    {items}
                    {(rest > 0).then(|| view! {
                        <li class="home-latest-outline-rest">{format!("还有 {rest} 节")}</li>
                    })}
                </ol>
            </aside>
        }
    });

    let link_id = id.clone();
    view! {
        <article class=class>
            <div class="home-latest-main">
                <p class="home-latest-meta tabular-nums">
                    <time>{date}</time>
                    {format!(" · {} 分钟 · {} 字", post.reading_time, words)}
                </p>
                <h3 class="home-latest-title">
                    <a class="home-latest-link" href=post_href(&post.slug) data-vt-post-link=link_id>
                        <span data-vt-post-id=id data-vt-role="title">{post.title}</span>
                    </a>
                </h3>
                {summary.map(|s| view! { <p class="home-latest-summary">{s}</p> })}
                <div class="home-latest-foot">
                    {tags.map(|t| view! { <p class="home-latest-tags">{t}</p> })}
                    <span class="home-latest-more" aria-hidden="true">
                        {count}
                        "阅读全文"
                        <Icon name=IconName::ArrowRight size=15/>
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
                {total.map(|total| view! {
                    <span class="home-index-total tabular-nums">{total}</span>
                })}
            </h2>
            <a class="home-index-archive" href="/archives">
                "归档"
                <Icon name=IconName::ArrowUpRight size=14/>
            </a>
        </div>
    }
}

/// 按「年 · 月」分组的文章索引与分页。第 1 页传入的列表已去掉最新一篇。
#[component]
fn HomeIndex(current_page: i32, posts: Vec<PostListItem>, total: i64) -> impl IntoView {
    let groups = group_by_month(&posts);
    let first_row = (i64::from(current_page) - 1) * i64::from(POSTS_PER_PAGE);

    let heading = if current_page == 1 {
        view! { <HomeIndexHeading total=Some(total)/> }.into_any()
    } else {
        view! {
            <h2 id="home-index-title" class="sr-only">{format!("第 {current_page} 页文章")}</h2>
        }
        .into_any()
    };

    let body = if total == 0 {
        view! {
            <EmptyState title="还没有文章" description="写下第一篇后，它会出现在这里。"/>
        }
        .into_any()
    } else if posts.is_empty() && first_row >= total {
        view! {
            <div class="home-index-end">
                <EmptyState
                    title="已经翻到最后了"
                    description="这一页没有文章，回到首页看看最近写下的吧。"
                />
                <a class="btn btn-secondary" href="/">"回到首页"</a>
            </div>
        }
        .into_any()
    } else {
        let months = groups
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
            "/".to_string()
        } else {
            format!("/page/{}", current_page - 1)
        };
        let next_route = format!("/page/{}", current_page + 1);
        view! {
            {months}
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
        <section id="home-posts" class="home-index" tabindex="-1" aria-labelledby="home-index-title">
            <span hidden=true data-vt-list="true"></span>
            {heading}
            {body}
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
    let id = post.id.to_string();
    let link_id = id.clone();
    let summary = post.summary.clone().filter(|s| !s.trim().is_empty());
    view! {
        <a class="home-row" href=post_href(&post.slug) data-vt-post-link=link_id>
            <time class="home-row-date tabular-nums">{date}</time>
            <span class="home-row-main">
                <span class="home-row-title" data-vt-post-id=id data-vt-role="title">{post.title}</span>
                {summary.map(|s| view! { <span class="home-row-summary">{s}</span> })}
            </span>
            <span class="home-row-meta tabular-nums">
                {format!("{} 分钟", post.reading_time)}
                <span class="home-row-arrow"><Icon name=IconName::ArrowRight size=15/></span>
            </span>
        </a>
    }
}

/// 尾部三格：最近笔记、常去标签、去别处走走。
#[component]
fn HomeCorners(notes: Vec<Note>, tags: Vec<Tag>) -> impl IntoView {
    let notes_view = if notes.is_empty() {
        view! { <p class="home-corner-empty">"还没有公开的笔记。"</p> }.into_any()
    } else {
        let items = notes
            .into_iter()
            .map(|note| {
                let date = note
                    .published_at
                    .unwrap_or(note.updated_at)
                    .format("%m.%d")
                    .to_string();
                view! {
                    <li>
                        <a href=format!("/notes/{}", encode_component(&note.slug))>
                            <span class="home-corner-note-title">{note.display_title()}</span>
                            <time class="tabular-nums">{date}</time>
                        </a>
                    </li>
                }
            })
            .collect_view();
        view! { <ul class="home-corner-notes">{items}</ul> }.into_any()
    };

    let tags_view = if tags.is_empty() {
        view! { <p class="home-corner-empty">"还没有标签。"</p> }.into_any()
    } else {
        let links = tags
            .into_iter()
            .map(|tag| {
                view! {
                    <a href=tag_href(&tag.name)>
                        {tag.name.clone()}
                        <span class="tabular-nums">{tag.post_count}</span>
                    </a>
                }
            })
            .collect_view();
        view! { <div class="home-corner-tags">{links}</div> }.into_any()
    };

    view! {
        <section class="home-corners home-enter" aria-label="更多入口">
            <div class="home-corner">
                <div class="home-corner-head">
                    <h2>"最近笔记"</h2>
                    <a class="home-corner-more" href="/notes">"全部"</a>
                </div>
                {notes_view}
            </div>
            <div class="home-corner">
                <div class="home-corner-head">
                    <h2>"常去标签"</h2>
                    <a class="home-corner-more" href="/archives">"全部"</a>
                </div>
                {tags_view}
            </div>
            <div class="home-corner">
                <div class="home-corner-head"><h2>"去别处走走"</h2></div>
                <ul class="home-corner-links">
                    <li><HomeCornerLink to="/friends" label="友链"/></li>
                    <li><HomeCornerLink to="/about" label="关于"/></li>
                    <li><HomeCornerLink to="/changelog" label="更新日志"/></li>
                    <li><HomeCornerLink to="/about/writing" label="写作指南"/></li>
                </ul>
            </div>
        </section>
    }
}

#[component]
fn HomeCornerLink(to: &'static str, label: &'static str) -> impl IntoView {
    view! {
        <a class="home-corner-link" href=to>
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
    let pathname = use_location().pathname;
    let retrying = RwSignal::new(false);
    let retry_failed = RwSignal::new(false);

    let on_retry = move |_: ()| {
        retrying.set(true);
        retry_failed.set(false);
        let request_path = pathname.get_untracked();
        spawn_local(async move {
            let result = list_published_posts(current_page, POSTS_PER_PAGE).await;
            // Suspense 切换期间旧卡片可能已卸载，也不能回填另一条路由。
            if pathname.try_get_untracked().as_ref() != Some(&request_path) {
                return;
            }
            retrying.set(false);
            match result {
                Ok(response) => on_recovered.run(response),
                Err(_) => retry_failed.set(true),
            }
        });
    };

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

/// 只保留有已发布文章的标签，按篇数降序、同数按名称排列。
/// （移植自主 crate `pages::archives::ranked_tags`。）
fn ranked_tags(mut tags: Vec<Tag>) -> Vec<Tag> {
    tags.retain(|t| t.post_count > 0);
    tags.sort_by(|a, b| {
        b.post_count
            .cmp(&a.post_count)
            .then_with(|| a.name.cmp(&b.name))
    });
    tags
}

fn post_time(post: &PostListItem) -> DateTime<Utc> {
    post.published_at.unwrap_or(post.created_at)
}

fn post_date(post: &PostListItem) -> String {
    post_time(post).format("%Y.%m.%d").to_string()
}

fn post_href(slug: &str) -> String {
    format!("/post/{}", encode_component(slug))
}

fn tag_href(tag: &str) -> String {
    format!("/tags/{}", encode_component(tag))
}

/// 路径段与查询值的百分号编码：只保留 RFC 3986 非保留字符，与 Dioxus 路由生成的链接一致。
fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
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

    #[test]
    fn encodes_non_ascii_path_segments() {
        assert_eq!(encode_component("rust-1.0"), "rust-1.0");
        assert_eq!(
            encode_component("源码 阅读"),
            "%E6%BA%90%E7%A0%81%20%E9%98%85%E8%AF%BB"
        );
    }
}
