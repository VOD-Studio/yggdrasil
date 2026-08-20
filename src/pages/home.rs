//! 首页模块。
//!
//! 对应路由：
//! - `/`：首页，默认展示第 1 页文章。
//! - `/page/:page`：分页首页，展示指定页码的已发布文章列表。
//!
//! 数据获取：通过 `use_server_future` 调用 `list_published_posts` server function，
//! 从服务端获取已发布文章的分页列表与总数，并渲染文章卡片与分页导航。
//! 在 `wasm32` 目标下，server function 的函数体被替换为向服务端端点发起 HTTP POST 请求的客户端存根；
//! 实际的数据库访问逻辑仅在 `feature = "server"` 启用时运行。

use dioxus::prelude::*;

use crate::api::posts::{list_published_posts, PostListResponse};
use crate::components::empty_state::EmptyState;
use crate::components::post_card::PostCard;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::home_skeleton::HomeSkeleton;
use crate::components::ui::Pagination;
use crate::router::Route;

// 每页展示的已发布文章数量，用于分页计算。
const POSTS_PER_PAGE: i32 = 10;

/// 首页组件，对应路由 `/`。
///
/// 直接委托给 `HomePage` 并固定页码为 1。
#[component]
pub fn Home() -> Element {
    rsx! {
        HomePage { page: 1 }
    }
}

/// 首页分页组件，对应路由 `/page/:page`。
///
/// 对传入的页码进行下限校正后，渲染头部信息与文章列表。
#[component]
pub fn HomePage(page: i32) -> Element {
    let current_page = page.max(1);

    rsx! {
        div { class: "animate-page-enter",
            if current_page == 1 {
                HomeHero {}
            } else {
                HomePaginatedHeader { current_page }
            }
            HomePosts { current_page }
        }
    }
}

/// 首页 Hero 组件：现代极简与诗意叙事的视觉锚点。
#[component]
fn HomeHero() -> Element {
    rsx! {
        div { class: "mt-6 sm:mt-8 mb-12 sm:mb-16 flex flex-col items-start gap-6",
            // 顶部微标：动态呼吸光标 + 沉静文案
            div { class: "inline-flex items-center gap-2.5 px-3.5 py-1.5 rounded-full text-xs font-medium bg-[var(--color-paper-accent)]/10 text-[var(--color-paper-accent)] border border-[var(--color-paper-accent)]/20 shadow-xs backdrop-blur-xs",
                span { class: "relative flex h-2 w-2",
                    span { class: "animate-ping absolute inline-flex h-full w-full rounded-full bg-[var(--color-paper-accent)] opacity-75" }
                    span { class: "relative inline-flex rounded-full h-2 w-2 bg-[var(--color-paper-accent)]" }
                }
                span { "专注于文字与思考" }
                span { class: "opacity-40", "·" }
                span { "Yggdrasil" }
            }

            // 核心标题与诗意导语
            div { class: "flex flex-col gap-3.5 max-w-2xl",
                h1 { class: "text-4xl sm:text-5xl md:text-6xl font-extrabold text-[var(--color-paper-primary)] tracking-tighter leading-[1.1]",
                    "世界遗忘的，"
                    br {}
                    span { class: "text-[var(--color-paper-accent)]", "树记得。" }
                }
                p { class: "text-base sm:text-lg text-[var(--color-paper-secondary)] font-normal leading-relaxed mt-1",
                    "极简、快速、现代。在数字世界树的枝叶间，拾取并珍藏每一篇文字、代码与思考。"
                }
            }

            // 快速探索栏（胶囊导航）
            div { class: "w-full pt-2 flex flex-wrap items-center gap-2.5 sm:gap-3 text-xs font-medium",
                Link {
                    to: Route::Search {},
                    class: "inline-flex items-center gap-2 px-4 py-2 rounded-full bg-[var(--color-paper-entry)] hover:bg-[var(--color-paper-border)]/60 text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]/50 transition-all duration-200 active:scale-[0.98] shadow-xs group",
                    svg {
                        class: "w-3.5 h-3.5 text-[var(--color-paper-accent)] group-hover:scale-110 transition-transform",
                        fill: "currentColor",
                        view_box: "0 -960 960 960",
                        path { d: "M784-120 532-372q-30 24-69 38t-83 14q-109 0-184.5-75.5T120-580q0-109 75.5-184.5T380-840q109 0 184.5 75.5T640-580q0 44-14 83t-38 69l252 252-56 56ZM380-400q75 0 127.5-52.5T560-580q0-75-52.5-127.5T380-760q-75 0-127.5 52.5T200-580q0 75 52.5 127.5T380-400Z" }
                    }
                    span { "搜索文章" }
                }
                Link {
                    to: Route::Archives {},
                    class: "inline-flex items-center gap-2 px-4 py-2 rounded-full bg-[var(--color-paper-entry)] hover:bg-[var(--color-paper-border)]/60 text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]/40 transition-all duration-200 active:scale-[0.98] group",
                    svg {
                        class: "w-3.5 h-3.5 text-[var(--color-paper-tertiary)] group-hover:text-[var(--color-paper-accent)] transition-colors",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        view_box: "0 0 24 24",
                        path { d: "M3 4v16a2 2 0 002 2h14a2 2 0 002-2V4M3 4h18M3 4l3-3h12l3 3M10 12h4" }
                    }
                    span { "时间归档" }
                }
                Link {
                    to: Route::Tags {},
                    class: "inline-flex items-center gap-2 px-4 py-2 rounded-full bg-[var(--color-paper-entry)] hover:bg-[var(--color-paper-border)]/60 text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]/40 transition-all duration-200 active:scale-[0.98] group",
                    svg {
                        class: "w-3.5 h-3.5 text-[var(--color-paper-tertiary)] group-hover:text-[var(--color-paper-accent)] transition-colors",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        view_box: "0 0 24 24",
                        path { d: "M7 7h.01M7 3h5c.512 0 1.024.195 1.414.586l7 7a2 2 0 010 2.828l-7 7a2 2 0 01-2.828 0l-7-7A1.994 1.994 0 013 12V7a4 4 0 014-4z" }
                    }
                    span { "分类标签" }
                }
                Link {
                    to: Route::Friends {},
                    class: "inline-flex items-center gap-2 px-4 py-2 rounded-full bg-[var(--color-paper-entry)] hover:bg-[var(--color-paper-border)]/60 text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]/40 transition-all duration-200 active:scale-[0.98] group",
                    svg {
                        class: "w-3.5 h-3.5 text-[var(--color-paper-tertiary)] group-hover:text-[var(--color-paper-accent)] transition-colors",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        view_box: "0 0 24 24",
                        path { d: "M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" }
                    }
                    span { "朋友们" }
                }
            }
        }
    }
}

