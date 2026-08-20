//! 文章卡片骨架屏
//!
//! 模拟 PostCard 组件的视觉占位，用于列表页加载。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 文章卡片骨架屏组件。
/// Props:
/// - `featured`：是否作为头条置顶特色文章骨架
/// - `has_cover`：是否展示封面占位
#[component]
pub fn PostCardSkeleton(
    #[props(default = false)] featured: bool,
    #[props(default = false)] has_cover: bool,
) -> Element {
    let article_class: &'static str = if featured {
        "mb-12 flex flex-col bg-[var(--color-paper-entry)] rounded-[2rem] border border-[var(--color-paper-border)]/60 overflow-hidden"
    } else {
        "mb-10 flex flex-col bg-[var(--color-paper-entry)] rounded-[2rem] border border-transparent overflow-hidden"
    };
    let title_box_class: &'static str = if featured {
        "h-8 sm:h-9 w-3/4 rounded"
    } else {
        "h-7 w-3/4 rounded"
    };

    rsx! {
        article { class: article_class,
            if has_cover {
                div { class: "w-full overflow-hidden",
                    SkeletonBox { class: "w-full aspect-[21/9] !rounded-none" }
                }
            }
            div { class: "p-8 flex flex-col gap-3.5",
                if featured {
                    SkeletonBox { class: "h-5 w-20 rounded-full" }
                }
                // 标题占位
                SkeletonBox { class: title_box_class }
                // 摘要两到三行
                SkeletonBox { class: "h-4 w-full rounded" }
                SkeletonBox { class: "h-4 w-5/6 rounded" }
                if featured {
                    SkeletonBox { class: "h-4 w-2/3 rounded" }
                }
                // 元信息行 (日期 + 阅读时间 + 标签)
                div { class: "flex flex-wrap items-center justify-between gap-3 mt-3 pt-3 border-t border-[var(--color-paper-border)]/30",
                    div { class: "flex flex-wrap items-center gap-3",
                        SkeletonBox { class: "h-3.5 w-20 rounded" }
                        SkeletonBox { class: "h-3.5 w-1 rounded" }
                        SkeletonBox { class: "h-3.5 w-20 rounded" }
                        SkeletonBox { class: "h-3.5 w-1 rounded" }
                        SkeletonBox { class: "h-3.5 w-16 rounded" }
                    }
                    SkeletonBox { class: "hidden sm:block h-3.5 w-16 rounded" }
                }
            }
        }
    }
}
