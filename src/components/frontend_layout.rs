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
        Route::Notes {} | Route::NoteDetail { .. } | Route::NotebookDetail { .. } => rsx! {
            DelayedSkeleton { crate::pages::notes::NotesSkeleton {} }
        },
        Route::HomePage { page } => rsx! {
            DelayedSkeleton { HomeSkeleton { current_page: (*page).max(1) } }
        },
        Route::Archives {} => rsx! {
            DelayedSkeleton { ArchiveSkeleton {} }
        },
        Route::TagDetail { tag } => rsx! {
            TagDetailSkeleton { tag: tag.clone() }
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
        Route::WritingGuide {} => rsx! {
            DelayedSkeleton {
                p { class: "py-20 text-sm text-paper-secondary", role: "status", "正在载入写作指南…" }
            }
        },
        Route::ComponentShowcase {} | Route::ComponentDetail { .. } => rsx! {
            div { class: "py-20", role: "status", "正在翻开组件图鉴…" }
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
    let max_width = if matches!(
        route,
        Route::ComponentShowcase {} | Route::ComponentDetail { .. }
    ) {
        "max-w-6xl"
    } else {
        "max-w-4xl"
    };

    rsx! {
        FrontendShell {
            max_width,
            header: rsx! { Header {
                max_width,
                nav_items,
                right_content: rsx! {
                    SearchIconLink {}
                    ThemeToggle {}
                },
            } },
            footer: rsx! { Footer {} },
            main_content: rsx! {
                SuspenseBoundary { fallback: move |_| route_skeleton(&route), Outlet::<Route> {} }
            },
        }
    }
}

/// 前台页面与图鉴样例共用的布局关系；路由内容由外层提供。
#[component]
pub(crate) fn FrontendShell(
    header: Element,
    main_content: Element,
    footer: Element,
    #[props(default = "max-w-4xl")] max_width: &'static str,
    #[props(default = "min-h-screen")] min_height: &'static str,
) -> Element {
    rsx! {
        div { class: "{min_height} flex flex-col bg-paper-theme",
            {header}
            main { class: "flex-1 w-full {max_width} mx-auto px-6 py-6 md:py-12 overflow-x-clip",
                {main_content}
            }
            {footer}
        }
    }
}
