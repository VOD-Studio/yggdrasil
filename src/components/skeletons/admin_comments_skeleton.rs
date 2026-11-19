//! 后台评论管理骨架屏
//!
//! 与评论管理同一版式：标题行、状态选项卡与评论流。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 评论流骨架（列表加载中使用）。
#[component]
pub fn AdminCommentsListSkeleton() -> impl IntoView {
    view! {
        <div class="admin-panel admin-comment-list" aria-hidden="true">
            {(0..5)
                .map(|_| {
                    view! {
                        <div class="admin-comment">
                            <SkeletonBox height="1rem" width="1rem" radius="0.25rem"/>
                            <SkeletonBox height="30px" width="30px" radius="9999px"/>
                            <div class="admin-comment-body">
                                <SkeletonBox height="0.875rem" width="10rem" radius="0.25rem"/>
                                <SkeletonBox height="1rem" width="calc(4 / 5 * 100%)" radius="0.25rem" style="--skeleton-margin-top: 0.625rem"/>
                                <SkeletonBox height="0.75rem" width="12rem" radius="0.25rem" style="--skeleton-margin-top: 0.75rem"/>
                            </div>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}

/// 评论管理全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn AdminCommentsSkeleton() -> impl IntoView {
    view! {
        <div class="admin-comments" aria-hidden="true">
            <AdminPageHeadSkeleton title_width="5rem" subtitle_width="7rem"/>
            <div class="admin-toolbar">
                <SkeletonBox height="2rem" width="20rem" radius="9999px"/>
            </div>
            <AdminCommentsListSkeleton/>
        </div>
    }
}
