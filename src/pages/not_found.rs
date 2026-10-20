//! 404 页面模块。
//!
//! 对应路由 `/:..segments`。
//!
//! 当用户访问未匹配任何前端路由的 URL 时，Dioxus Router 会回退到该 404 页面。
//! 一句话说明、写错的地址、返回首页 / 搜索，以及按地址里的词匹配到的「也许你在找」。

use dioxus::prelude::*;

use crate::api::posts::{recommend_for_path, PathRecommendations};
use crate::components::ui::{Icon, IconName};
use crate::router::Route;

/// 404 页面组件，对应兜底路由 `/:..segments`。
///
/// 展示 404 标记、说明、当前地址、返回首页与搜索，以及推荐文章。
///
/// # 两种命中路径
/// 本组件在两种完全不同的机制下被渲染：
/// 1. **路由匹配** —— 访问任意未命中路径，Router 命中 catch-all `Route::NotFound`，
///    此时 `ErrorBoundary` **无错误**，本组件作为 children（Outlet）正常渲染。
/// 2. **错误冒泡** —— 如 `PostDetail` 对不存在的 slug 抛出 `ServerFnError(404)`，
///    `ErrorLayout` 的 `ErrorBoundary` 捕获后在 fallback 里渲染本组件。
///
/// 「返回首页」必须在导航前清除可能存在的错误边界，否则场景 2 会卡死：
/// ErrorBoundary 持有错误时不渲染 children（Outlet），路由虽切到 `Home`，
/// 页面仍停留在 fallback（本组件），表现为「URL 变了但页面不变」。
/// 场景 1 下没有错误，`clear_errors` 是 no-op。
#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    let _ = segments;

    // Commit 404 status code during server-side rendering
    #[cfg(feature = "server")]
    {
        dioxus::fullstack::FullstackContext::commit_http_status(
            http::StatusCode::NOT_FOUND,
            Some("Page Not Found".to_string()),
        );
    }
    // 错误冒泡时路由仍是原页面（如 PostDetail），地址同样取当前路由。
    let path = use_route::<Route>().to_string();

    rsx! {
        document::Title { "页面不存在 · Yggdrasil" }
        div { class: "not-found page-enter",
            p { class: "not-found-code tabular-nums", "404" }
            h1 { class: "not-found-title", "这个页面不存在" }
            p { class: "not-found-description", "地址可能写错了，或者页面已经移走。" }
            p { class: "not-found-path", code { title: "{path}", "{path}" } }
            div { class: "not-found-actions",
                // 返回首页：先清除错误边界再导航（见组件顶部文档场景 2）。
                button {
                    r#type: "button",
                    class: "btn btn-primary btn-lg",
                    onclick: move |_| {
                        if let Some(ctx) = try_consume_context::<ErrorContext>() {
                            ctx.clear_errors();
                        }
                        let _ = dioxus::router::navigator().push(Route::Home {});
                    },
                    "返回首页"
                }
                // 错误冒泡时本页渲染在前台布局之外，取不到搜索面板，改为链接到搜索页。
                Link {
                    class: "btn btn-secondary btn-lg",
                    to: Route::Search { q: String::new() },
                    onclick: move |_| {
                        if let Some(ctx) = try_consume_context::<ErrorContext>() {
                            ctx.clear_errors();
                        }
                    },
                    Icon { name: IconName::Search, size: 16 }
                    "搜索"
                }
            }
            Recommendations { key: "{path}", path: path.clone() }
        }
    }
}

/// 「也许你在找」：无命中或请求失败时整块不显示。
///
/// 只在浏览器里请求（effect 不在 SSR 中执行）：本组件也会渲染在错误边界的
/// fallback 里，那里挂起服务端 future 会让水合数据与 DOM 对不上。
/// 地址变化时由调用方以 `key` 重建本组件。
#[component]
fn Recommendations(path: String) -> Element {
    let mut result = use_signal(|| None::<PathRecommendations>);
    use_effect(move || {
        let path = path.clone();
        spawn(async move {
            if let Ok(found) = recommend_for_path(path).await {
                result.set(Some(found));
            }
        });
    });
    let Some(found) = result() else {
        return rsx! {};
    };
    if found.posts.is_empty() {
        return rsx! {};
    }
    let words = found.words.join("」「");
    rsx! {
        section { class: "not-found-maybe", aria_labelledby: "not-found-maybe-title",
            h2 { id: "not-found-maybe-title", class: "section-label", "也许你在找" }
            ul {
                for post in found.posts {
                    li { key: "{post.slug}",
                        // 错误冒泡场景下先清除错误边界，导航后才会渲染新页面。
                        Link {
                            class: "not-found-row",
                            to: Route::PostDetail { slug: post.slug.clone() },
                            onclick: move |_| {
                                if let Some(ctx) = try_consume_context::<ErrorContext>() {
                                    ctx.clear_errors();
                                }
                            },
                            span { "{post.title}" }
                            if let Some(at) = post.published_at {
                                time { class: "tabular-nums", {at.format("%Y.%m.%d").to_string()} }
                            }
                        }
                    }
                }
            }
            p { class: "not-found-note", "按地址里的词「{words}」匹配到的文章。" }
        }
    }
}
