//! 后台仪表盘骨架屏
//!
//! 仅在 AdminLayout 的内容区展示，不包含 Header 与 Footer，
//! 用于校验登录状态期间保持视觉稳定。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 仪表盘内容区骨架屏组件（不含 header/footer）。
///
/// 包含统计卡片、快捷操作按钮与最近文章列表三组占位块。
#[component]
pub fn AdminDashboardSkeleton() -> Element {
    rsx! {
        div { class: "space-y-8",
            // 统计卡片骨架
            div { class: "grid grid-cols-1 md:grid-cols-3 gap-6",
                for _ in 0..3 {
                    div { class: "rounded-2xl bg-paper-entry border border-paper-border p-6 text-center space-y-3",
                        SkeletonBox { class: "h-9 w-16 mx-auto rounded" }
                        SkeletonBox { class: "h-4 w-20 mx-auto rounded" }
                    }
                }
            }

            // 快捷操作骨架
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                SkeletonBox { class: "h-12 rounded-full" }
                SkeletonBox { class: "h-12 rounded-full" }
            }

            // 最近文章列表骨架
            div { class: "space-y-4",
                SkeletonBox { class: "h-6 w-24 rounded" }
                div { class: "space-y-0",
                    for _ in 0..5 {
                        div { class: "flex justify-between items-center py-3 border-b border-paper-border",
                            SkeletonBox { class: "h-4 w-[45%] rounded" }
                            SkeletonBox { class: "h-3 w-20 rounded" }
                        }
                    }
                }
            }
        }
    }
}