/// 首页分页头部：在翻页模式下呈现精简的面包屑导航。
#[component]
fn HomePaginatedHeader(current_page: i32) -> Element {
    rsx! {
        div { class: "mt-4 mb-10 flex items-center justify-between pb-5 border-b border-[var(--color-paper-border)]/60",
            div { class: "flex items-baseline gap-3",
                h1 { class: "text-2xl sm:text-3xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                    "文章列表"
                }
                span { class: "text-sm text-[var(--color-paper-secondary)] font-medium",
                    "第 {current_page} 页"
                }
            }
            Link {
                to: Route::Home {},
                class: "text-xs font-semibold text-[var(--color-paper-accent)] hover:underline inline-flex items-center gap-1 transition-colors",
                "← 返回首页"
            }
        }
    }
}

/// 首页文章列表与分页展示组件。
#[component]
fn HomePosts(current_page: i32) -> Element {
    let router = dioxus::router::router();

    let posts_res = use_server_future(move || {
        let page = match router.current::<Route>() {
            Route::HomePage { page } => page.max(1),
            // / 路由（Route::Home）及其它变体：用 prop 兜底。
            _ => current_page,
        };
        list_published_posts(page, POSTS_PER_PAGE)
    })?;

    // 将结果映射为更便于本地使用的 (posts, total) 形式。
    let posts_data = posts_res.read().as_ref().map(|r| match r {
        Ok(PostListResponse { posts, total }) => Ok((posts.clone(), *total)),
        Err(e) => Err(e.to_string()),
    });

    match posts_data {
        Some(Ok((posts, total))) => {
            rsx! {
                if total > 0 {
                    // 列表前置小标题
                    div { class: "flex items-center justify-between mb-6",
                        div { class: "flex items-center gap-2.5",
                            span { class: "w-2 h-2 rounded-full bg-[var(--color-paper-accent)]" }
                            h2 { class: "text-base sm:text-lg font-bold text-[var(--color-paper-primary)] tracking-tight",
                                if current_page == 1 { "最新发布" } else { "文章列表" }
                            }
                        }
                        span { class: "text-xs text-[var(--color-paper-tertiary)] font-mono",
                            "共 {total} 篇"
                        }
                    }

                    // 文章卡片列表：第 1 页的第一篇若存在，作为 Featured 头条渲染
                    for (index, post) in posts.iter().enumerate() {
                        PostCard {
                            key: "{post.id}",
                            post: post.clone(),
                            featured: current_page == 1 && index == 0,
                        }
                    }

                    // 分页导航
                    Pagination {
                        variant: "frontend",
                        current_page,
                        total,
                        per_page: POSTS_PER_PAGE,
                        prev_route: if current_page - 1 <= 1 { Route::Home {} } else { Route::HomePage {
                            page: current_page - 1,
                        } },
                        next_route: Route::HomePage {
                            page: current_page + 1,
                        },
                        unit: "篇",
                    }
                } else {
                    EmptyState {
                        title: "还没有文章",
                        description: "这里会收集你写下的每一篇文字与思考。",
                    }
                }
            }
        }
        // 不透传内部错误细节，统一展示通用文案（与标签页等其它页面一致）。
        Some(Err(_)) => {
            rsx! {
                div { class: "text-center text-red-500 dark:text-red-400 py-20", "加载失败" }
            }
        }
        _ => {
            rsx! {
                DelayedSkeleton { HomeSkeleton {} }
            }
        }
    }
}
