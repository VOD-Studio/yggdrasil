//! 后台评论管理骨架屏
//!
//! 镜像后台 AdminComments 页面的结构：Header（标题+描述）+ 状态筛选 Tabs + 5 条评论卡片行。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
/// 评论表格骨架屏组件（纯表格部分，供 AdminCommentsPage 内部加载态使用）。
#[component]
pub fn AdminCommentsTableSkeleton() -> Element {
    rsx! {
        div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 overflow-hidden",
            table { class: "w-full text-sm",
                thead {
                    tr { class: "bg-[var(--color-paper-entry)]/80 border-b border-[var(--color-paper-border)]/70",
                        th { class: "px-4 py-3.5 w-10 text-center",
                            SkeletonBox { class: "h-4 w-4 rounded mx-auto" }
                        }
                        th { class: "px-5 py-3.5 w-48",
                            SkeletonBox { class: "h-3 w-16" }
                        }
                        th { class: "px-5 py-3.5",
                            SkeletonBox { class: "h-3 w-20" }
                        }
                        th { class: "px-5 py-3.5 w-56",
                            SkeletonBox { class: "h-3 w-16" }
                        }
                        th { class: "px-4 py-3.5 w-24",
                            SkeletonBox { class: "h-3 w-10 mx-auto" }
                        }
                        th { class: "px-4 py-3.5 w-28",
                            SkeletonBox { class: "h-3 w-14" }
                        }
                        th { class: "px-5 py-3.5 w-36",
                            SkeletonBox { class: "h-3 w-12 ml-auto" }
                        }
                    }
                }
                tbody {
                    for _ in 0..8 {
                        tr { class: "border-b border-[var(--color-paper-border)]/60 last:border-0",
                            td { class: "px-4 py-3.5 text-center",
                                SkeletonBox { class: "h-4 w-4 rounded mx-auto" }
                            }
                            td { class: "px-5 py-3.5",
                                div { class: "flex items-center gap-2.5",
                                    SkeletonBox { class: "h-8 w-8 rounded-full shrink-0" }
                                    div { class: "space-y-1 min-w-0 flex-1",
                                        SkeletonBox { class: "h-3.5 w-20 rounded" }
                                        SkeletonBox { class: "h-2.5 w-28 rounded" }
                                    }
                                }
                            }
                            td { class: "px-5 py-3.5",
                                SkeletonBox { class: "h-4 w-3/4 rounded" }
                            }
                            td { class: "px-5 py-3.5",
                                SkeletonBox { class: "h-3.5 w-32 rounded" }
                            }
                            td { class: "px-4 py-3.5",
                                SkeletonBox { class: "h-5 w-14 mx-auto rounded-full" }
                            }
                            td { class: "px-4 py-3.5",
                                SkeletonBox { class: "h-4 w-20" }
                            }
                            td { class: "px-5 py-3.5",
                                SkeletonBox { class: "h-6 w-24 ml-auto rounded" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 后台评论管理全页骨架屏组件（供 AdminLayout 路由级 fallback 使用）。
#[component]
pub fn AdminCommentsSkeleton() -> Element {
    rsx! {
        div { class: "w-full max-w-7xl mx-auto space-y-6",
            // 页头：标题与描述
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70",
                div { class: "space-y-1.5",
                    SkeletonBox { class: "h-9 w-36 rounded-lg" }
                    SkeletonBox { class: "h-4 w-64 rounded" }
                }
            }

            // 状态筛选 Tabs
            div { class: "flex gap-3 border-b border-[var(--color-paper-border)]/70 pb-3 mb-6",
                SkeletonBox { class: "h-8 w-16 rounded-full" }
                SkeletonBox { class: "h-8 w-20 rounded-full" }
                SkeletonBox { class: "h-8 w-20 rounded-full" }
                SkeletonBox { class: "h-8 w-20 rounded-full" }
            }

            // 评论列表表格
            AdminCommentsTableSkeleton {}

            // 分页栏
            div { class: "flex justify-between items-center pt-4",
                SkeletonBox { class: "h-4 w-32 rounded" }
                div { class: "flex gap-2",
                    SkeletonBox { class: "h-8 w-16 rounded-full" }
                    SkeletonBox { class: "h-8 w-16 rounded-full" }
                }
            }
        }
    }
}
