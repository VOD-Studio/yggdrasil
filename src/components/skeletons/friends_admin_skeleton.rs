//! 后台友链管理骨架屏
//!
//! 镜像后台 FriendsAdmin 页面的结构：Header（标题+描述）+ 表单卡片 + 列表卡片。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{SkeletonBox, SkeletonCard};

/// 后台友链列表卡片骨架屏组件（供 LinkList 组件内部加载态使用）。
#[component]
pub fn FriendsAdminListSkeleton() -> Element {
    rsx! {
        SkeletonCard { class: Some("p-6 sm:p-8 space-y-6 shadow-xs"),
            SkeletonBox { class: "h-6 w-32 rounded" }
            div { class: "space-y-4 divide-y divide-[var(--color-paper-border)]/50",
                for i in 0..4 {
                    div { key: "{i}", class: "pt-4 flex items-center justify-between gap-4",
                        div { class: "flex items-center gap-3.5 flex-1 min-w-0",
                            SkeletonBox { class: "h-10 w-10 rounded-2xl shrink-0" }
                            div { class: "space-y-1.5 flex-1 min-w-0",
                                SkeletonBox { class: "h-4 w-32 rounded" }
                                SkeletonBox { class: "h-3 w-48 rounded" }
                            }
                        }
                        SkeletonBox { class: "h-8 w-24 rounded-lg" }
                    }
                }
            }
        }
    }
}

/// 后台友链管理全页骨架屏组件（供 AdminLayout 路由级 fallback 使用）。
#[component]
pub fn FriendsAdminSkeleton() -> Element {
    rsx! {
        div { class: "w-full max-w-7xl mx-auto space-y-8",
            // 页头
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70",
                div { class: "space-y-1.5",
                    SkeletonBox { class: "h-9 w-36 rounded-lg" }
                    SkeletonBox { class: "h-4 w-60 rounded" }
                }
                SkeletonBox { class: "h-9 w-28 rounded-full" }
            }

            // 表单卡片占位
            SkeletonCard { class: Some("p-6 sm:p-8 space-y-6 shadow-xs"),
                SkeletonBox { class: "h-6 w-28 rounded" }
                div { class: "grid grid-cols-1 md:grid-cols-2 gap-5",
                    div { class: "space-y-2",
                        SkeletonBox { class: "h-3.5 w-16 rounded" }
                        SkeletonBox { class: "h-10 w-full rounded-2xl" }
                    }
                    div { class: "space-y-2",
                        SkeletonBox { class: "h-3.5 w-20 rounded" }
                        SkeletonBox { class: "h-10 w-full rounded-2xl" }
                    }
                    div { class: "space-y-2",
                        SkeletonBox { class: "h-3.5 w-20 rounded" }
                        SkeletonBox { class: "h-10 w-full rounded-2xl" }
                    }
                    div { class: "space-y-2",
                        SkeletonBox { class: "h-3.5 w-24 rounded" }
                        SkeletonBox { class: "h-10 w-full rounded-2xl" }
                    }
                    div { class: "space-y-2 md:col-span-2",
                        SkeletonBox { class: "h-3.5 w-16 rounded" }
                        SkeletonBox { class: "h-16 w-full rounded-2xl" }
                    }
                }
                SkeletonBox { class: "h-10 w-24 rounded-full" }
            }

            // 友链列表卡片占位
            FriendsAdminListSkeleton {}
        }
    }
}
