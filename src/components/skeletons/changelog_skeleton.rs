//! 更新日志页骨架屏。
//!
//! 镜像更新日志的页头、统计栏和版本双栏布局，避免加载时出现文章封面占位。
//! 动画由外层 DelayedSkeleton 统一提供。

use crate::components::skeletons::atoms::{SkeletonBox, SkeletonCard};
use dioxus::prelude::*;

/// 更新日志加载占位：桌面端版本导航 + 版本卡片，小屏隐藏导航。
#[component]
pub fn ChangelogSkeleton() -> Element {
    rsx! {
        div { aria_hidden: "true",
            div { class: "page-header mb-6",
                SkeletonBox { class: "h-10 w-36 rounded", animate: false }
            }

            div { class: "flex flex-wrap items-center gap-x-4 gap-y-1 mb-8",
                SkeletonBox { class: "h-5 w-36 rounded", animate: false }
                SkeletonBox { class: "h-5 w-24 rounded", animate: false }
            }

            div { class: "flex gap-8",
                div { class: "hidden lg:block w-40 shrink-0",
                    div { class: "sticky top-20",
                        for i in 0..6 {
                            div { key: "{i}", class: "flex items-center gap-2 py-1.5",
                                SkeletonBox { class: "w-1.5 h-1.5 rounded-full shrink-0", animate: false }
                                SkeletonBox { class: "h-5 w-16 rounded", animate: false }
                            }
                        }
                    }
                }

                div { class: "flex-1 min-w-0 space-y-6",
                    for i in 0..2 {
                        SkeletonCard { key: "{i}", class: Some("rounded-[2rem] p-6 md:p-8"),
                            div { class: "flex items-baseline gap-3",
                                SkeletonBox { class: "h-7 md:h-8 w-24 rounded", animate: false }
                                SkeletonBox { class: "h-4 w-20 rounded ml-auto", animate: false }
                            }

                            div { class: "mt-4 space-y-5",
                                for group in 0..2 {
                                    div { key: "{group}",
                                        SkeletonBox { class: "h-5 w-12 rounded-full mb-2", animate: false }
                                        div { class: "space-y-2 pl-4",
                                            SkeletonBox { class: "h-4 w-full rounded", animate: false }
                                            SkeletonBox { class: "h-4 w-5/6 rounded", animate: false }
                                            SkeletonBox { class: "h-4 w-3/4 rounded", animate: false }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
