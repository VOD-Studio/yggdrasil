//! 文章详情页骨架屏
//!
//! 与文章页逐块对齐：面包屑、两行标题、元信息、发丝线与正文段落。

use crate::components::skeletons::atoms::*;
use leptos::prelude::*;

/// 正文段落占位的宽度，错落排布更像真实段落。
const PARAGRAPH_WIDTHS: [&str; 8] = [
    "width: 100%",
    "width: 96%",
    "width: 88%",
    "width: 100%",
    "width: 72%",
    "width: 94%",
    "width: 100%",
    "width: 60%",
];

/// 文章详情/预览页共享的正文骨架屏片段。供 [`PostDetailSkeleton`] 与
/// [`crate::components::skeletons::post_preview_skeleton::PostPreviewSkeleton`]
/// 复用，两者视觉上唯一差异是预览页顶部多一段预览横幅。
#[component]
pub fn PostDetailBody() -> impl IntoView {
    view! {
        <div class="post-header">
            <SkeletonBox height="0.75rem" width="7rem"/>
            <SkeletonBox class="post-skeleton-title" style="--skeleton-margin-top: 1.25rem"/>
            <SkeletonBox width="calc(3 / 5 * 100%)" class="post-skeleton-title" style="--skeleton-margin-top: 0.625rem"/>
            <SkeletonBox height="0.75rem" width="18rem" style="--skeleton-margin-top: 1.5rem; --skeleton-max-width: 100%"/>
        </div>
        <div class="post-skeleton-body">
            {PARAGRAPH_WIDTHS
                .iter()
                .map(|width| view! { <SkeletonBox height="0.875rem" style=*width/> })
                .collect_view()}
            <SkeletonBox class="post-skeleton-block"/>
        </div>
    }
}

/// 文章详情页骨架屏组件。
#[component]
pub fn PostDetailSkeleton() -> impl IntoView {
    view! {
        <div class="post-layout">
            <article class="post-single" role="status" aria-label="正在加载文章">
                <PostDetailBody/>
            </article>
        </div>
    }
}
