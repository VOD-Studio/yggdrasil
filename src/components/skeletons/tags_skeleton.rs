//! 标签相关骨架屏
//!
//! 提供归档页标签索引与标签详情页的加载占位组件。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{SkeletonBox, SkeletonCard};
use crate::components::skeletons::post_card_skeleton::PostCardSkeleton;

/// 归档页标签索引骨架屏组件。
///
/// 结构：图标与摘要 + 标签胶囊，匹配默认展开的折叠卡片。
#[component]
pub fn TagsSkeleton() -> Element {
    rsx! {
        SkeletonCard { class: "archive-tags-skeleton mb-9 p-5 sm:p-6",
            div { class: "flex items-center gap-3 mb-5",
                SkeletonBox { class: "h-11 w-11 shrink-0 rounded-2xl" }
                div { class: "flex-1 min-w-0",
                    SkeletonBox { class: "h-5 w-24 rounded mb-2" }
                    SkeletonBox { class: "h-3 w-48 max-w-full rounded" }
                }
                SkeletonBox { class: "h-4 w-4 rounded" }
            }

            div { class: "flex flex-wrap gap-2.5 border-t border-paper-border/50 pt-5",
                for i in 0..8 {
                    SkeletonBox {
                        key: "{i}",
                        class: "h-11 rounded-full",
                        style: match i % 6 {
                            0 => "width: 100px;",
                            1 => "width: 120px;",
                            2 => "width: 90px;",
                            3 => "width: 140px;",
                            4 => "width: 110px;",
                            _ => "width: 130px;",
                        },
                    }
                }
            }
        }
    }
}

/// 标签详情页骨架屏组件。
///
/// 结构与首页文章列表相同，包含统计行与文章卡片骨架。
#[component]
pub fn TagDetailSkeleton() -> Element {
    rsx! {
        div {
            // 统计行占位
            div { class: "mt-2 mb-6",
                SkeletonBox { class: "h-5 w-32 rounded" }
            }

            // 文章卡片列表
            for i in 0..5 {
                PostCardSkeleton { key: "{i}" }
            }
        }
    }
}
