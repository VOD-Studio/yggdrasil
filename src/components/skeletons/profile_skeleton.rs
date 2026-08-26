//! 后台个人信息骨架屏
//!
//! 镜像后台 Profile 页面（/admin/profile）的结构：
//! 页头（标题+描述）+ 身份卡（圆形头像 + 文字行）+ 基本资料卡 + 安全卡。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{SkeletonBox, SkeletonCard};

/// 后台个人信息骨架屏组件。
#[component]
pub fn ProfileSkeleton() -> Element {
    rsx! {
        div { class: "w-full max-w-2xl mx-auto space-y-8",
            // 页头
            div { class: "space-y-2 pb-6 border-b border-[var(--color-paper-border)]/50",
                SkeletonBox { class: "h-9 w-32 rounded-lg" }
                SkeletonBox { class: "h-4 w-72 rounded" }
            }

            // 身份卡：圆形头像 + 文字行
            SkeletonCard { class: Some("p-6 md:p-8 flex items-center gap-6"),
                SkeletonBox { class: "h-24 w-24 rounded-full flex-shrink-0" }
                div { class: "flex-1 space-y-2.5",
                    SkeletonBox { class: "h-6 w-40 rounded" }
                    SkeletonBox { class: "h-4 w-24 rounded" }
                    SkeletonBox { class: "h-3 w-32 rounded" }
                }
            }

            // 基本资料卡：卡头 + 三条输入行 + 按钮行
            SkeletonCard { class: Some("p-6 md:p-8 space-y-6"),
                div { class: "flex items-center gap-3",
                    SkeletonBox { class: "h-10 w-10 rounded-full flex-shrink-0" }
                    div { class: "space-y-2",
                        SkeletonBox { class: "h-5 w-24 rounded" }
                        SkeletonBox { class: "h-3.5 w-56 rounded" }
                    }
                }
                div { class: "space-y-2 max-w-xl",
                    SkeletonBox { class: "h-3.5 w-16 rounded" }
                    SkeletonBox { class: "h-9 w-full rounded-2xl" }
                }
                div { class: "space-y-2 max-w-xl",
                    SkeletonBox { class: "h-3.5 w-16 rounded" }
                    SkeletonBox { class: "h-9 w-full rounded-2xl" }
                }
                div { class: "flex justify-end",
                    SkeletonBox { class: "h-9 w-24 rounded-full" }
                }
            }

            // 安全卡：卡头 + 输入行 + 双列输入 + 按钮行
            SkeletonCard { class: Some("p-6 md:p-8 space-y-6"),
                div { class: "flex items-center gap-3",
                    SkeletonBox { class: "h-10 w-10 rounded-full flex-shrink-0" }
                    div { class: "space-y-2",
                        SkeletonBox { class: "h-5 w-16 rounded" }
                        SkeletonBox { class: "h-3.5 w-48 rounded" }
                    }
                }
                div { class: "space-y-2 max-w-xl",
                    SkeletonBox { class: "h-3.5 w-16 rounded" }
                    SkeletonBox { class: "h-9 w-full rounded-2xl" }
                }
                div { class: "grid sm:grid-cols-2 gap-4 max-w-xl",
                    SkeletonBox { class: "h-9 w-full rounded-2xl" }
                    SkeletonBox { class: "h-9 w-full rounded-2xl" }
                }
                div { class: "flex justify-end",
                    SkeletonBox { class: "h-9 w-24 rounded-full" }
                }
            }
        }
    }
}
