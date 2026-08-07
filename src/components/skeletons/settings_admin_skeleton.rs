//! 后台站点配置骨架屏
//!
//! 镜像后台 SiteSettingsPage 的结构：Header（标题+描述）+ GitHub 链接配置卡片。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::ADMIN_CARD_CLASS;

/// 后台站点配置骨架屏组件。
#[component]
pub fn SettingsAdminSkeleton() -> Element {
    rsx! {
        div { class: "w-full max-w-7xl mx-auto space-y-8",
            // 页头
            div { class: "flex flex-col md:flex-row md:items-end justify-between gap-6 pb-8 border-b border-[var(--color-paper-border)]/50",
                div { class: "space-y-2",
                    SkeletonBox { class: "h-9 w-36 rounded-lg" }
                    SkeletonBox { class: "h-4 w-60 rounded" }
                }
            }

            // 配置卡片占位
            div { class: "{ADMIN_CARD_CLASS} p-8 space-y-6",
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
