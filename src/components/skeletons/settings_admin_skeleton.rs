//! 后台站点配置骨架屏
//!
//! 镜像 SiteSettingsPage 的分区化布局（页头 + 左侧分类导航 + 右侧内容卡片）。
//! 根容器沿用页面的 flex-1 min-h-0 flex 撑满高度并自带 px-6 py-12 内边距，
//! 避免骨架屏只占顶部、下方留出大片空白；左右两列结构对齐，减少加载时的布局跳动。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{SkeletonBox, SkeletonCard};
/// 后台站点配置骨架屏组件。
#[component]
pub fn SettingsAdminSkeleton() -> Element {
    rsx! {
        // 镜像 SiteSettingsPage 根容器：flex-1 min-h-0 flex 撑满 main 高度。
        // main 在 internal-scroll 变体下无 padding，故内边距由本骨架自带。
        div { class: "w-full flex-1 min-h-0 flex flex-col px-6 py-8 sm:py-12",
            // 页头（固定）
            div { class: "flex-shrink-0 flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70 mb-6",
                div { class: "space-y-1.5",
                    SkeletonBox { class: "h-9 w-36 rounded-lg" }
                    SkeletonBox { class: "h-4 w-60 rounded" }
                }
            }

            // 顶部横向分类导航栏占位
            div { class: "flex-shrink-0 flex items-center gap-2 overflow-x-auto pb-3 mb-6 border-b border-[var(--color-paper-border)]/60 scrollbar-none",
                for i in 0..8 {
                    SkeletonBox { key: "{i}", class: "h-8 w-20 rounded-full shrink-0" }
                }
            }

            // 右侧/居中内容占位（镜像配置卡片堆叠）
            div { class: "flex-1 min-w-0 min-h-0 overflow-hidden max-w-5xl mx-auto w-full space-y-8",
                SkeletonCard { class: Some("p-6 sm:p-8 space-y-6 shadow-xs"),
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
