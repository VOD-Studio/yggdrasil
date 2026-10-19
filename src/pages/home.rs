//! 首页「书桌」与分页首页。
//!
//! 第 1 页：状态行、站点标语、搜索与常读标签、最新一篇（右侧目录预览）、按「年 · 月」
//! 分组的文章索引、尾部三格。第 2 页起只有页标题与索引。数据都来自已缓存的公开接口，
//! 同一组件内并行发起，SSR 一次取齐。

use chrono::{DateTime, Datelike, Utc};
use dioxus::prelude::*;

use crate::api::notes::list_notes;
use crate::api::posts::{
    latest_post_outline, list_published_posts, list_tags, PostListResponse, PostOutline,
};
use crate::api::settings::get_site_settings;
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::home_skeleton::HomeSkeleton;
use crate::components::ui::{Button, ButtonVariant, Icon, IconName, Pagination};
use crate::models::note::{Note, NoteFilter};
use crate::models::post::{PostListItem, Tag};
use crate::router::Route;

const POSTS_PER_PAGE: i32 = 10;
/// 搜索框下「常读」标签数。
const TOP_TAGS: usize = 6;
/// 尾部「常去标签」数。
const TAG_CLOUD: usize = 10;
/// 尾部「最近笔记」数。
const RECENT_NOTES: usize = 4;

#[component]
pub fn Home() -> Element {
    rsx! { HomePage { page: 1 } }
}

#[component]
pub fn HomePage(page: i32) -> Element {
    let current_page = page.max(1);
    rsx! {
        SuspenseBoundary {
            fallback: move |_| rsx! { DelayedSkeleton { HomeSkeleton { current_page } } },
            if current_page == 1 {
                HomeDesk {}
            } else {
                // 翻页时卸载旧资源和重试任务，避免旧响应覆盖新页。
                HomeArchive { key: "page-{current_page}", current_page }
            }
        }
    }
}

