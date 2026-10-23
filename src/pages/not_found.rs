//! 404 与页面加载失败页面。
//!
//! [`NotFound`]：访问未命中任何路由的地址，或页面数据报告“不存在”时渲染。
//! 一句话说明、写错的地址、返回首页 / 搜索，以及按地址里的词匹配到的「也许你在找」。
//! [`PageLoadFailed`]：页面数据加载失败（非 404）时的兜底卡片。

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::hooks::use_location;

use crate::api::posts::{recommend_for_path, PathRecommendations};
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::ui::{Icon, IconName};

/// 404 页面组件。
///
/// 展示 404 标记、说明、当前地址、返回首页与搜索，以及推荐文章。渲染时把响应状态置为 404
/// （仅 `SsrMode::Async` 路由与未匹配路由的响应头尚可修改，其余模式见迁移方案“假设与预案”）。
#[component]
pub fn NotFound() -> impl IntoView {
    crate::request_ctx::set_status(404);
    let location = use_location();
    // 当前地址（路径 + 查询），错误冒泡时路由仍是原页面，同样取当前地址。
    let path = Memo::new(move |_| {
        let pathname = location.pathname.get();
        let search = location.search.get();
        if search.is_empty() {
            pathname
        } else {
            format!("{pathname}?{search}")
        }
    });

    view! {
        <Title text="页面不存在 · Yggdrasil"/>
        <div class="not-found page-enter">
            <p class="not-found-code tabular-nums">"404"</p>
            <h1 class="not-found-title">"这个页面不存在"</h1>
            <p class="not-found-description">"地址可能写错了，或者页面已经移走。"</p>
            <p class="not-found-path">
                <code title=move || path.get()>{move || path.get()}</code>
            </p>
            <div class="not-found-actions">
                <button
                    type="button"
                    class="btn btn-primary btn-lg"
                    on:click=move |_| crate::navigation::push("/")
                >
                    "返回首页"
                </button>
                <a class="btn btn-secondary btn-lg" href="/search">
                    <Icon name=IconName::Search size=16/>
                    "搜索"
                </a>
            </div>
            <Recommendations path=path/>
        </div>
    }
}

/// 页面加载失败（非 404）：网络或服务出了点问题，可返回首页。
///
/// 渲染时把响应状态置为 500（同样只在响应头尚可修改时生效）。
#[component]
pub fn PageLoadFailed() -> impl IntoView {
    crate::request_ctx::set_status(500);
    view! {
        <div class="flex min-h-[50vh] items-center justify-center px-6 md:min-h-[55vh] animate-page-enter">
            <EmptyState
                icon=IconName::CircleAlert
                title="页面加载失败"
                description="网络或服务出了点问题，稍后再试一次。"
                action=EmptyStateAction::new("返回首页", move |_| crate::navigation::push("/"))
            />
        </div>
    }
}

/// 「也许你在找」：无命中或请求失败时整块不显示。
///
/// 只在浏览器里请求（Effect 不在 SSR 中执行）；地址变化时重新请求，
/// 用世代号丢弃过期响应。
#[component]
fn Recommendations(#[prop(into)] path: Signal<String>) -> impl IntoView {
    let result = RwSignal::new(None::<PathRecommendations>);
    let generation = StoredValue::new(0u64);
    Effect::new(move |_| {
        let path = path.get();
        let current = generation.get_value() + 1;
        generation.set_value(current);
        result.set(None);
        spawn_local(async move {
            if let Ok(found) = recommend_for_path(path).await {
                if generation.try_get_value() == Some(current) {
                    let _ = result.try_set(Some(found));
                }
            }
        });
    });

    move || {
        let found = result
            .try_get()
            .flatten()
            .filter(|found| !found.posts.is_empty())?;
        let words = found.words.join("」「");
        Some(view! {
            <section class="not-found-maybe" aria-labelledby="not-found-maybe-title">
                <h2 id="not-found-maybe-title" class="section-label">
                    "也许你在找"
                </h2>
                <ul>
                    {found
                        .posts
                        .into_iter()
                        .map(|post| {
                            let href = crate::routes::Route::PostDetail {
                                slug: post.slug.clone(),
                            }
                                .to_string();
                            let date = post
                                .published_at
                                .map(|at| {
                                    view! {
                                        <time class="tabular-nums">
                                            {at.format("%Y.%m.%d").to_string()}
                                        </time>
                                    }
                                });
                            view! {
                                <li>
                                    <a class="not-found-row" href=href>
                                        <span>{post.title}</span>
                                        {date}
                                    </a>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
                <p class="not-found-note">{format!("按地址里的词「{words}」匹配到的文章。")}</p>
            </section>
        })
    }
}
