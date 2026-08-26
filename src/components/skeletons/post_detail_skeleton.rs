//! 文章详情页骨架屏
//!
//! 在文章详情数据加载期间展示面包屑、标题、摘要、元信息、封面与正文占位。

use crate::components::skeletons::atoms::*;
use dioxus::prelude::*;

/// 文章详情/预览页共享的正文骨架屏片段：面包屑 + 标题 + 摘要 + 元信息 + 封面 +
/// 正文段落 + 页脚占位。供 [`PostDetailSkeleton`] 与
/// [`crate::components::skeletons::post_preview_skeleton::PostPreviewSkeleton`]
/// 复用，两者视觉上唯一差异是预览页顶部多一段预览横幅。
#[component]
pub fn PostDetailBody() -> Element {
    rsx! {
        // 面包屑占位
        SkeletonBox { class: "h-4 w-48 rounded mb-6" }

        // 标题占位 (模拟 h1)
        SkeletonBox { class: "h-10 w-4/5 rounded mb-4" }

        // 摘要占位
        SkeletonBox { class: "h-5 w-2/3 rounded mb-4" }

        // 元信息行 (作者 · 日期 · 阅读时间)
        div { class: "flex items-center gap-2 mb-8",
            SkeletonBox { class: "h-4 w-16 rounded" }
            SkeletonBox { class: "h-4 w-1 rounded" }
            SkeletonBox { class: "h-4 w-24 rounded" }
            SkeletonBox { class: "h-4 w-1 rounded" }
            SkeletonBox { class: "h-4 w-20 rounded" }
        }

        // 封面图占位 (模拟 PostCover 16:9 比例)
        SkeletonBox { class: "w-full aspect-video rounded-lg mb-8" }

        // 正文段落占位 (模拟多段 PostContent)
        div { class: "space-y-4 mb-8",
            SkeletonBox { class: "h-4 w-full rounded" }
            SkeletonBox { class: "h-4 w-full rounded" }
            SkeletonBox { class: "h-4 w-5/6 rounded" }
            SkeletonBox { class: "h-4 w-full rounded" }
            SkeletonBox { class: "h-4 w-full rounded" }
            SkeletonBox { class: "h-4 w-3/4 rounded" }
            // 空行模拟段落间距
            div { class: "h-2" }
            SkeletonBox { class: "h-4 w-full rounded" }
            SkeletonBox { class: "h-4 w-full rounded" }
            SkeletonBox { class: "h-4 w-2/3 rounded" }
        }

        // PostFooter 占位
        div { class: "border-t border-gray-200 dark:border-gray-700 pt-6 mt-8",
            div { class: "flex items-center justify-between",
                SkeletonBox { class: "h-4 w-32 rounded" }
                SkeletonBox { class: "h-4 w-24 rounded" }
            }
        }
    }
}

/// 文章详情页骨架屏组件。
///
/// 结构：面包屑 + 标题(h1) + 摘要 + 元信息 + 封面图 + 正文(多段) + 页脚占位。
#[component]
pub fn PostDetailSkeleton() -> Element {
    rsx! {
        article { class: "post-single",
            PostDetailBody {}
        }
    }
}
