//! 文章详情页骨架屏
//!
//! 与文章页逐块对齐：面包屑、两行标题、元信息、发丝线与正文段落。

use crate::components::skeletons::atoms::*;
use dioxus::prelude::*;

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
pub fn PostDetailBody() -> Element {
    rsx! {
        div { class: "post-header",
            SkeletonBox { class: "h-3 w-28" }
            SkeletonBox { class: "post-skeleton-title mt-5" }
            SkeletonBox { class: "post-skeleton-title mt-2.5 w-3/5" }
            SkeletonBox { class: "h-3 mt-6 w-72 max-w-full" }
        }
        div { class: "post-skeleton-body",
            for (i, width) in PARAGRAPH_WIDTHS.iter().enumerate() {
                SkeletonBox { key: "{i}", class: "h-3.5", style: *width }
            }
            SkeletonBox { class: "post-skeleton-block" }
        }
    }
}

/// 文章详情页骨架屏组件。
#[component]
pub fn PostDetailSkeleton() -> Element {
    rsx! {
        div { class: "post-layout",
            article { class: "post-single", role: "status", aria_label: "正在加载文章",
                PostDetailBody {}
            }
        }
    }
}
