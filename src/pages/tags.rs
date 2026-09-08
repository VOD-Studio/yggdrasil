//! 标签专题：刊物式开场、可排序文章流与关联主题。
//! 保留 SSR 取数（最近 200 篇）；每个标签拥有独立的请求和浏览状态。

use std::collections::{BTreeMap, BTreeSet};

use dioxus::prelude::*;

use crate::api::posts::{get_posts_by_tag, PostListResponse};
use crate::components::post_card::PostCard;
use crate::components::skeletons::tags_skeleton::TagPostsLoading;
use crate::models::post::PostListItem;
use crate::router::Route;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TagSortOrder {
    Latest,
    Earliest,
}

impl TagSortOrder {
    fn value(self) -> &'static str {
        match self {
            Self::Latest => "latest",
            Self::Earliest => "earliest",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Latest => "最新",
            Self::Earliest => "最早",
        }
    }

    fn opening(self) -> &'static str {
        match self {
            Self::Latest => "最近写下",
            Self::Earliest => "从最初读起",
        }
    }
}

fn sorted_posts(posts: &[PostListItem], order: TagSortOrder) -> Vec<&PostListItem> {
    let mut sorted: Vec<_> = posts.iter().collect();
    sorted.sort_by_key(|post| (post.published_at.unwrap_or(post.created_at), post.id));
    if order == TagSortOrder::Latest {
        sorted.reverse();
    }
    sorted
}

/// 每篇文章对同一关联标签只计一次，数量相同按名称排序，确保 SSR 与浏览器一致。
fn related_tags(posts: &[PostListItem], current_tag: &str) -> Vec<String> {
    let mut counts = BTreeMap::new();
    for post in posts {
        for tag in post.tags.iter().collect::<BTreeSet<_>>() {
            if tag != current_tag && !tag.trim().is_empty() {
                *counts.entry(tag).or_insert(0_usize) += 1;
            }
        }
    }
    let mut tags: Vec<_> = counts.into_iter().collect();
    tags.sort_by(|(a, a_count), (b, b_count)| b_count.cmp(a_count).then_with(|| a.cmp(b)));
    tags.into_iter()
        .take(6)
        .map(|(tag, _)| tag.clone())
        .collect()
}

#[component]
pub fn TagDetail(tag: String) -> Element {
    rsx! {
        div { class: "tag-page",
            TagIntro { tag: tag.clone() }
            // 单元素 keyed 列表使同路由变体切换时卸载旧请求、重置排序。
            // key 放在 Suspense 外面，旧结果不会在新标题下短暂出现。
            for current_tag in std::iter::once(tag) {
                div { key: "{current_tag}", id: "tag-posts", class: "tag-workspace",
                    SuspenseBoundary {
                        fallback: |_| rsx! { TagPostsLoading {} },
                        TagDetailContent { tag: current_tag }
                    }
                }
            }
        }
    }
}

/// 路由兜底和实际页面共用开场；已知的主题名称无需等待文章请求。
#[component]
pub(crate) fn TagIntro(tag: String) -> Element {
    rsx! {
        header { class: "tag-intro",
            Link { class: "tag-text-link tag-back", to: Route::Archives {},
                span { class: "tag-arrow", aria_hidden: "true", "←" }
                span { "归档与标签" }
            }
            div { class: "tag-hero",
                div { class: "tag-hero-copy tag-enter",
                    p { class: "tag-eyebrow",
                        span { class: "tag-eyebrow-mark", aria_hidden: "true", "#" }
                        "TOPIC COLLECTION" span { class: "tag-eyebrow-translation", " / 主题手记" }
                    }
                    h1 { "{tag}" }
                    p { class: "tag-description", "沿着一个主题，拾起散落的思考。" br {} "让有关的文字，在这里慢慢生长。" }
                }
                div { class: "tag-hero-art tag-enter", style: "--tag-delay: 100ms", aria_hidden: "true",
                    TagIllustration {}
                    span { class: "tag-art-caption", "A BRANCH OF THE GARDEN" }
                }
            }
        }
    }
}

