//! 首页模块。
//!
//! 对应路由：
//! - `/`：首页，默认展示第 1 页文章。
//! - `/page/:page`：分页首页，展示指定页码的已发布文章列表。
//!
//! 数据获取：通过 `use_server_future` 调用 `list_published_posts` server function，
//! 从服务端获取已发布文章的分页列表与总数。首页直接展示居中的单栏文章流，
//! 所有文章统一使用紧凑列表，不区分头条样式；分页仍订阅路由变化。
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
/// 对传入的页码进行下限校正后，渲染文章列表。
#[component]
pub fn HomePage(page: i32) -> Element {
    let current_page = page.max(1);

    rsx! {
        section { class: "animate-page-enter", aria_label: "文章列表",
            h1 { class: "sr-only", "文章列表 · 第 {current_page} 页" }
            HomePosts { current_page }
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

    // 借用列表，仅在传入组件 props 时克隆单篇文章，避免复制整个分页结果。
    let posts_data = posts_res.read();
    match posts_data.as_ref() {
        Some(Ok(PostListResponse { posts, total })) => {
            let total = *total;
            rsx! {
                if total > 0 {
                    for post in posts.iter() {
                        PostCard {
                            key: "{post.id}",
                            post: post.clone(),
                            compact: true,
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
                EmptyState {
                    title: "文章暂时无法加载",
                    description: "请稍后刷新页面重试。",
                }
            }
        }
        _ => {
            rsx! {
                DelayedSkeleton { HomeSkeleton {} }
            }
        }
    }
}
