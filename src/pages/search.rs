//! 全文搜索结果页 `/search?q=`：顶部 Composer，结果按相关度或时间排列，命中词高亮。
//!
//! 输入停顿后自动检索，回车立即提交；编辑与清空会立即作废旧请求。输入框里按 ↓
//! 进入结果，结果间 ↑↓ 移动，↑ 越过第一条回到输入框。未输入时给出常用标签。
//!
//! 搜索请求只在浏览器发起（Effect / 事件处理里 `spawn_local`），页面没有 SSR 数据，
//! 因此路由使用默认 `SsrMode`。服务端与水合首帧都按 `?q=` 渲染输入框、不渲染结果；
//! 历史记录里保存的状态在挂载后的 Effect 里恢复，保证两端首帧 DOM 一致。

use std::sync::Arc;

use leptos::ev::KeyboardEvent;
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::error::ApiError;
use crate::api::posts::{list_tags, search_posts, PostListResponse};
use crate::components::palette::{contains_ci, highlight, snippet};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::search_skeleton::SearchSkeleton;
use crate::components::ui::{
    Composer, ComposerInput, ComposerSend, Icon, IconName, LoadError, SegmentedControl,
    SegmentedItem,
};
use crate::models::post::{PostListItem, Tag};
use crate::pages::archives::ranked_tags;
use crate::routes::Route;
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

/// 搜索结果：成功的响应，或失败。
type SearchResult = Result<PostListResponse, ApiError>;

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
    fn result(&self) -> Option<SearchResult> {
        if self.failed {
            Some(Err(ApiError::Internal("搜索暂时未能完成，请重试".into())))
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

/// 搜索路由，对应 `/search`（查询参数 `q`）。没有 SSR 数据，请求全部在浏览器发起。
pub struct SearchRoute;

#[lazy_route]
impl LazyRoute for SearchRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <SearchPage/> }.into_any()
    }
}

