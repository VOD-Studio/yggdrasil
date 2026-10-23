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
                <div class="comment-main flex flex-col gap-2">
                    <SkeletonBox class="h-3.5 w-24"/>
                    <SkeletonBox class="h-3 w-full"/>
                    <SkeletonBox class="h-3 w-2/3"/>
                </div>
            </div>
            <div class="comment is-child">
                <SkeletonBox class="comment-avatar"/>
                <div class="comment-main flex flex-col gap-2">
                    <SkeletonBox class="h-3.5 w-20"/>
                    <SkeletonBox class="h-3 w-3/4"/>
                </div>
            </div>
            <div class="comment">
                <SkeletonBox class="comment-avatar"/>
                <div class="comment-main flex flex-col gap-2">
                    <SkeletonBox class="h-3.5 w-28"/>
                    <SkeletonBox class="h-3 w-full"/>
                    <SkeletonBox class="h-3 w-1/2"/>
                </div>
            </div>
        </div>
    }
}
