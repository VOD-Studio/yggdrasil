//! 首页骨架屏：与「书桌」布局逐块对齐，数据到达后不跳动。
//! 最新一篇面板的右栏固定 6 行目录占位。

use leptos::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 目录占位每行的宽度，错落排布更像真实标题。
const OUTLINE_WIDTHS: [&str; 6] = [
    "width: 70%",
    "width: 90%",
    "width: 60%",
    "width: 85%",
    "width: 75%",
    "width: 50%",
];
/// 索引每行标题的宽度。
const ROW_WIDTHS: [&str; 5] = [
    "width: 70%",
    "width: 55%",
    "width: 80%",
    "width: 62%",
    "width: 74%",
];

#[component]
pub fn HomeSkeleton(#[prop(default = 1)] current_page: i32) -> impl IntoView {
    if current_page > 1 {
        return view! {
            <div class="home home-paged" role="status" aria-label="正在加载文章">
                <header class="home-paged-header">
                    <SkeletonBox height="1rem" width="5rem"/>
                    <SkeletonBox width="10rem" class="home-skeleton-title"/>
                    <SkeletonBox height="1rem" width="4rem"/>
                </header>
                <HomePostsSkeleton current_page=current_page/>
            </div>
        }
        .into_any();
    }
    view! {
        <div class="home" role="status" aria-label="正在加载首页">
            <div class="home-hero">
                <SkeletonBox height="0.75rem" width="14rem" style="--skeleton-max-width: 100%"/>
                <SkeletonBox width="22rem" class="home-skeleton-title" style="--skeleton-margin-top: 1.5rem; --skeleton-max-width: 100%"/>
                <SkeletonBox width="17rem" class="home-skeleton-title" style="--skeleton-margin-top: 0.75rem; --skeleton-max-width: 100%"/>
                <SkeletonBox height="0.875rem" width="28rem" style="--skeleton-margin-top: 1.5rem; --skeleton-max-width: 100%"/>
                <SkeletonBox class="home-skeleton-composer" style="--skeleton-margin-top: 2.25rem"/>
            </div>
            <div class="home-section">
                <SkeletonBox height="0.75rem" width="4rem" style="--skeleton-margin-bottom: 1rem"/>
                <div class="home-latest">
                    <div class="home-latest-main">
                        <SkeletonBox height="0.75rem" width="10rem"/>
                        <SkeletonBox height="1.75rem" width="calc(4 / 5 * 100%)" style="--skeleton-margin-top: 1rem"/>
                        <SkeletonBox height="0.75rem" width="100%" style="--skeleton-margin-top: 1rem"/>
                        <SkeletonBox height="0.75rem" width="calc(3 / 4 * 100%)" style="--skeleton-margin-top: 0.625rem"/>
                    </div>
                    <div class="home-latest-outline">
                        <SkeletonBox height="0.75rem" width="4rem" style="--skeleton-margin-bottom: 0.75rem"/>
                        <div class="home-skeleton-lines">
                            {OUTLINE_WIDTHS
                                .iter()
                                .map(|width| view! { <SkeletonBox height="0.75rem" style=*width/> })
                                .collect_view()}
                        </div>
                    </div>
                </div>
            </div>
            <HomePostsSkeleton current_page=current_page/>
        </div>
    }
    .into_any()
}

/// 文章索引占位：月份分隔与五行条目。
#[component]
pub fn HomePostsSkeleton(current_page: i32) -> impl IntoView {
    view! {
        <div class="home-index" aria-hidden="true">
            {(current_page == 1)
                .then(|| {
                    view! {
                        <div class="home-index-heading">
                            <SkeletonBox height="1.25rem" width="7rem"/>
                            <SkeletonBox height="1rem" width="3rem"/>
                        </div>
                    }
                })}
            <div class="home-month">
                <SkeletonBox height="0.75rem" width="4rem" class="home-month-label"/>
                {ROW_WIDTHS
                    .iter()
                    .map(|width| {
                        view! {
                            <div class="home-row home-row-skeleton">
                                <SkeletonBox height="0.75rem" width="2.5rem"/>
                                <div>
                                    <SkeletonBox height="1rem" style=*width/>
                                    <SkeletonBox height="0.75rem" width="90%" style="--skeleton-margin-top: 0.5rem"/>
                                </div>
                                <SkeletonBox height="0.75rem" width="3.5rem" class="home-row-meta"/>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}
