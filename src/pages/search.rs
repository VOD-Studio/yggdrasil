//! 全文搜索结果页 `/search?q=`：顶部 Composer，结果按相关度或时间排列，命中词高亮。
//!
//! 输入停顿后自动检索，回车立即提交；编辑与清空会立即作废旧请求。输入框里按 ↓
//! 进入结果，结果间 ↑↓ 移动，↑ 越过第一条回到输入框。未输入时给出常用标签。

use std::rc::Rc;

use dioxus::prelude::*;

use crate::api::posts::{list_tags, search_posts, PostListResponse};
use crate::components::palette::{contains_ci, highlight, snippet};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::search_skeleton::SearchSkeleton;
use crate::components::ui::{FilterTabs, Icon, IconName, LoadError};
use crate::models::post::PostListItem;
use crate::pages::archives::ranked_tags;
use crate::router::Route;
use crate::utils::time::sleep_ms;

const SEARCH_DEBOUNCE_MS: u32 = 350;
/// 服务端最多返回的条数。
const RESULT_LIMIT: usize = 50;
/// 未输入时推荐的标签数。
const SUGGESTED_TAGS: usize = 8;
/// 摘要片段在命中词两侧保留的字数。
const SNIPPET_RADIUS: usize = 40;
/// 每条结果最多列出的标签数。
const RESULT_TAGS: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum SortOrder {
    #[default]
    Relevance,
    Latest,
}

impl SortOrder {
    fn value(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Latest => "latest",
        }
    }

    fn from_value(value: &str) -> Self {
        if value == "latest" {
            Self::Latest
        } else {
            Self::Relevance
        }
    }
}

/// Only settled responses are persisted; request IDs and pending work belong to
/// the mounted page and must never be revived by browser Back.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct SearchHistoryState {
    query: String,
    submitted_query: String,
    response: Option<PostListResponse>,
    failed: bool,
    #[serde(default)]
    order: SortOrder,
}

impl SearchHistoryState {
    fn result(&self) -> Option<Result<PostListResponse, ServerFnError>> {
        if self.failed {
            Some(Err(ServerFnError::new("搜索暂时未能完成，请重试")))
        } else {
            self.response.clone().map(Ok)
        }
    }
}

/// 服务端已按相关度排序；「最新」在浏览器里按发布时间重排，同时间按 id 稳定。
fn ordered(posts: &[PostListItem], order: SortOrder) -> Vec<PostListItem> {
    let mut posts = posts.to_vec();
    if order == SortOrder::Latest {
        posts.sort_by(|a, b| {
            let at = |p: &PostListItem| p.published_at.unwrap_or(p.created_at);
            at(b).cmp(&at(a)).then_with(|| b.id.cmp(&a.id))
        });
    }
    posts
}

fn result_id(index: usize) -> String {
    format!("search-result-{index}")
}