/// 第 1 页。文章列表失败时只把文章区换成错误卡片，开场照常显示。
#[component]
fn HomeDesk() -> Element {
    // 先建齐全部资源再挂起，避免逐个等待。
    let posts = use_server_future(|| list_published_posts(1, POSTS_PER_PAGE));
    let tags = use_server_future(list_tags);
    let settings = use_server_future(get_site_settings);
    let outline = use_server_future(latest_post_outline);
    let notes = use_server_future(|| list_notes(NoteFilter::default()));
    let mut posts = posts?;
    let (tags, settings, outline, notes) = (tags?, settings?, outline?, notes?);

    let tags = ranked_tags(
        tags.read()
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(|r| r.tags.clone())
            .unwrap_or_default(),
    );
    let tagline = settings
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default()
        .home_tagline();
    let notes: Vec<Note> = notes
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|page| page.notes.iter().take(RECENT_NOTES).cloned().collect())
        .unwrap_or_default();
    // 外层 None 尚未取到（已挂起时不会出现），内层 None 为请求失败。
    let listing = posts.read().as_ref().map(|r| r.as_ref().ok().cloned());
    let response = listing.clone().flatten();
    let latest = response.as_ref().and_then(|r| r.posts.first().cloned());
    let total = response.as_ref().map(|r| r.total);
    // 两次读取之间可能有新文章发布；slug 不一致时不显示目录。
    let outline = outline
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .flatten()
        .filter(|o| latest.as_ref().is_some_and(|p| p.slug == o.slug));

    rsx! {
        div { class: "home",
            section { class: "home-hero home-enter", aria_labelledby: "home-title",
                HomeStatus {
                    updated: latest.as_ref().map(post_date),
                    total,
                    tag_count: tags.len(),
                }
                h1 { id: "home-title", class: "home-title",
                    "{tagline.title}"
                    if !tagline.subtitle.is_empty() {
                        br {}
                        span { class: "home-title-muted", "{tagline.subtitle}" }
                    }
                }
                p { class: "home-intro", "{tagline.intro}" }
                HomeSearch {}
                if !tags.is_empty() {
                    div { class: "home-top-tags",
                        span { class: "home-top-tags-label", "常读" }
                        for tag in tags.iter().take(TOP_TAGS) {
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
            match listing {
                Some(Some(PostListResponse { posts, total })) => rsx! {
                    if let Some(latest) = posts.first() {
                        section { class: "home-section home-enter", aria_labelledby: "home-latest-label",
                            h2 { id: "home-latest-label", class: "section-label", "最新一篇" }
                            LatestPanel { post: latest.clone(), outline }
                        }
                    }
                    HomeIndex { current_page: 1, posts: posts.iter().skip(1).cloned().collect::<Vec<_>>(), total }
                },
                Some(None) => rsx! {
                    section { id: "home-posts", class: "home-index", tabindex: "-1", aria_labelledby: "home-index-title",
                        span { hidden: true, "data-vt-list": "true" }
                        HomeIndexHeading { total: None }
                        HomePostsError {
                            current_page: 1,
                            on_recovered: move |response| posts.set(Some(Ok(response))),
                        }
                    }
                },
                None => rsx! {},
            }
            HomeCorners { notes, tags: tags.into_iter().take(TAG_CLOUD).collect::<Vec<_>>() }
        }
    }
}

/// 第 2 页起：页标题与索引。
#[component]
fn HomeArchive(current_page: i32) -> Element {
    let router = dioxus::router::router();
    // Router 会在原生快照前通知旧路由；页码未变时不重新挂起旧列表。
    let requested_page = use_memo(move || match router.current::<Route>() {
        Route::HomePage { page } => page.max(1),
        _ => current_page,
    });
    let mut posts =
        use_server_future(move || list_published_posts(requested_page(), POSTS_PER_PAGE))?;
    let listing = posts.read().as_ref().map(|r| r.as_ref().ok().cloned());

    rsx! {
        div { class: "home home-paged",
            header { class: "home-paged-header home-enter",
                Link { class: "home-back", to: Route::Home {},
                    Icon { name: IconName::ArrowLeft, size: 15 }
                    "回到首页"
                }
                h1 { class: "home-paged-title", "全部文章" }
                p { class: "home-paged-page", "第 {current_page} 页" }
            }
            match listing {
                Some(Some(PostListResponse { posts, total })) => rsx! {
                    HomeIndex { current_page, posts, total }
                },
                Some(None) => rsx! {
                    section { id: "home-posts", class: "home-index", tabindex: "-1", aria_label: "文章列表",
                        span { hidden: true, "data-vt-list": "true" }
                        HomePostsError {
                            current_page,
                            on_recovered: move |response| posts.set(Some(Ok(response))),
                        }
                    }
                },
                None => rsx! {},
            }
        }
    }
}

/// 状态行：Clay 呼吸点、最近更新、篇数与标签数。篇数链到下方索引。
#[component]
fn HomeStatus(updated: Option<String>, total: Option<i64>, tag_count: usize) -> Element {
    rsx! {
        p { class: "home-status",
            span { class: "home-pulse", aria_hidden: "true" }
            if let Some(date) = updated {
                span { "最近更新 " time { class: "tabular-nums", "{date}" } }
            }
            if let Some(total) = total {
                span { class: "home-status-sep", aria_hidden: "true", "·" }
                a { class: "home-status-link", href: "#home-posts",
                    span { class: "tabular-nums", "{total}" }
                    " 篇文章"
                }
            }
            if tag_count > 0 {
                span { class: "home-status-sep", aria_hidden: "true", "·" }
                span { span { class: "tabular-nums", "{tag_count}" } " 个标签" }
            }
        }
    }
}

/// 开场搜索框：提交进入全文搜索页。没有脚本时表单按 GET 提交到同一地址。
#[component]
fn HomeSearch() -> Element {
    let navigator = use_navigator();
    let mut query = use_signal(String::new);
    rsx! {
        form {
            class: "composer home-search",
            role: "search",
            action: "/search",
            method: "get",
            onsubmit: move |evt: FormEvent| {
                evt.prevent_default();
                navigator.push(Route::Search { q: query().trim().to_string() });
            },
            label { class: "sr-only", r#for: "home-search-input", "搜索文章" }
            Icon { name: IconName::Search, size: 18 }
            input {
                id: "home-search-input",
                class: "composer-input",
                r#type: "search",
                name: "q",
                placeholder: "搜索文章、标签，或一个念头…",
                autocomplete: "off",
                "enterkeyhint": "search",
                value: "{query}",
                oninput: move |evt| query.set(evt.value()),
            }
            kbd { class: "kbd home-search-kbd", aria_hidden: "true", "⌘K" }
            button { class: "composer-send", r#type: "submit", aria_label: "搜索",
                Icon { name: IconName::ArrowRight, size: 17 }
            }
        }
    }
}

/// 最新一篇面板。整卡可点；有小节时右栏列出前几节，否则改为单栏并放大标题。
#[component]
fn LatestPanel(post: PostListItem, outline: Option<PostOutline>) -> Element {
    let class = if outline.is_some() {
        "home-latest"
    } else {
        "home-latest is-single"
    };
    let date = post_date(&post);
    let words = group_digits(post.word_count);
    rsx! {
        article { class: "{class}",
            div { class: "home-latest-main",
                p { class: "home-latest-meta tabular-nums",
                    time { "{date}" }
                    " · {post.reading_time} 分钟 · {words} 字"
                }
                h3 { class: "home-latest-title",
                    Link {
                        class: "home-latest-link",
                        to: Route::PostDetail { slug: post.slug.clone() },
                        "data-vt-post-link": "{post.id}",
                        span { "data-vt-post-id": "{post.id}", "data-vt-role": "title", "{post.title}" }
                    }
                }
                if let Some(summary) = post.summary.as_ref().filter(|s| !s.trim().is_empty()) {
                    p { class: "home-latest-summary", "{summary}" }
                }
                div { class: "home-latest-foot",
                    if !post.tags.is_empty() {
                        p { class: "home-latest-tags", {post.tags.join(" · ")} }
                    }
                    span { class: "home-latest-more", aria_hidden: "true",
                        if let Some(outline) = outline.as_ref() {
                            span { class: "home-latest-count", "本文 {outline.total} 节 · " }
                        }
                        "阅读全文"
                        Icon { name: IconName::ArrowRight, size: 15 }
                    }
                }
            }
            if let Some(outline) = outline {
                aside { class: "home-latest-outline", aria_label: "本文目录",
                    p { class: "home-latest-outline-total tabular-nums", "本文 {outline.total} 节" }
                    ol {
                        for (i, heading) in outline.headings.iter().enumerate() {
                            li { key: "{i}",
                                span { class: "home-latest-outline-num", aria_hidden: "true", {format!("{:02}", i + 1)} }
                                span { "{heading}" }
                            }
                        }
                        if outline.total as usize > outline.headings.len() {
                            li { class: "home-latest-outline-rest",
                                "还有 {outline.total as usize - outline.headings.len()} 节"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HomeIndexHeading(total: Option<i64>) -> Element {
    rsx! {
        div { class: "home-index-heading",
            h2 { id: "home-index-title",
                "全部文章"
                if let Some(total) = total {
                    span { class: "home-index-total tabular-nums", "{total}" }
                }
            }
            Link { class: "home-index-archive", to: Route::Archives {},
                "归档"
                Icon { name: IconName::ArrowUpRight, size: 14 }
            }
        }
    }
}

/// 按「年 · 月」分组的文章索引与分页。第 1 页传入的列表已去掉最新一篇。
#[component]
fn HomeIndex(current_page: i32, posts: Vec<PostListItem>, total: i64) -> Element {
    let groups = group_by_month(&posts);
    let first_row = (i64::from(current_page) - 1) * i64::from(POSTS_PER_PAGE);
    rsx! {
        section { id: "home-posts", class: "home-index", tabindex: "-1", aria_labelledby: "home-index-title",
            span { hidden: true, "data-vt-list": "true" }
            if current_page == 1 {
                HomeIndexHeading { total: Some(total) }
            } else {
                h2 { id: "home-index-title", class: "sr-only", "第 {current_page} 页文章" }
            }
            if total == 0 {
                EmptyState { title: "还没有文章", description: "写下第一篇后，它会出现在这里。" }
            } else if posts.is_empty() && first_row >= total {
                div { class: "home-index-end",
                    EmptyState { title: "已经翻到最后了", description: "这一页没有文章，回到首页看看最近写下的吧。" }
                    Link { class: "btn btn-secondary", to: Route::Home {}, "回到首页" }
                }
            } else {
                for (index, group) in groups.into_iter().enumerate() {
                    div { key: "{group.label}", class: "home-month home-enter", style: "--home-delay: {index.min(5) * 45}ms",
                        h3 { class: "home-month-label",
                            span { class: "tabular-nums", "{group.label}" }
                        }
                        ul {
                            for post in group.posts.iter() {
                                li { key: "{post.id}", HomeRow { post: post.clone() } }
                            }
                        }
                    }
                }
                Pagination {
                    variant: "frontend", current_page, total, per_page: POSTS_PER_PAGE,
                    prev_route: if current_page - 1 <= 1 { Route::Home {} } else { Route::HomePage { page: current_page - 1 } },
                    next_route: Route::HomePage { page: current_page + 1 },
                    unit: "篇",
                }
            }
        }
    }
}

#[component]
fn HomeRow(post: PostListItem) -> Element {
    let date = post
        .published_at
        .unwrap_or(post.created_at)
        .format("%m.%d")
        .to_string();
    rsx! {
        Link {
            class: "home-row",
            to: Route::PostDetail { slug: post.slug.clone() },
            "data-vt-post-link": "{post.id}",
            time { class: "home-row-date tabular-nums", "{date}" }
            span { class: "home-row-main",
                span { class: "home-row-title", "data-vt-post-id": "{post.id}", "data-vt-role": "title", "{post.title}" }
                if let Some(summary) = post.summary.as_ref().filter(|s| !s.trim().is_empty()) {
                    span { class: "home-row-summary", "{summary}" }
                }
            }
            span { class: "home-row-meta tabular-nums",
                "{post.reading_time} 分钟"
                span { class: "home-row-arrow", Icon { name: IconName::ArrowRight, size: 15 } }
            }
        }
    }
}

/// 尾部三格：最近笔记、常去标签、去别处走走。
#[component]
fn HomeCorners(notes: Vec<Note>, tags: Vec<Tag>) -> Element {
    rsx! {
        section { class: "home-corners home-enter", aria_label: "更多入口",
            div { class: "home-corner",
                div { class: "home-corner-head",
                    h2 { "最近笔记" }
                    Link { class: "home-corner-more", to: Route::Notes {}, "全部" }
                }
                if notes.is_empty() {
                    p { class: "home-corner-empty", "还没有公开的笔记。" }
                } else {
                    ul { class: "home-corner-notes",
                        for note in notes {
                            li { key: "{note.id}",
                                Link { to: Route::NoteDetail { slug: note.slug.clone() },
                                    span { class: "home-corner-note-title", {note.display_title()} }
                                    time { class: "tabular-nums", {note.published_at.unwrap_or(note.updated_at).format("%m.%d").to_string()} }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "home-corner",
                div { class: "home-corner-head",
                    h2 { "常去标签" }
                    Link { class: "home-corner-more", to: Route::Archives {}, "全部" }
                }
                if tags.is_empty() {
                    p { class: "home-corner-empty", "还没有标签。" }
                } else {
                    div { class: "home-corner-tags",
                        for tag in tags {
                            Link { key: "{tag.id}", to: Route::TagDetail { tag: tag.name.clone() },
                                "{tag.name}"
                                span { class: "tabular-nums", "{tag.post_count}" }
                            }
                        }
                    }
                }
            }
            div { class: "home-corner",
                div { class: "home-corner-head", h2 { "去别处走走" } }
                ul { class: "home-corner-links",
                    li { HomeCornerLink { to: Route::Friends {}, label: "友链" } }
                    li { HomeCornerLink { to: Route::About {}, label: "关于" } }
                    li { HomeCornerLink { to: Route::Changelog {}, label: "更新日志" } }
                    li { HomeCornerLink { to: Route::WritingGuide {}, label: "写作指南" } }
                }
            }
        }
    }
}

#[component]
fn HomeCornerLink(to: Route, label: &'static str) -> Element {
    rsx! {
        Link { class: "home-corner-link", to,
            "{label}"
            Icon { name: IconName::ArrowUpRight, size: 14 }
        }
    }
}

/// 区块加载失败卡片。重试在卡片内完成，成功后回填资源，不触发整页骨架。
#[component]
fn HomePostsError(current_page: i32, on_recovered: EventHandler<PostListResponse>) -> Element {
    let router = dioxus::router::router();
    let mut retrying = use_signal(|| false);
    let mut retry_failed = use_signal(|| false);

    rsx! {
        div { class: "home-error", role: "group", aria_labelledby: "home-error-title",
            div { class: "home-error-copy",
                h3 { id: "home-error-title", "文章暂时没能加载" }
                p { role: "status", aria_live: "polite", aria_atomic: "true",
                    if retry_failed() {
                        "这次仍未加载成功，稍后再试一次吧。"
                    } else {
                        "网络或服务出了点问题，稍后再试一次。"
                    }
                    if retrying() {
                        span { class: "sr-only", "正在重新加载文章，请稍候。" }
                    }
                }
            }
            Button {
                variant: ButtonVariant::Secondary,
                icon: IconName::RotateCw,
                loading: retrying(),
                class: "home-error-retry",
                onclick: move |_| {
                    if retrying() {
                        return;
                    }
                    retrying.set(true);
                    retry_failed.set(false);
                    let request_route = router.current::<Route>();
                    spawn(async move {
                        let result = list_published_posts(current_page, POSTS_PER_PAGE).await;
                        // Suspense 切换期间旧卡片可能尚未卸载，也不能回填另一条路由。
                        if router.current::<Route>() != request_route {
                            return;
                        }
                        retrying.set(false);
                        match result {
                            Ok(response) => on_recovered.call(response),
                            Err(_) => retry_failed.set(true),
                        }
                    });
                },
                "重新加载"
            }
        }
    }
}

/// 只保留有已发布文章的标签，按篇数降序、同数按名称排列。
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

/// 千位分隔：`4866` → `4,866`。
fn group_digits(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
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

    fn tag(id: i32, name: &str, post_count: i64) -> Tag {
        Tag {
            id,
            name: name.into(),
            post_count,
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
    fn groups_thousands() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(866), "866");
        assert_eq!(group_digits(4866), "4,866");
        assert_eq!(group_digits(1_234_567), "1,234,567");
    }
}
