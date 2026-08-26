//! 标签相关骨架屏
//!
//! 提供标签列表页与标签详情页的加载占位组件。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::post_card_skeleton::PostCardSkeleton;

/// 标签列表页骨架屏组件。
///
/// 结构：统计行（"共 N 个标签，M 篇文章"）+ 标签云（flex wrap 的 pill 列表）。
#[component]
pub fn TagsSkeleton() -> Element {
    rsx! {
        div {
            // 统计行占位
            div { class: "mt-2 mb-6",
                SkeletonBox { class: "h-5 w-48 rounded" }
            }

            // 标签云占位 (flex wrap gap-4)
            div { class: "flex flex-wrap gap-4 mt-6",
                // 生成 24 个不同宽度的标签 pill
                for i in 0..24 {
                    SkeletonBox {
                        key: "{i}",
                        class: "h-8 rounded-lg",
                        style: match i % 6 {
                            0 => "width: 60px;",
                            1 => "width: 80px;",
                            2 => "width: 50px;",
                            3 => "width: 100px;",
                            4 => "width: 70px;",
                            _ => "width: 90px;",
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