/// 搜索页。`q` 来自 `/search?q=`：历史记录里没有保存的查询时用它作为初始查询并立即搜索。
#[component]
pub fn Search(#[props(default)] q: String) -> Element {
    let entry_id = use_hook(crate::bridges::navigation::entry_id);
    let saved = use_hook(|| {
        crate::bridges::navigation::read_state::<SearchHistoryState>("search").unwrap_or_default()
    });
    let initial_query = if saved.query.is_empty() {
        q.trim().to_string()
    } else {
        saved.query.clone()
    };
    let mut query = use_signal(|| initial_query.clone());
    let mut submitted_query = use_signal(|| saved.submitted_query.clone());
    let mut search_res = use_signal(|| saved.result());
    let mut order = use_signal(|| saved.order);
    let mut is_searching = use_signal(|| false);
    let mut request_id = use_signal(|| 0_u64);
    let mut is_composing = use_signal(|| false);
    let mut input_element = use_signal(|| None::<Rc<MountedData>>);

    use_effect(move || {
        let result = search_res();
        let state = SearchHistoryState {
            query: query(),
            submitted_query: if is_searching() {
                String::new()
            } else {
                submitted_query()
            },
            response: result
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .cloned(),
            failed: result.as_ref().is_some_and(|result| result.is_err()),
            order: order(),
        };
        crate::bridges::navigation::write_state(&entry_id, "search", &state);
    });

    let mut on_search = move |value: String| {
        let q = value.trim().to_string();
        if is_composing() || q.is_empty() || (is_searching() && q == submitted_query()) {
            return;
        }
        let id = request_id() + 1;
        request_id.set(id);
        submitted_query.set(q.clone());
        is_searching.set(true);
        search_res.set(None);
        spawn(async move {
            let res = search_posts(q).await;
            if request_id() == id {
                search_res.set(Some(res));
                is_searching.set(false);
            }
        });
    };

    let focus_input = move || {
        if let Some(element) = input_element() {
            spawn(async move {
                let _ = element.set_focus(true).await;
            });
        }
    };

    let mut clear_search = move || {
        request_id += 1;
        query.set(String::new());
        submitted_query.set(String::new());
        search_res.set(None);
        is_searching.set(false);
        focus_input();
    };

    let mut schedule_search = move || {
        // 同一序号同时约束防抖计时与网络响应，A → B → A 也不会提前提交。
        request_id += 1;
        is_searching.set(false);
        let id = request_id();
        let q = query().trim().to_string();
        if q.is_empty() {
            submitted_query.set(String::new());
            search_res.set(None);
            return;
        }
        if is_composing() {
            return;
        }
        spawn(async move {
            sleep_ms(SEARCH_DEBOUNCE_MS).await;
            if request_id() == id
                && !is_composing()
                && !(q == submitted_query() && search_res().as_ref().is_some_and(|res| res.is_ok()))
            {
                on_search(q);
            }
        });
    };

    // 带 `?q=` 进入或在本页再次带入新查询时立即搜索；与已提交的查询相同则不重复请求。
    use_effect(use_reactive((&q,), move |(q,)| {
        let q = q.trim().to_string();
        if q.is_empty() || q == *submitted_query.peek() {
            return;
        }
        query.set(q.clone());
        on_search(q);
    }));

    let status = if is_searching() {
        format!("正在查找「{}」…", submitted_query())
    } else {
        match search_res().as_ref() {
            Some(Ok(res)) if res.posts.len() >= RESULT_LIMIT => {
                format!(
                    "「{}」找到 {RESULT_LIMIT} 篇以上，列出最相关的 {RESULT_LIMIT} 篇",
                    submitted_query()
                )
            }
            Some(Ok(res)) => format!("「{}」找到 {} 篇", submitted_query(), res.posts.len()),
            Some(Err(_)) => "搜索暂时未能完成，请重试".to_string(),
            None => String::new(),
        }
    };
    let idle = !is_searching() && search_res().is_none();

    rsx! {
        document::Title { "搜索 · Yggdrasil" }
        div { class: "search-page",
            h1 { class: "search-title page-enter", "搜索" }
            form {
                class: "composer search-composer page-enter",
                style: "--enter-index: 1",
                role: "search",
                action: "/search",
                method: "get",
                onsubmit: move |event| {
                    event.prevent_default();
                    on_search(query());
                },
                Icon { name: IconName::Search, size: 18 }
                label { class: "sr-only", r#for: "article-search", "搜索文章" }
                input {
                    id: "article-search",
                    class: "composer-input",
                    r#type: "search",
                    name: "q",
                    placeholder: "标题、摘要或标签…",
                    autocomplete: "off",
                    maxlength: "200",
                    "enterkeyhint": "search",
                    value: query(),
                    onmounted: move |event| input_element.set(Some(event.data())),
                    oninput: move |event| {
                        let value = event.value();
                        if value.is_empty() {
                            clear_search();
                        } else {
                            query.set(value);
                            schedule_search();
                        }
                    },
                    oncompositionstart: move |_| {
                        is_composing.set(true);
                        schedule_search();
                    },
                    oncompositionend: move |_| {
                        is_composing.set(false);
                        schedule_search();
                    },
                    onkeydown: move |event| {
                        if event.is_composing() || is_composing() {
                            return;
                        }
                        match event.key() {
                            Key::Escape => {
                                event.prevent_default();
                                clear_search();
                            }
                            Key::ArrowDown if search_res().as_ref().is_some_and(|r| r.as_ref().is_ok_and(|r| !r.posts.is_empty())) => {
                                event.prevent_default();
                                focus_result(0);
                            }
                            _ => {}
                        }
                    },
                }
                if !query().is_empty() {
                    button {
                        class: "btn btn-xs btn-icon btn-ghost search-clear",
                        r#type: "button",
                        aria_label: "清空搜索",
                        onclick: move |_| clear_search(),
                        Icon { name: IconName::X, size: 14 }
                    }
                }
                button {
                    class: "composer-send",
                    r#type: "submit",
                    aria_label: "搜索",
                    disabled: query().trim().is_empty(),
                    Icon { name: IconName::ArrowRight, size: 17 }
                }
            }

            p { class: "sr-only", role: "status", aria_live: "polite", "{status}" }
            section {
                class: "search-content",
                "data-vt-list": (!is_searching()).then_some("true"),
                aria_label: "搜索结果",
                aria_busy: is_searching().to_string(),
                if idle {
                    SuggestedTags {
                        on_pick: move |name: String| {
                            query.set(name.clone());
                            on_search(name);
                        },
                    }
                } else if is_searching() {
                    DelayedSkeleton { SearchSkeleton {} }
                } else if let Some(Ok(res)) = search_res() {
                    if res.posts.is_empty() {
                        div { class: "search-empty page-enter",
                            span { class: "search-empty-icon", aria_hidden: "true",
                                Icon { name: IconName::SearchX, size: 20 }
                            }
                            p { class: "search-empty-title", "没有找到「{submitted_query}」" }
                            p { class: "search-empty-description", "换个更短的关键词，或者从归档里按标签找找。" }
                            Link { class: "btn btn-secondary btn-sm", to: Route::Archives {}, "浏览归档" }
                        }
                    } else {
                        div { class: "search-toolbar",
                            p { class: "search-count tabular-nums", aria_hidden: "true",
                                "「{submitted_query}」找到 "
                                strong { "{res.posts.len()}" }
                                if res.posts.len() >= RESULT_LIMIT { " 篇以上" } else { " 篇" }
                            }
                            FilterTabs {
                                items: vec![("relevance", "相关度"), ("latest", "最新")],
                                active_value: order().value().to_string(),
                                on_change: move |value: String| order.set(SortOrder::from_value(&value)),
                                aria_label: "排序",
                                class: "search-sort",
                            }
                        }
                        ol {
                            class: "search-results",
                            "data-order": order().value(),
                            onkeydown: move |event| {
                                let Some(index) = focused_result() else { return };
                                match event.key() {
                                    Key::ArrowDown => {
                                        event.prevent_default();
                                        focus_result(index + 1);
                                    }
                                    Key::ArrowUp => {
                                        event.prevent_default();
                                        if index == 0 {
                                            focus_input();
                                        } else {
                                            focus_result(index - 1);
                                        }
                                    }
                                    _ => {}
                                }
                            },
                            for (index, post) in ordered(&res.posts, order()).into_iter().enumerate() {
                                li { key: "{post.id}",
                                    SearchResult { post, index, query: submitted_query() }
                                }
                            }
                        }
                    }
                } else {
                    LoadError {
                        title: "搜索暂时未能完成",
                        class: "page-enter",
                        on_retry: move |_| on_search(query()),
                    }
                }
            }
        }
    }
}

/// 当前获得焦点的结果序号（仅 wasm；服务端渲染时没有焦点）。
fn focused_result() -> Option<usize> {
    #[cfg(target_arch = "wasm32")]
    {
        let active = web_sys::window()?.document()?.active_element()?;
        active.id().strip_prefix("search-result-")?.parse().ok()
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

fn focus_result(index: usize) {
    #[cfg(target_arch = "wasm32")]
    crate::components::palette::focus_by_id(&result_id(index));
    #[cfg(not(target_arch = "wasm32"))]
    let _ = index;
}

/// 未输入时的「试试这些标签」。标签接口失败时不显示。
#[component]
fn SuggestedTags(on_pick: EventHandler<String>) -> Element {
    let tags = use_resource(move || async move { list_tags().await });
    let data = tags.read();
    let Some(Ok(data)) = data.as_ref() else {
        return rsx! {};
    };
    let tags = ranked_tags(data.tags.clone());
    if tags.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "search-suggest page-enter",
            h2 { class: "search-suggest-title", "试试这些标签" }
            div { class: "search-suggest-list",
                for tag in tags.into_iter().take(SUGGESTED_TAGS) {
                    button {
                        key: "{tag.id}",
                        r#type: "button",
                        class: "chip",
                        onclick: {
                            let name = tag.name.clone();
                            move |_| on_pick.call(name.clone())
                        },
                        "{tag.name}"
                        span { class: "chip-count", "{tag.post_count}" }
                    }
                }
            }
        }
    }
}

/// 命中词用 `<mark>` 标出的一段文字。
#[component]
fn Marked(text: String, query: String) -> Element {
    rsx! {
        for (i, (part, hit)) in highlight(&text, &query).into_iter().enumerate() {
            if hit {
                mark { key: "{i}", "{part}" }
            } else {
                span { key: "{i}", "{part}" }
            }
        }
    }
}

#[component]
fn SearchResult(post: PostListItem, index: usize, query: String) -> Element {
    let date = post
        .published_at
        .unwrap_or(post.created_at)
        .format("%Y.%m.%d")
        .to_string();
    let reading_time = post.reading_time.max(1);
    let summary = post
        .summary
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .map(|text| snippet(text, &query, SNIPPET_RADIUS));
    rsx! {
        Link {
            id: result_id(index),
            class: "search-result",
            "data-vt-post-link": "{post.id}",
            to: Route::PostDetail { slug: post.slug.clone() },
            span { class: "search-result-title", "data-vt-post-id": "{post.id}", "data-vt-role": "title",
                Marked { text: post.title.clone(), query: query.clone() }
            }
            if let Some(summary) = summary {
                span { class: "search-result-summary",
                    Marked { text: summary, query: query.clone() }
                }
            }
            span { class: "search-result-meta",
                time { class: "tabular-nums", "{date}" }
                span { class: "tabular-nums", "{reading_time} 分钟" }
                if !post.tags.is_empty() {
                    span { class: "search-result-tags",
                        for tag in post.tags.iter().take(RESULT_TAGS) {
                            span {
                                key: "{tag}",
                                class: if contains_ci(tag, &query) { "search-result-tag is-hit" } else { "search-result-tag" },
                                "{tag}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn post(id: i32, day: Option<u32>, created_day: u32) -> PostListItem {
        let at = |d| Utc.with_ymd_and_hms(2026, 7, d, 0, 0, 0).unwrap();
        PostListItem {
            id,
            author_id: 1,
            title: format!("文章 {id}"),
            slug: format!("post-{id}"),
            summary: None,
            status: crate::models::post::PostStatus::Published,
            published_at: day.map(at),
            created_at: at(created_day),
            updated_at: at(created_day),
            deleted_at: None,
            tags: Vec::new(),
            cover_image: None,
            reading_time: 1,
            word_count: 0,
        }
    }

    #[test]
    fn latest_order_sorts_by_publication_and_keeps_relevance_untouched() {
        let posts = vec![post(1, Some(2), 1), post(2, None, 9), post(3, Some(5), 1)];
        let ids = |order| {
            ordered(&posts, order)
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(SortOrder::Relevance), [1, 2, 3]);
        assert_eq!(ids(SortOrder::Latest), [2, 3, 1]);
    }

    #[test]
    fn sort_order_round_trips_through_its_value() {
        for order in [SortOrder::Relevance, SortOrder::Latest] {
            assert_eq!(SortOrder::from_value(order.value()), order);
        }
        assert_eq!(SortOrder::from_value("unknown"), SortOrder::Relevance);
    }
}
