//! 后台素材管理骨架屏
//!
//! 镜像后台 Assets 页面的结构：Header（标题+描述）+ 筛选/搜索工具栏 + 12 个网格缩略图卡片。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 后台素材管理骨架屏组件。
#[component]
pub fn AssetsSkeleton() -> Element {
    rsx! {
        div { class: "w-full max-w-7xl mx-auto space-y-6",
            // 页头：标题 + 描述
            div { class: "space-y-2 mb-8",
                SkeletonBox { class: "h-9 w-36 rounded-lg" }
                SkeletonBox { class: "h-4 w-72 rounded" }
            }

            // 顶栏：筛选 tabs + 搜索框 + 排序按钮
            div { class: "flex flex-wrap items-end justify-between gap-4 mb-6",
                // Tabs 占位
                div { class: "flex gap-2 border-b border-paper-border pb-2",
                    SkeletonBox { class: "h-8 w-16 rounded-full" }
                    SkeletonBox { class: "h-8 w-20 rounded-full" }
                    SkeletonBox { class: "h-8 w-20 rounded-full" }
                }
                // 右侧 搜索 + 排序 占位
                div { class: "flex items-center gap-3 mb-6",
                    SkeletonBox { class: "h-9 w-56 rounded-2xl" }
                    SkeletonBox { class: "h-8 w-16 rounded-full" }
                    SkeletonBox { class: "h-8 w-16 rounded-full" }
                }
            }

            // 响应式缩略图网格
            div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6 gap-4",
                for i in 0..12 {
                    div { key: "{i}", class: "aspect-square rounded-2xl border border-paper-border bg-paper-entry overflow-hidden p-2 flex flex-col justify-between",
                        SkeletonBox { class: "w-full flex-1 rounded-xl" }
                        div { class: "mt-2 flex justify-between items-center",
                            SkeletonBox { class: "h-3 w-20 rounded" }
                            SkeletonBox { class: "h-3 w-10 rounded" }
                        }
                    }
                }
            }
        }
    }
}
