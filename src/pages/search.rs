//! 搜索页：全文检索、主题入口与轻量阅读索引。
//! 搜索由显式提交触发；请求序号确保清空或新搜索后不会回填旧结果。

use std::rc::Rc;

use dioxus::prelude::*;

use crate::api::posts::{list_tags, search_posts, PostListResponse};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::search_skeleton::SearchSkeleton;
use crate::models::post::PostListItem;
use crate::router::Route;

#[component]
pub fn Search() -> Element {
    let mut query = use_signal(String::new);
    let mut submitted_query = use_signal(String::new);
    let mut search_res = use_signal(|| None::<Result<PostListResponse, ServerFnError>>);
    let mut is_searching = use_signal(|| false);
    let mut request_id = use_signal(|| 0_u64);
    let mut input_element = use_signal(|| None::<Rc<MountedData>>);

    let mut on_search = move |value: String| {
        let q = value.trim().to_string();
        if q.is_empty() || (is_searching() && q == submitted_query()) {
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

    let mut clear_search = move || {
        request_id += 1;
        query.set(String::new());
        submitted_query.set(String::new());
        search_res.set(None);
        is_searching.set(false);
        if let Some(element) = input_element() {
            spawn(async move {
                let _ = element.set_focus(true).await;
            });
        }
    };

    let status = if is_searching() {
        format!("正在查找「{}」…", submitted_query())
    } else {
        match search_res().as_ref() {
            Some(Ok(res)) if res.posts.len() == 50 => {
                "找到 50 篇文章，展示最相关的结果".to_string()
            }
            Some(Ok(res)) => format!("找到 {} 篇相关文章", res.posts.len()),
            Some(Err(_)) => "搜索暂时未能完成，请重试".to_string(),
            None => "输入关键词，搜索标题与正文".to_string(),
        }
    };

    rsx! {
        div { class: "search-page",
            header { class: "search-hero search-enter",
                div { class: "search-hero-copy",
                    p { class: "search-eyebrow", span { aria_hidden: "true" } "SEARCH / 文字索引" }
                    h1 { "在字里行间，" span { "找到一点灵感。" } }
                    p { class: "search-description", "一个词，一条线索。重新遇见那些值得留下的文字。" }
                }
                SearchIllustration {}
            }

            section { class: "search-workspace search-enter", style: "--search-delay: 90ms",
                aria_label: "文章搜索",
                form { class: "search-form", role: "search",
                    onsubmit: move |event| {
                        event.prevent_default();
                        on_search(query());
                    },
                    label { class: "search-label", r#for: "article-search", "你想寻找什么？" }
                    div { class: "search-field", "data-loading": is_searching().to_string(),
                        svg { class: "search-field-icon", view_box: "0 0 24 24", fill: "none", "aria-hidden": "true",
                            circle { cx: "10.75", cy: "10.75", r: "6.75", stroke: "currentColor", stroke_width: "1.6" }
                            path { d: "m16 16 4.5 4.5", stroke: "currentColor", stroke_width: "1.6", stroke_linecap: "round" }
                        }
                        input {
                            id: "article-search",
                            class: "search-input ygg-search-clear",
                            r#type: "search",
                            name: "q",
                            placeholder: "搜索文章、想法、只言片语…",
                            autocomplete: "off",
                            maxlength: "200",
                            enterkeyhint: "search",
                            aria_describedby: "search-hint",
                            value: query(),
                            onmounted: move |event| input_element.set(Some(event.data())),
                            oninput: move |event| {
                                let value = event.value();
                                if value.is_empty() {
                                    clear_search();
                                } else {
                                    query.set(value);
                                }
                            },
                            onkeydown: move |event| {
                                if event.key() == Key::Escape {
                                    event.prevent_default();
                                    clear_search();
                                }
                            },
                        }
                        button {
                            class: "search-clear",
                            r#type: "button",
                            aria_label: "清空搜索",
                            "data-visible": (!query().is_empty()).to_string(),
                            disabled: query().is_empty(),
                            tabindex: if query().is_empty() { "-1" } else { "0" },
                            onclick: move |_| clear_search(),
                            svg { view_box: "0 0 20 20", fill: "none", "aria-hidden": "true",
                                path { d: "m6 6 8 8M14 6l-8 8", stroke: "currentColor", stroke_width: "1.5", stroke_linecap: "round" }
                            }
                        }
                        button {
                            class: "search-submit",
                            r#type: "submit",
                            disabled: query().trim().is_empty() || (is_searching() && query().trim() == submitted_query()),
                            aria_label: if is_searching() { "正在搜索" } else { "搜索文章" },
                            span { if is_searching() { "寻找中" } else { "搜索" } }
                            span { class: "search-submit-symbol", "data-loading": is_searching().to_string(), aria_hidden: "true",
                                if is_searching() { span { class: "search-spinner" } } else { "↗" }
                            }
                        }
                    }
                    div { class: "search-hints", id: "search-hint",
                        span { "搜索标题与正文，让好奇心带路。" }
                        span { class: "search-keyboard-hint", kbd { "Enter" } " 搜索" span { "·" } kbd { "Esc" } " 清空" }
                    }
                }
            }

            p { class: "sr-only", role: "status", aria_live: "polite", "{status}" }
            section { class: "search-content", aria_label: "搜索结果", aria_busy: is_searching().to_string(),
                if is_searching() {
                    div { class: "search-results-heading search-enter",
                        h2 { "正在翻阅文字" }
                        span { "正在寻找相关的文章…" }
                    }
                    DelayedSkeleton { SearchSkeleton {} }
                } else if let Some(Ok(res)) = search_res() {
                    div { key: "results-{request_id}", class: "search-enter",
                        div { class: "search-results-heading",
                            h2 { "关于「{submitted_query}」" }
                            span { "{status}" }
                        }
                        if res.posts.is_empty() {
                            div { class: "search-empty",
                                span { class: "search-empty-symbol", aria_hidden: "true", "∅" }
                                h3 { "这条线索，还没有回音。" }
                                p { "试试更简短的词，或换一种表达。" }
                                button { class: "search-text-link", r#type: "button", onclick: move |_| clear_search(),
                                    "换个关键词" span { aria_hidden: "true", "↗" }
                                }
                            }
                        } else {
                            p { class: "search-result-order", "按相关度排列 · 最多展示 50 篇" }
                            div { class: "search-result-list",
                                for (index, post) in res.posts.into_iter().enumerate() {
                                    SearchResult { key: "{post.id}", post, index }
                                }
                            }
                        }
                    }
                } else if search_res().as_ref().is_some_and(|res| res.is_err()) {
                    div { class: "search-empty search-enter",
                        span { class: "search-empty-symbol", aria_hidden: "true", "↻" }
                        h2 { "线索还在，稍后再试。" }
                        p { "暂时没能完成搜索，请重新试一次。" }
                        button { class: "search-text-link", r#type: "button", onclick: move |_| on_search(submitted_query()),
                            "重新搜索" span { aria_hidden: "true", "↗" }
                        }
                    }
                } else {
                    div { class: "search-intro search-enter", style: "--search-delay: 170ms",
                        span { class: "search-section-number", aria_hidden: "true", "01 / DISCOVER" }
                        h2 { "不必有答案，从好奇开始。" }
                        p { "想找的也许是一篇文章，也许是曾经一闪而过的念头。" }
                    }
                }
            }

            SearchTopics {}

            footer { class: "search-footer search-enter", style: "--search-delay: 260ms",
                span { "每一片文字，都有它的来处。" }
                Link { class: "search-text-link", to: Route::Archives {},
                    "去归档随意翻翻" span { aria_hidden: "true", "↗" }
                }
            }
        }
    }
}

/// 独立加载主题，失败时仍保留可用的搜索与归档入口。
#[component]
fn SearchTopics() -> Element {
    let tags = use_resource(move || async move { list_tags().await });
    let data = tags.read();
    let Some(Ok(data)) = data.as_ref() else {
        return rsx! {};
    };
    let mut topics: Vec<_> = data.tags.iter().filter(|tag| tag.post_count > 0).collect();
    topics.sort_by_key(|tag| std::cmp::Reverse(tag.post_count));
    if topics.is_empty() {
        return rsx! {};
    }
    rsx! {
        nav { class: "search-topics search-enter", style: "--search-delay: 210ms", aria_label: "按主题探索文章",
            div { class: "search-topics-heading", span { "也可以，沿着主题探索" } span { aria_hidden: "true", "EXPLORE BY TOPIC" } }
            div { class: "search-topic-list",
                for tag in topics.into_iter().take(8) {
                    Link { key: "{tag.id}", class: "search-topic", to: Route::TagDetail { tag: tag.name.clone() },
                        span { class: "search-topic-hash", aria_hidden: "true", "#" }
                        span { "{tag.name}" }
                        span { class: "search-topic-count", aria_label: "{tag.post_count} 篇文章", "{tag.post_count}" }
                    }
                }
            }
        }
    }
}

#[component]
fn SearchResult(post: PostListItem, index: usize) -> Element {
    let number = format!("{:02}", index + 1);
    let delay = index.min(7) * 45;
    let date = post.formatted_date();
    let reading_time = post.reading_time.max(1);
    rsx! {
        article { class: "search-result search-enter", style: "--search-delay: {delay}ms",
            Link { class: "search-result-link", to: Route::PostDetail { slug: post.slug },
                span { class: "search-result-number", aria_hidden: "true", "{number}" }
                div { class: "search-result-body",
                    div { class: "search-result-meta", time { datetime: "{date}", "{date}" } span { "{reading_time} 分钟阅读" } }
                    h2 { "{post.title}" }
                    if let Some(summary) = post.summary.filter(|text| !text.is_empty()) {
                        p { class: "search-result-summary", "{summary}" }
                    }
                    if !post.tags.is_empty() {
                        div { class: "search-result-tags",
                            for tag in post.tags.iter().take(3) { span { key: "{tag}", "# {tag}" } }
                        }
                    }
                }
                span { class: "search-result-arrow", aria_hidden: "true", "↗" }
            }
        }
    }
}

/// 原生矢量插画，颜色随站点主题变化，镜片随搜索框聚焦轻移。
#[component]
fn SearchIllustration() -> Element {
    rsx! {
        div { class: "search-illustration", aria_hidden: "true",
            svg { view_box: "0 0 260 230", fill: "none",
                circle { class: "search-art-orbit", cx: "130", cy: "112", r: "88", stroke: "currentColor", stroke_width: "0.7", stroke_dasharray: "2 7" }
                circle { class: "search-art-orbit", cx: "130", cy: "112", r: "107", stroke: "currentColor", stroke_width: "0.6" }
                g { class: "search-art-pages",
                    rect { class: "search-art-back", x: "57", y: "40", width: "124", height: "155", rx: "8", transform: "rotate(-10 119 117)" }
                    rect { class: "search-art-paper", x: "68", y: "37", width: "124", height: "155", rx: "8", stroke: "currentColor", stroke_width: "1" }
                    path { class: "search-art-lines", d: "M89 68h43M89 82h76M89 96h62M89 144h69M89 157h54M89 170h29", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round" }
                    path { d: "M156 37v28l9-6 9 6V37", fill: "currentColor", opacity: "0.2" }
                }
                g { class: "search-art-lens",
                    circle { class: "search-art-glass", cx: "163", cy: "121", r: "36", stroke: "currentColor", stroke_width: "2" }
                    circle { cx: "163", cy: "121", r: "29", stroke: "currentColor", stroke_width: "0.7", opacity: "0.3" }
                    path { d: "m189 147 26 28", stroke: "currentColor", stroke_width: "9", stroke_linecap: "round" }
                    path { d: "m189 147 26 28", stroke: "var(--color-paper-theme)", stroke_width: "5", stroke_linecap: "round" }
                    path { d: "M151 134c0-17 10-28 25-29-1 17-9 26-25 29Zm0 0 15-17", stroke: "currentColor", stroke_width: "1.5", stroke_linecap: "round", stroke_linejoin: "round" }
                }
                path { class: "search-art-spark", d: "M43 102v12m-6-6h12M208 65v10m-5-5h10", stroke: "currentColor", stroke_width: "1.3", stroke_linecap: "round" }
                circle { cx: "63", cy: "177", r: "3", fill: "currentColor", opacity: "0.5" }
            }
            span { "A LITTLE CURIOSITY GOES A LONG WAY" }
        }
    }
}