#[component]
fn TagIllustration(#[props(default = false)] paused: bool) -> Element {
    rsx! {
        svg { class: "tag-illustration", view_box: "0 0 240 240", fill: "none", "aria-hidden": "true",
            circle { cx: "120", cy: "120", r: "104", stroke: "currentColor", stroke_width: "0.7", stroke_dasharray: "1 7", opacity: "0.3" }
            path { d: "M120 7V23M120 217V233M7 120H23M217 120H233", stroke: "currentColor", stroke_width: "0.7", opacity: "0.4" }
            g { class: "tag-illustration-pages", stroke: "currentColor", stroke_linejoin: "round",
                path { d: "M65 63L164 48L185 187L86 202Z", stroke_width: "0.9", opacity: "0.22" }
                path { d: "M58 51H168V191H58Z", stroke_width: "1", opacity: "0.48" }
                path { d: "M70 51V191M83 170H145M83 178H120", stroke_width: "0.8", opacity: "0.28" }
                path { d: "M141 51V91L151 83L161 91V51", stroke_width: "1.1", fill: "currentColor", fill_opacity: "0.06" }
            }
            g { class: "tag-illustration-sprout", stroke: "currentColor", stroke_width: "1.4", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M112 156V109M99 156H126" }
                path { d: "M112 138C92 138 86 124 88 111C105 112 115 120 112 138ZM112 119C111 100 122 92 136 94C136 109 127 119 112 119Z" }
                path { d: "M112 137L97 121M112 119L127 103", opacity: "0.65" }
            }
            if paused {
                circle { cx: "179", cy: "174", r: "23", fill: "var(--color-paper-theme)", stroke: "currentColor", stroke_width: "0.8" }
                path { d: "M174 167V181M184 167V181", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round" }
            } else {
                path { d: "M184 101V113M178 107H190M49 151V159M45 155H53", stroke: "currentColor", stroke_width: "1", stroke_linecap: "round", opacity: "0.55" }
                circle { cx: "184", cy: "145", r: "2", fill: "currentColor", opacity: "0.5" }
            }
        }
    }
}

#[component]
fn TagDetailContent(tag: String) -> Element {
    let mut order = use_signal(|| TagSortOrder::Latest);
    // 父组件按标签 remount，因此闭包中的 tag 在此作用域内不会过期。
    let request_tag = tag.clone();
    let mut posts_res = use_server_future(move || get_posts_by_tag(request_tag.clone()))?;
    let posts_data = posts_res.read();

    match posts_data.as_ref() {
        Some(Ok(PostListResponse { posts, total })) => {
            let sorted = sorted_posts(posts, order());
            let related = related_tags(posts, &tag);
            let recent = posts
                .iter()
                .max_by_key(|post| post.published_at.unwrap_or(post.created_at));
            let truncated = *total > posts.len() as i64;
            rsx! {
                section { class: "tag-results", aria_label: "主题文章", aria_busy: "false",
                    p { class: "sr-only", role: "status", aria_atomic: "true",
                        "已加载 {posts.len()} 篇文章，按{order().label()}排序。"
                    }
                    div { class: "tag-toolbar tag-enter",
                        div { class: "tag-collection-meta",
                            p { class: "tag-count", "共 " strong { "{total}" } " 篇文章" }
                            if let Some(post) = recent {
                                p { class: "tag-recent", "最近收录 " time { datetime: post.formatted_date(), "{post.formatted_date()}" } }
                            }
                        }
                        if posts.len() > 1 {
                            div { class: "tag-sort-group",
                                if truncated { span { class: "tag-sort-note", "当前列表排序" } }
                                div { class: "tag-sort", role: "group", aria_label: "文章排序", "data-order": order().value(),
                                    for choice in [TagSortOrder::Latest, TagSortOrder::Earliest] {
                                        button {
                                            r#type: "button",
                                            aria_pressed: (order() == choice).to_string(),
                                            onclick: move |_| { if order() != choice { order.set(choice); } },
                                            "{choice.label()}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if truncated {
                        p { class: "tag-limit-note", "共 {total} 篇，当前展示最近 {posts.len()} 篇；排序和相关标签基于当前列表。" }
                    }
                    if posts.is_empty() {
                        TagStatus {}
                    } else {
                        div { class: "tag-post-list", "data-order": order().value(),
                            for (index, post) in sorted.into_iter().enumerate() {
                                div {
                                    key: "{post.id}",
                                    class: if index == 0 { "tag-entry tag-featured tag-enter" } else { "tag-entry tag-enter" },
                                    style: "--tag-delay: {index.min(5) * 50}ms",
                                    if index == 0 {
                                        span { class: "tag-featured-label", span { aria_hidden: "true", "01 /" } "{order().opening()}" }
                                    } else {
                                        span { class: "tag-entry-number", aria_hidden: "true", {format!("{:02}", index + 1)} }
                                    }
                                    PostCard { post: post.clone(), compact: true }
                                    span { class: "tag-entry-arrow", aria_hidden: "true", "↗" }
                                }
                            }
                        }
                    }
                    if !related.is_empty() {
                        nav { class: "tag-related tag-enter", aria_label: "相关标签",
                            div { class: "tag-related-heading",
                                div {
                                    p { class: "tag-eyebrow", "CONNECTED NOTES / 相邻枝叶" }
                                    h2 { "沿着枝叶，继续阅读" }
                                }
                                Link { class: "tag-text-link", to: Route::Archives {},
                                    span { "全部标签" } span { class: "tag-arrow", aria_hidden: "true", "↗" }
                                }
                            }
                            div { class: "tag-related-links",
                                for name in related {
                                    Link { key: "{name}", class: "tag-related-link", to: Route::TagDetail { tag: name.clone() },
                                        span { class: "tag-related-mark", aria_hidden: "true", "#" }
                                        span { "{name}" }
                                        span { class: "tag-arrow", aria_hidden: "true", "↗" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Some(Err(_)) => rsx! {
            section { class: "tag-results", aria_label: "主题文章", aria_busy: "false",
                TagStatus { failed: true, on_retry: move |_| {
                    if !posts_res.pending() { posts_res.restart(); }
                } }
            }
        },
        _ => rsx! { TagPostsLoading {} },
    }
}

#[component]
fn TagStatus(
    #[props(default = false)] failed: bool,
    #[props(default)] on_retry: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "tag-status tag-enter", "data-state": if failed { "error" } else { "empty" },
            div { class: "tag-status-art", aria_hidden: "true", TagIllustration { paused: failed } }
            div { class: "tag-status-copy",
                p { class: "tag-eyebrow", if failed { "A LITTLE PAUSE / 稍作停留" } else { "STILL GROWING / 等待生长" } }
                h2 { if failed { "文字暂时未能展开" } else { "这里的文字，还在生长" } }
                p { class: "tag-status-description", role: "status", aria_atomic: "true",
                    if failed { "文章暂时未能加载，稍等片刻，再试一次。" }
                    else { "这个主题下还没有发布的文章，先去别的枝叶间逛逛吧。" }
                }
                div { class: "tag-status-actions",
                    if failed {
                        button { class: "tag-button", r#type: "button", onclick: move |_| on_retry.call(()),
                            span { class: "tag-retry-icon", aria_hidden: "true", "↻" } "重新加载"
                        }
                    }
                    Link { class: "tag-text-link", to: Route::Archives {},
                        span { "返回归档" } span { class: "tag-arrow", aria_hidden: "true", "↗" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::post::PostStatus;
    use chrono::{TimeZone, Utc};

    fn post(id: i32, created_day: u32, published_day: Option<u32>, tags: &[&str]) -> PostListItem {
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
            tags: tags.iter().map(|tag| (*tag).to_string()).collect(),
            cover_image: None,
            reading_time: 1,
            word_count: 0,
        }
    }

    #[test]
    fn sorting_uses_publication_then_creation_and_stable_id_ties() {
        let posts = vec![
            post(4, 1, Some(10), &[]),
            post(2, 20, Some(5), &[]),
            post(3, 10, None, &[]),
            post(1, 2, None, &[]),
        ];
        let ids = |order| {
            sorted_posts(&posts, order)
                .iter()
                .map(|post| post.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(TagSortOrder::Latest), [4, 3, 2, 1]);
        assert_eq!(ids(TagSortOrder::Earliest), [1, 2, 3, 4]);
        assert_eq!(
            posts.iter().map(|post| post.id).collect::<Vec<_>>(),
            [4, 2, 3, 1]
        );
        assert!(sorted_posts(&[], TagSortOrder::Latest).is_empty());
    }

    #[test]
    fn related_tags_count_articles_exclude_self_and_break_ties_by_name() {
        let posts = vec![
            post(
                1,
                1,
                None,
                &["Architecture", "Rust", "Rust", "CI-CD", "", " "],
            ),
            post(2, 2, None, &["Architecture", "Docker", "CI-CD"]),
        ];
        assert_eq!(
            related_tags(&posts, "Architecture"),
            ["CI-CD", "Docker", "Rust"]
        );
        assert!(related_tags(&[post(1, 1, None, &["Architecture"])], "Architecture").is_empty());
        assert!(related_tags(&[], "Architecture").is_empty());
    }

    #[test]
    fn related_tags_keep_the_six_most_connected_topics() {
        let posts = vec![
            post(1, 1, None, &["H", "G", "F", "E", "D", "C", "B", "A"]),
            post(2, 2, None, &["H", "G"]),
        ];
        assert_eq!(related_tags(&posts, "A"), ["G", "H", "B", "C", "D", "E"]);
    }
}
