//! 归档与标签详情骨架屏：与真实版式逐块对齐（摘要行、标签区、年份分组）。

use leptos::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::pages::tags::TagIntro;

/// 标签占位的宽度，错落排布更像真实标签。
const CHIP_WIDTHS: [&str; 10] = [
    "width: 4.5rem",
    "width: 3.5rem",
    "width: 5.5rem",
    "width: 4rem",
    "width: 6rem",
    "width: 3.75rem",
    "width: 5rem",
    "width: 4.25rem",
    "width: 3.25rem",
    "width: 5.25rem",
];
/// 文章行标题的宽度。
const ROW_WIDTHS: [&str; 5] = [
    "width: 72%",
    "width: 58%",
    "width: 80%",
    "width: 64%",
    "width: 50%",
];

/// 归档页整页占位：页标题之下的全部区块。
#[component]
pub fn ArchiveSkeleton() -> impl IntoView {
    view! {
        <div class="archive">
            <header class="archive-header">
                <h1 class="archive-title">"归档"</h1>
            </header>
            <ArchiveBodySkeleton/>
        </div>
    }
}

/// 标签详情整页占位：开场照常显示，只有数据区占位。
#[component]
pub fn TagDetailSkeleton(#[prop(into)] tag: String) -> impl IntoView {
    view! {
        <div class="archive">
            <TagIntro tag=tag/>
            <ArchiveBodySkeleton/>
        </div>
    }
}

/// 摘要行、标签区与两个年份分组的占位。
#[component]
pub fn ArchiveBodySkeleton() -> impl IntoView {
    view! {
        <div role="status" aria-label="正在加载文章">
            <div aria-hidden="true">
                <SkeletonBox class="archive-summary h-4 w-60 max-w-full"/>
                <div class="archive-tags">
                    <div class="archive-tags-head">
                        <SkeletonBox class="h-3 w-14"/>
                    </div>
                    <div class="archive-tags-list">
                        {CHIP_WIDTHS
                            .iter()
                            .map(|width| view! { <SkeletonBox class="h-7 rounded-full" style=*width/> })
                            .collect_view()}
                    </div>
                </div>
                <div class="archive-years">
                    {(0..2)
                        .map(|_| {
                            view! {
                                <div class="archive-year">
                                    <div class="archive-year-label">
                                        <SkeletonBox class="h-8 w-20"/>
                                        <SkeletonBox class="h-3 w-10 mt-2"/>
                                    </div>
                                    <div class="archive-rows">
                                        {ROW_WIDTHS
                                            .iter()
                                            .map(|width| {
                                                view! {
                                                    <div class="archive-entry archive-entry-skeleton">
                                                        <SkeletonBox class="h-3 w-10"/>
                                                        <SkeletonBox class="h-4" style=*width/>
                                                    </div>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
        </div>
    }
}