/// 搜索页。`q` 来自 `/search?q=`：历史记录里没有保存的查询时用它作为初始查询并立即搜索。
#[component]
fn SearchPage() -> impl IntoView {
    let params = use_query_map();
    let q = Memo::new(move |_| {
        params
            .read()
            .get("q")
            .unwrap_or_default()
            .trim()
            .to_string()
    });
    let entry_id = crate::bridges::navigation::entry_id();

    // 首帧与服务端一致：输入框只带 `?q=`，其余状态在挂载后恢复。
    let query = RwSignal::new(q.get_untracked());
    let submitted_query = RwSignal::new(String::new());
    let search_res = RwSignal::new(None::<SearchResult>);
    let order = RwSignal::new(SortOrder::default());
    let is_searching = RwSignal::new(false);
    let request_id = RwSignal::new(0_u64);
    let is_composing = RwSignal::new(false);
    let input_ref = NodeRef::<html::Input>::new();
    // 历史状态恢复完成前不写回，避免用首帧默认值覆盖掉要恢复的记录。
    let restored = RwSignal::new(false);

    Effect::new(move |_| {
        if !restored.get() {
            return;
        }
        let result = search_res.get();
        let state = SearchHistoryState {
            query: query.get(),
            submitted_query: if is_searching.get() {
                String::new()
            } else {
                submitted_query.get()
            },
            response: result
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .cloned(),
            failed: result.as_ref().is_some_and(|result| result.is_err()),
            order: order.get(),
        };
        crate::bridges::navigation::write_state(&entry_id, "search", &state);
    });

    let on_search = move |value: String| {
        let q = value.trim().to_string();
        if is_composing.get_untracked()
            || q.is_empty()
            || (is_searching.get_untracked() && q == submitted_query.get_untracked())
        {
            return;
        }
        let id = request_id.get_untracked() + 1;
        request_id.set(id);
        submitted_query.set(q.clone());
        is_searching.set(true);
        search_res.set(None);
        spawn_local(async move {
            let res = search_posts(q).await;
            // 页面可能已离开；序号不符说明有更新的请求或清空，旧响应作废。
            if request_id.try_get_untracked() == Some(id) {
                search_res.set(Some(res));
                is_searching.set(false);
            }
        });
    };

    // NodeRef 先于元素挂入文档被填充，等一帧再聚焦。
    let focus_input = move || {
        request_animation_frame(move || {
            if let Some(input) = input_ref.get_untracked() {
                let _ = input.focus();
            }
        });
    };

    let clear_search = move || {
        request_id.update(|id| *id += 1);
        query.set(String::new());
        submitted_query.set(String::new());
        search_res.set(None);
        is_searching.set(false);
        focus_input();
    };

    let schedule_search = move || {
        // 同一序号同时约束防抖计时与网络响应，A → B → A 也不会提前提交。
        request_id.update(|id| *id += 1);
        is_searching.set(false);
        let id = request_id.get_untracked();
        let q = query.get_untracked().trim().to_string();
        if q.is_empty() {
            submitted_query.set(String::new());
            search_res.set(None);
            return;
        }
        if is_composing.get_untracked() {
            return;
        }
        spawn_local(async move {
            sleep_ms(SEARCH_DEBOUNCE_MS).await;
            if request_id.try_get_untracked() == Some(id)
                && !is_composing.get_untracked()
                && !(q == submitted_query.get_untracked()
                    && search_res.with_untracked(|res| matches!(res, Some(Ok(_)))))
            {
                on_search(q);
            }
        });
    };

    // 挂载后先恢复历史状态，再处理 `?q=`：带 `?q=` 进入或在本页再次带入新查询时立即搜索；
    // 与已提交的查询相同则不重复请求。恢复与首次判断必须在同一个 Effect 里按序完成。
    Effect::new(move |previous: Option<()>| {
        let q = q.get();
        if previous.is_none() {
            if let Some(saved) =
                crate::bridges::navigation::read_state::<SearchHistoryState>("search")
            {
                if !saved.query.is_empty() {
                    query.set(saved.query.clone());
                }
                submitted_query.set(saved.submitted_query.clone());
                search_res.set(saved.result());
                order.set(saved.order);
            }
            restored.set(true);
        }
        if q.is_empty() || q == submitted_query.get_untracked() {
            return;
        }
        query.set(q.clone());
        on_search(q);
    });

    let status = move || {
        let submitted = submitted_query.get();
        if is_searching.get() {
            format!("正在查找「{submitted}」…")
        } else {
            search_res.with(|res| match res {
                Some(Ok(res)) if res.posts.len() >= RESULT_LIMIT => {
                    format!(
                        "「{submitted}」找到 {RESULT_LIMIT} 篇以上，列出最相关的 {RESULT_LIMIT} 篇"
                    )
                }
                Some(Ok(res)) => format!("「{submitted}」找到 {} 篇", res.posts.len()),
                Some(Err(_)) => "搜索暂时未能完成，请重试".to_string(),
                None => String::new(),
            })
        }
    };

    let content = move || {
        let searching = is_searching.get();
        let res = search_res.get();
        match res {
            None if !searching => view! {
                <SuggestedTags on_pick=move |name: String| {
                    query.set(name.clone());
                    on_search(name);
                }/>
            }
            .into_any(),
            _ if searching => view! {
                <DelayedSkeleton>
                    <SearchSkeleton/>
                </DelayedSkeleton>
            }
            .into_any(),
            Some(Ok(res)) => results_view(res, submitted_query.get_untracked(), order, focus_input),
            _ => view! {
                <LoadError
                    title="搜索暂时未能完成"
                    class="page-enter"
                    on_retry=move |_| on_search(query.get_untracked())
                />
            }
            .into_any(),
        }
    };

    let on_input = move |value: String| {
        if value.is_empty() {
            clear_search();
        } else {
            query.set(value);
            schedule_search();
        }
    };

    let on_keydown = move |event: KeyboardEvent| match event.key().as_str() {
        "Escape" => {
            event.prevent_default();
            clear_search();
        }
        "ArrowDown"
            if search_res.with_untracked(|res| {
                res.as_ref()
                    .is_some_and(|res| res.as_ref().is_ok_and(|res| !res.posts.is_empty()))
            }) =>
        {
            event.prevent_default();
            focus_result(0);
        }
        _ => {}
    };

    view! {
        <Title text="搜索 · Yggdrasil"/>
        <div class="search-page">
            <h1 class="search-title page-enter">"搜索"</h1>
            <form
                class="search-composer page-enter"
                style="--enter-index: 1"
                role="search"
                action="/search"
                method="get"
                on:submit=move |event| {
                    event.prevent_default();
                    on_search(query.get_untracked());
                }
            >
                <Composer>
                    <Icon name=IconName::Search size=18/>
                    <label class="sr-only" for="article-search">
                        "搜索文章"
                    </label>
                    <ComposerInput
                        id="article-search"
                        search=true
                        maxlength=200
                        placeholder="标题、摘要或标签…"
                        value=query
                        input_ref=input_ref
                        oninput=on_input
                        oncomposition=move |active: bool| {
                            is_composing.set(active);
                            schedule_search();
                        }
                        onkeydown=on_keydown
                    />
                    <Show when=move || !query.get().is_empty()>
                        <button
                            class="btn btn-xs btn-icon btn-ghost search-clear"
                            type="button"
                            aria-label="清空搜索"
                            on:click=move |_| clear_search()
                        >
                            <Icon name=IconName::X size=14/>
                        </button>
                    </Show>
                    <ComposerSend
                        label="搜索"
                        disabled=Signal::derive(move || query.get().trim().is_empty())
                    />
                </Composer>
            </form>

            <p class="sr-only" role="status" aria-live="polite">
                {status}
            </p>
            <section
                class="search-content"
                data-vt-list=move || (!is_searching.get()).then_some("true")
                aria-label="搜索结果"
                aria-busy=move || is_searching.get().to_string()
            >
                {content}
            </section>
        </div>
    }
}

