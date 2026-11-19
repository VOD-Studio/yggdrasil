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
                    <SkeletonBox height="34px" width="34px" radius="0.5rem"/>
                    <SkeletonBox height="0.875rem" width="10rem" radius="0.25rem"/>
                </div>
                <div class="write-head-actions">
                    <SkeletonBox height="0.875rem" width="6rem" radius="0.25rem" style="--skeleton-margin-right: 0.5rem"/>
                    <SkeletonBox height="34px" width="4rem" radius="0.5rem"/>
                    <SkeletonBox height="34px" width="4rem" radius="0.5rem"/>
                </div>
            </div>
            <div class="write-body">
                <div class="write-main">
                    <div class="write-doc">
                        <SkeletonBox height="2.5rem" width="calc(4 / 5 * 100%)" radius="0.375rem"/>
                        <SkeletonBox height="0.875rem" width="16rem" radius="0.25rem" style="--skeleton-margin-top: 1rem"/>
                        <div class="write-rule"></div>
                        <div class="space-y-3">
                            <SkeletonBox height="1rem" width="100%" radius="0.25rem"/>
                            <SkeletonBox height="1rem" width="calc(11 / 12 * 100%)" radius="0.25rem"/>
                            <SkeletonBox height="1rem" width="calc(4 / 5 * 100%)" radius="0.25rem"/>
                            <SkeletonBox height="1rem" width="100%" radius="0.25rem" style="--skeleton-margin-top: 1.5rem"/>
                            <SkeletonBox height="1rem" width="calc(3 / 4 * 100%)" radius="0.25rem"/>
                        </div>
                    </div>
                </div>
                <div class="write-drawer-slot write-skeleton-drawer">
                    <div class="write-drawer">
                        <SkeletonBox height="1rem" width="5rem" radius="0.25rem"/>
                        {(0..3)
                            .map(|_| {
                                view! {
                                    <div>
                                        <SkeletonBox height="0.75rem" width="4rem" radius="0.25rem" style="--skeleton-margin-bottom: 0.5rem"/>
                                        <SkeletonBox height="2.25rem" width="100%" radius="0.5rem"/>
                                    </div>
                                }
                            })
                            .collect_view()}
                        <SkeletonBox width="100%" radius="10px" style="--skeleton-aspect-ratio: 21/9"/>
                    </div>
                </div>
            </div>
        </div>
    }
}
