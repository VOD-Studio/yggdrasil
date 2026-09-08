//! 前台布局组件
//!
//! 包裹所有前台路由，提供统一的 Header、Footer 与主内容区容器，
//! 并为不同路由在 SuspenseBoundary 中展示对应的骨架屏。

use dioxus::prelude::*;

use crate::components::footer::Footer;
use crate::components::header::{Header, SearchIconLink};
use crate::components::nav::build_nav_items;
use crate::components::skeletons::archive_skeleton::ArchiveSkeleton;
use crate::components::skeletons::changelog_skeleton::ChangelogSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::friends_skeleton::FriendsSkeleton;
use crate::components::skeletons::home_skeleton::HomeSkeleton;
use crate::components::skeletons::post_detail_skeleton::PostDetailSkeleton;
use crate::components::skeletons::search_skeleton::SearchSkeleton;
use crate::components::skeletons::tags_skeleton::TagDetailSkeleton;
use crate::router::Route;
use crate::theme::ThemeToggle;

/// 根据当前前台路由选择对应的骨架屏组件。
fn route_skeleton(route: &Route) -> Element {
    match route {
        Route::HomePage { page } => rsx! {
            DelayedSkeleton { HomeSkeleton { current_page: (*page).max(1) } }
        },
        Route::Archives {} => rsx! {
            DelayedSkeleton { ArchiveSkeleton {} }
        },
        Route::TagDetail { .. } => rsx! {
            DelayedSkeleton { TagDetailSkeleton {} }
        },
        Route::Friends {} => rsx! {
            DelayedSkeleton { FriendsSkeleton {} }
        },
        Route::Search {} => rsx! {
            DelayedSkeleton { SearchSkeleton {} }
        },
        Route::PostDetail { .. } => rsx! {
            DelayedSkeleton { PostDetailSkeleton {} }
        },
        Route::Changelog {} => rsx! {
            DelayedSkeleton { ChangelogSkeleton {} }
        },
        Route::NotFound { .. } => rsx! {
            div { class: "py-20 md:py-28" }
        },
        _ => rsx! {
            DelayedSkeleton { HomeSkeleton {} }
        },
    }
}

/// 前台整体布局组件。
///
/// 负责渲染 Header（含前台导航与主题切换）、主内容区与 Footer，
/// 并在路由内容加载过程中显示与路由匹配的骨架屏。
#[component]
pub fn FrontendLayout() -> Element {
    let route = use_route::<Route>();
    let nav_items = build_nav_items(route.clone());
    // 首页与其它前台页面共用阅读宽度，导航和正文保持对齐。
    let max_width = "max-w-4xl";

    rsx! {
        div { class: "min-h-screen flex flex-col bg-paper-theme",
            Header {
                max_width,
                nav_items,
                right_content: rsx! {
                    SearchIconLink {}
                    ThemeToggle {}
                },
            }
            main { class: "flex-1 w-full {max_width} mx-auto px-6 py-6 md:py-12 overflow-x-clip",
                SuspenseBoundary { fallback: move |_| route_skeleton(&route), Outlet::<Route> {} }
            }
            Footer {}
        }
    }
}
