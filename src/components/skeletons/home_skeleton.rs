//! 首页骨架屏
//!
//! 模拟首页文章卡片列表与分页区域。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::post_card_skeleton::PostCardSkeleton;

/// 首页骨架屏组件。
///
/// 显示与 `POSTS_PER_PAGE` 等量的文章卡片骨架与分页按钮占位，
/// 使骨架屏与加载完成后的实际列表长度一致，避免内容跳变。
#[component]
pub fn HomeSkeleton() -> Element {
    rsx! {
        div {
            // 列表头部小标占位
            div { class: "flex items-center justify-between mb-8 pb-3.5 border-b border-[var(--color-paper-border)]/30",
                div { class: "flex items-center gap-2.5",
                    SkeletonBox { class: "w-2 h-2 rounded-full" }
                    SkeletonBox { class: "h-5 w-20 rounded" }
                }
                SkeletonBox { class: "h-4 w-14 rounded" }
            }

            // 第 1 篇作为 Featured 骨架（含封面与大号字形占位）
            PostCardSkeleton { featured: true, has_cover: true }

            // 后续 9 篇常规卡片骨架
            for _ in 0..9 {
                PostCardSkeleton {}
            }

            // 分页按钮占位
            div { class: "flex mt-10 mb-6 justify-between",
                SkeletonBox { class: "h-9 w-24 rounded-full" }
                SkeletonBox { class: "h-9 w-24 rounded-full" }
            }
        }
    }
}
