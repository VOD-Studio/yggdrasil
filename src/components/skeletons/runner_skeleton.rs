//! 后台代码试运行沙箱骨架屏
//!
//! 镜像后台 Runner 页面的结构：Header（标题+描述）+ 语言切换 Pills + 沙箱配置/代码编辑器 + 输出面板。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{SkeletonBox, SkeletonCard};

/// 后台代码试运行沙箱骨架屏组件。
#[component]
pub fn RunnerSkeleton() -> Element {
    rsx! {
        div { class: "w-full max-w-7xl mx-auto space-y-8",
            // 页头
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70",
                div { class: "space-y-1.5",
                    SkeletonBox { class: "h-9 w-48 rounded-lg" }
                    SkeletonBox { class: "h-4 w-96 rounded" }
                }
                SkeletonBox { class: "h-8 w-36 rounded-full" }
            }

            // 配置卡片：语言切换 Tabs + 资源覆盖
            SkeletonCard { class: Some("p-6 sm:p-8 space-y-6 shadow-xs"),
                SkeletonBox { class: "h-6 w-36 rounded" }
                div { class: "space-y-2.5",
                    SkeletonBox { class: "h-3.5 w-16 rounded" }
                    div { class: "flex flex-wrap gap-2.5",
                        SkeletonBox { class: "h-8 w-20 rounded-full" }
                        SkeletonBox { class: "h-8 w-20 rounded-full" }
                        SkeletonBox { class: "h-8 w-16 rounded-full" }
                        SkeletonBox { class: "h-8 w-16 rounded-full" }
                        SkeletonBox { class: "h-8 w-20 rounded-full" }
                    }
                }
                div { class: "space-y-2",
                    SkeletonBox { class: "h-3.5 w-28 rounded" }
                    SkeletonBox { class: "h-10 w-full rounded-2xl" }
                }
            }

            // 沙箱代码编辑器卡片占位
            SkeletonCard { class: Some("p-6 space-y-4 shadow-xs"),
                div { class: "flex justify-between items-center pb-3 border-b border-[var(--color-paper-border)]/60",
                    SkeletonBox { class: "h-5 w-24 rounded" }
                    SkeletonBox { class: "h-8 w-20 rounded-full" }
                }
                SkeletonBox { class: "h-64 w-full rounded-2xl" }
            }
        }
    }
}
