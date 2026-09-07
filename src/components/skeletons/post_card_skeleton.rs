//! 文章卡片骨架屏
//!
//! 模拟 PostCard 组件的视觉占位，用于列表页加载。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 文章卡片骨架屏组件。
/// Props:
/// - `has_cover`：是否展示封面占位
/// - `compact`：首页紧凑文章流，与真实卡片共用版式类
#[component]
pub fn PostCardSkeleton(
    #[props(default = false)] has_cover: bool,
    #[props(default = false)] compact: bool,
) -> Element {
    let article_class: &'static str = if compact {
        "post-card-editorial post-card-compact"
    } else {
        "mb-10 flex flex-col bg-[var(--color-paper-entry)] rounded-[2rem] border border-transparent overflow-hidden"
    };

    rsx! {
        article { class: article_class,
            if has_cover {
                div { class: "post-card-cover overflow-hidden",
                    SkeletonBox { class: "w-full aspect-[21/9] !rounded-none" }
                }
            }
            div { class: "post-card-body p-8 flex flex-col gap-3.5 min-w-0",
                div { class: "flex flex-wrap items-center gap-3",
                    SkeletonBox { class: "h-3.5 w-20" }
                    SkeletonBox { class: "h-3.5 w-16" }
                }
                // 标题占位
                SkeletonBox { class: "h-7 w-3/4 rounded" }
                // 摘要两行
                SkeletonBox { class: "h-4 w-full rounded" }
                SkeletonBox { class: "h-4 w-5/6 rounded" }
                div { class: "flex flex-wrap items-center gap-3 mt-1",
                    SkeletonBox { class: "h-4 w-16" }
                    SkeletonBox { class: "h-4 w-20" }
                }
            }
        }
    }
}
