//! 写文章页骨架屏
//!
//! 与写作页同一版式：页头（返回、面包屑、保存状态与按钮）、居中的标题与正文、
//! 右侧属性抽屉。路由级 fallback 与编辑器初始化时共用。

use crate::components::skeletons::atoms::SkeletonBox;
use leptos::prelude::*;

/// 写文章页骨架屏组件。
#[component]
pub fn WriteSkeleton() -> impl IntoView {
    view! {
        <div class="write-page" aria-hidden="true">
            <div class="write-head">
                <div class="write-head-left">
                    <SkeletonBox class="h-[34px] w-[34px] rounded-lg"/>
                    <SkeletonBox class="h-3.5 w-40 rounded"/>
                </div>
                <div class="write-head-actions">
                    <SkeletonBox class="h-3.5 w-24 rounded mr-2"/>
                    <SkeletonBox class="h-[34px] w-16 rounded-lg"/>
                    <SkeletonBox class="h-[34px] w-16 rounded-lg"/>
                </div>
            </div>
            <div class="write-body">
                <div class="write-main">
                    <div class="write-doc">
                        <SkeletonBox class="h-10 w-4/5 rounded-md"/>
                        <SkeletonBox class="h-3.5 w-64 rounded mt-4"/>
                        <div class="write-rule"></div>
                        <div class="space-y-3">
                            <SkeletonBox class="h-4 w-full rounded"/>
                            <SkeletonBox class="h-4 w-11/12 rounded"/>
                            <SkeletonBox class="h-4 w-4/5 rounded"/>
                            <SkeletonBox class="h-4 w-full rounded mt-6"/>
                            <SkeletonBox class="h-4 w-3/4 rounded"/>
                        </div>
                    </div>
                </div>
                <div class="write-drawer-slot write-skeleton-drawer">
                    <div class="write-drawer">
                        <SkeletonBox class="h-4 w-20 rounded"/>
                        {(0..3)
                            .map(|_| {
                                view! {
                                    <div>
                                        <SkeletonBox class="h-3 w-16 rounded mb-2"/>
                                        <SkeletonBox class="h-9 w-full rounded-lg"/>
                                    </div>
                                }
                            })
                            .collect_view()}
                        <SkeletonBox class="w-full aspect-[21/9] rounded-[10px]"/>
                    </div>
                </div>
            </div>
        </div>
    }
}
