//! 评论列表骨架屏
//!
//! 在评论数据加载期间展示评论条目占位，结构和间距与真实评论列表一致。

use crate::components::skeletons::atoms::*;
use leptos::prelude::*;

/// 评论列表骨架屏组件：两条顶层评论夹一条回复，复用评论列表的类名。
#[component]
pub fn CommentListSkeleton() -> impl IntoView {
    view! {
        <div class="comment-list" aria-hidden="true">
            <div class="comment">
                <SkeletonBox class="comment-avatar"/>
                <div class="comment-main comment-skeleton-lines">
                    <SkeletonBox height="0.875rem" width="6rem"/>
                    <SkeletonBox height="0.75rem" width="100%"/>
                    <SkeletonBox height="0.75rem" width="calc(2 / 3 * 100%)"/>
                </div>
            </div>
            <div class="comment is-child">
                <SkeletonBox class="comment-avatar"/>
                <div class="comment-main comment-skeleton-lines">
                    <SkeletonBox height="0.875rem" width="5rem"/>
                    <SkeletonBox height="0.75rem" width="calc(3 / 4 * 100%)"/>
                </div>
            </div>
            <div class="comment">
                <SkeletonBox class="comment-avatar"/>
                <div class="comment-main comment-skeleton-lines">
                    <SkeletonBox height="0.875rem" width="7rem"/>
                    <SkeletonBox height="0.75rem" width="100%"/>
                    <SkeletonBox height="0.75rem" width="calc(1 / 2 * 100%)"/>
                </div>
            </div>
        </div>
    }
}
