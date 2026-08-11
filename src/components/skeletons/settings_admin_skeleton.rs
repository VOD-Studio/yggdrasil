//! 后台站点配置骨架屏
//!
//! 镜像 SiteSettingsPage 的分区化布局（页头 + 左侧分类导航 + 右侧内容卡片）。
//! 根容器沿用页面的 flex-1 min-h-0 flex 撑满高度并自带 px-6 py-12 内边距，
//! 避免骨架屏只占顶部、下方留出大片空白；左右两列结构对齐，减少加载时的布局跳动。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::ADMIN_CARD_CLASS;

/// 后台站点配置骨架屏组件。
#[component]
pub fn SettingsAdminSkeleton() -> Element {
    rsx! {
        // 镜像 SiteSettingsPage 根容器：flex-1 min-h-0 flex 撑满 main 高度。
        // main 在 internal-scroll 变体下无 padding，故内边距由本骨架自带。
        div { class: "w-full flex-1 min-h-0 flex flex-col px-6 py-12",
            // 页头（固定，不随右侧内容滚动）
            div { class: "flex-shrink-0 flex flex-col md:flex-row md:items-end justify-between gap-6 pb-6 border-b border-[var(--color-paper-border)] mb-6",
                div { class: "space-y-2",
                    SkeletonBox { class: "h-9 w-36 rounded-lg" }
                    SkeletonBox { class: "h-4 w-60 rounded" }
                }
            }

            // 左侧导航 + 右侧内容（占满剩余高度）
            div { class: "flex flex-col lg:flex-row gap-6 flex-1 min-h-0",
                // 左侧导航占位（镜像 10 个分区按钮）
                div { class: "lg:w-48 flex-shrink-0 flex lg:flex-col gap-1",
                    for _ in 0..10 {
                        SkeletonBox { class: "h-9 w-full rounded-xl" }
                    }
                }

                // 右侧内容占位（镜像单个配置卡片）
                div { class: "flex-1 min-w-0 {ADMIN_CARD_CLASS} p-8 space-y-6",
                    div { class: "flex items-center gap-3",
                        SkeletonBox { class: "w-10 h-10 rounded-full" }
                        div { class: "space-y-1.5",
                            SkeletonBox { class: "h-5 w-36 rounded" }
                            SkeletonBox { class: "h-3 w-56 rounded" }
                        }
                    }
                    div { class: "space-y-2 max-w-xl",
                        SkeletonBox { class: "h-4 w-20 rounded" }
                        SkeletonBox { class: "h-10 w-full rounded-2xl" }
                        SkeletonBox { class: "h-3 w-44 rounded" }
                    }
                    SkeletonBox { class: "h-4 w-64 rounded" }
                    div { class: "flex items-center justify-between pt-1",
                        SkeletonBox { class: "h-3 w-24 rounded" }
                        SkeletonBox { class: "h-10 w-24 rounded-full" }
                    }
                }
            }
        }
    }
}