/// 搜索完成后的内容：无结果提示，或工具栏（计数 + 排序）与结果列表。
fn results_view(
    res: PostListResponse,
    submitted: String,
    order: RwSignal<SortOrder>,
    focus_input: impl Fn() + Copy + Send + Sync + 'static,
) -> AnyView {
    if res.posts.is_empty() {
        return view! {
            <div class="search-empty page-enter">
                <span class="search-empty-icon" aria-hidden="true">
                    <Icon name=IconName::SearchX size=20/>
                </span>
                <p class="search-empty-title">{format!("没有找到「{submitted}」")}</p>
                <p class="search-empty-description">"换个更短的关键词，或者从归档里按标签找找。"</p>
                <a class="btn btn-secondary btn-sm" href=Route::Archives.to_string()>
                    "浏览归档"
                </a>
            </div>
        }
        .into_any();
    }

    let total = res.posts.len();
    let posts = Arc::new(res.posts);
    let list_query = submitted.clone();
    view! {
        <div class="search-toolbar">
            <p class="search-count tabular-nums" aria-hidden="true">
                {format!("「{submitted}」找到 ")} <strong>{total}</strong>
                {if total >= RESULT_LIMIT { " 篇以上" } else { " 篇" }}
            </p>
            <SegmentedControl
                items=vec![
                    SegmentedItem::new("relevance".to_string(), "相关度"),
                    SegmentedItem::new("latest".to_string(), "最新"),
                ]
                value=Signal::derive(move || order.get().value().to_string())
                on_change=move |value: String| order.set(SortOrder::from_value(&value))
                aria_label="排序"
                class="search-sort"
            />
        </div>
        <ol
            class="search-results"
            data-order=move || order.get().value()
            on:keydown=move |event: KeyboardEvent| {
                let Some(index) = focused_result() else { return };
                match event.key().as_str() {
                    "ArrowDown" => {
                        event.prevent_default();
                        focus_result(index + 1);
                    }
                    "ArrowUp" => {
                        event.prevent_default();
                        if index == 0 {
                            focus_input();
                        } else {
                            focus_result(index - 1);
                        }
                    }
                    _ => {}
                }
            }
        >
            {move || {
                ordered(&posts, order.get())
                    .into_iter()
                    .enumerate()
                    .map(|(index, post)| {
                        view! {
                            <li>
                                <SearchResultItem post index query=list_query.clone()/>
                            </li>
                        }
                    })
                    .collect_view()
            }}
        </ol>
    }
    .into_any()
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
///
/// 只在浏览器挂载后请求一次；用 Effect + 镜像信号而不是 `Resource`，避免外层
/// `Suspense` 把整页计入挂起（SSR 与水合首帧都不渲染）。
#[component]
fn SuggestedTags(#[prop(into)] on_pick: Callback<String>) -> impl IntoView {
    let tags = RwSignal::new(Vec::<Tag>::new());
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(data) = list_tags().await {
                let _ = tags.try_set(ranked_tags(data.tags));
            }
        });
    });

    move || {
        let tags = tags.get();
        if tags.is_empty() {
            return None;
        }
        Some(view! {
            <div class="search-suggest page-enter">
                <h2 class="search-suggest-title">"试试这些标签"</h2>
                <div class="search-suggest-list">
                    {tags
                        .into_iter()
                        .take(SUGGESTED_TAGS)
                        .map(|tag| {
                            let name = tag.name.clone();
                            view! {
                                <button
                                    type="button"
                                    class="chip"
                                    on:click=move |_| on_pick.run(name.clone())
                                >
                                    {tag.name} <span class="chip-count">{tag.post_count}</span>
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
        })
    }
}

/// 命中词用 `<mark>` 标出的一段文字。
#[component]
fn Marked(text: String, query: String) -> impl IntoView {
    highlight(&text, &query)
        .into_iter()
        .map(|(part, hit)| {
            if hit {
                view! { <mark>{part}</mark> }.into_any()
            } else {
                view! { <span>{part}</span> }.into_any()
            }
        })
        .collect_view()
}

#[component]
fn SearchResultItem(post: PostListItem, index: usize, query: String) -> impl IntoView {
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
    let link_id = post.id.to_string();
    let title_id = post.id.to_string();
    let tags = post
        .tags
        .iter()
        .take(RESULT_TAGS)
        .map(|tag| {
            let class = if contains_ci(tag, &query) {
                "search-result-tag is-hit"
            } else {
                "search-result-tag"
            };
            view! { <span class=class>{tag.clone()}</span> }
        })
        .collect_view();
    let has_tags = !post.tags.is_empty();
    view! {
        <a
            id=result_id(index)
            class="search-result"
            data-vt-post-link=link_id
            href=Route::PostDetail { slug: post.slug.clone() }.to_string()
        >
            <span class="search-result-title" data-vt-post-id=title_id data-vt-role="title">
                <Marked text=post.title.clone() query=query.clone()/>
            </span>
            {summary
                .map(|summary| {
                    view! {
                        <span class="search-result-summary">
                            <Marked text=summary query=query.clone()/>
                        </span>
                    }
                })}
            <span class="search-result-meta">
                <time class="tabular-nums">{date}</time>
                <span class="tabular-nums">{format!("{reading_time} 分钟")}</span>
                {has_tags.then(|| view! { <span class="search-result-tags">{tags}</span> })}
            </span>
        </a>
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

    #[test]
    fn result_item_keeps_view_transition_markers_and_highlights_hits() {
        let mut item = post(7, Some(3), 1);
        item.tags = vec!["Rust".to_string(), "其他".to_string()];
        item.summary = Some("这里提到了 rust 语言".to_string());
        let html = Owner::new().with(|| {
            view! { <SearchResultItem post=item index=2 query="rust".to_string()/> }.to_html()
        });
        // erase_components 下 SSR 在动态文本后输出 `<!>` 水合标记；断言只看内容与属性。
        let html = html.replace("<!>", "");
        assert!(html.contains(r#"id="search-result-2""#));
        assert!(html.contains(r#"data-vt-post-link="7""#));
        assert!(html.contains(r#"data-vt-post-id="7""#));
        assert!(html.contains(r#"data-vt-role="title""#));
        assert!(html.contains(r#"href="/post/post-7""#));
        assert!(html.contains("search-result-tag is-hit"));
        assert!(html.contains("<mark>rust</mark>"));
    }

    #[test]
    fn history_state_restores_settled_results_only() {
        let failed = SearchHistoryState {
            failed: true,
            ..Default::default()
        };
        assert!(matches!(failed.result(), Some(Err(_))));
        assert!(SearchHistoryState::default().result().is_none());
    }
}
