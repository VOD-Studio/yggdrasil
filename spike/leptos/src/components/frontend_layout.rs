//! 前台布局组件
//!
//! 包裹所有前台路由，提供统一的 Header、Footer 与主内容区容器，
//! 并为不同路由在 Suspense 中展示对应的骨架屏。
//!
//! 布局在主包里只构建一次（Leptos 组件体只运行一次），随路由变化的部分
//! （导航激活项、主区宽度、站点分享信息）都从 `use_location().pathname` 派生。

use leptos::children::ViewFn;
use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::hooks::use_location;

use crate::components::footer::Footer;
use crate::components::header::{Header, SearchIconLink};
use crate::components::nav::build_nav_items;
use crate::components::share_meta::SiteShareMeta;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::home_skeleton::HomeSkeleton;
use crate::components::skeletons::post_detail_skeleton::PostDetailSkeleton;
use crate::theme::ThemeToggle;

/// 是否为文章页：文章页由 PostDetail 写自己的分享元信息，主区放宽。
fn is_post_path(path: &str) -> bool {
    path.starts_with("/post/")
}

/// 根据当前前台路径选择对应的骨架屏组件。原型只有首页与文章详情两条前台路由。
fn route_skeleton(path: &str) -> AnyView {
    if is_post_path(path) {
        view! { <DelayedSkeleton><PostDetailSkeleton/></DelayedSkeleton> }.into_any()
    } else {
        // 首页与其余路径（Dioxus 版的 `_` 分支）都用首页骨架。
        view! { <DelayedSkeleton><HomeSkeleton/></DelayedSkeleton> }.into_any()
    }
}

/// 前台整体布局组件。
///
/// 负责渲染 Header（含前台导航与主题切换）、主内容区与 Footer，
/// 并在路由内容加载过程中显示与路由匹配的骨架屏。
#[component]
pub fn FrontendLayout() -> impl IntoView {
    let pathname = use_location().pathname;
    let nav_items = Memo::new(move |_| build_nav_items(&pathname.get()));
    // 页头与页脚在所有前台页面统一为 880px 索引宽度，栏目切换时外壳不跳动。
    // 主区按页面放宽：文章页 1240px（680px 正文 + 右侧目录），其余与页头同宽。
    let max_width = "max-w-[880px]";
    let is_post = Memo::new(move |_| is_post_path(&pathname.get()));
    let main_width = Signal::derive(move || {
        if is_post.get() {
            "max-w-[1240px]"
        } else {
            max_width
        }
    });

    view! {
        <FrontendShell
            max_width=main_width
            header=move || view! {
                <Header
                    max_width=max_width
                    nav_items=nav_items
                    right_content=move || view! {
                        // 搜索面板不在原型范围内：入口为指向 /search 的普通链接。
                        <SearchIconLink/>
                        <ThemeToggle icon_only=true/>
                    }
                />
            }
            footer=move || view! { <Footer max_width=max_width/> }
            main_content=move || view! {
                <Show when=move || !is_post.get()>
                    <SiteShareMeta path=pathname/>
                </Show>
                // 页面自己的 Suspense 先接住数据读取；这里兜底未被包住的资源读取，
                // 与 Dioxus 版包在 Outlet 外的 SuspenseBoundary 一致。
                <Suspense fallback=move || route_skeleton(&pathname.get_untracked())>
                    <Outlet/>
                </Suspense>
            }
        />
    }
}

/// 前台页面共用的布局关系；路由内容由外层提供。
#[component]
pub(crate) fn FrontendShell(
    #[prop(into)] header: ViewFn,
    #[prop(into)] main_content: ViewFn,
    #[prop(into)] footer: ViewFn,
    #[prop(into, default = "max-w-[880px]".into())] max_width: Signal<&'static str>,
    #[prop(default = "min-h-screen")] min_height: &'static str,
) -> impl IntoView {
    view! {
        <div class=format!("{min_height} flex flex-col bg-surface-0")>
            {header.run()}
            <main class=move || {
                format!(
                    "flex-1 w-full {} mx-auto px-6 py-6 md:py-12 overflow-x-clip",
                    max_width.get(),
                )
            }>
                {main_content.run()}
            </main>
            {footer.run()}
        </div>
    }
}
