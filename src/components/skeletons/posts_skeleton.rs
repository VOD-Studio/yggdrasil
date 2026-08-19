//! 后台文章管理列表骨架屏
//!
//! 镜像后台 Posts 页面的结构：Header（标题+按钮）+ 搜索栏 + 表格 + 分页栏。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
/// 后台文章管理表格骨架屏组件（供 AllPostsList 内部加载态使用）。
#[component]
pub fn PostsTableSkeleton() -> Element {
    rsx! {
        div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 overflow-hidden",
            table { class: "w-full text-sm",
                thead {
                    tr { class: "bg-[var(--color-paper-entry)]/80 border-b border-[var(--color-paper-border)]/70",
                        th { class: "px-5 py-3.5",
                            SkeletonBox { class: "h-3 w-16" }
                        }
                        th { class: "px-4 py-3.5 w-24",
                            SkeletonBox { class: "h-3 w-10 mx-auto" }
                        }
                        th { class: "px-4 py-3.5 w-28 hidden md:table-cell",
                            SkeletonBox { class: "h-3 w-10" }
                        }
                        th { class: "px-4 py-3.5 w-32",
                            SkeletonBox { class: "h-3 w-16" }
                        }
                        th { class: "px-5 py-3.5 w-48",
                            SkeletonBox { class: "h-3 w-12 ml-auto" }
                        }
                    }
                }
                tbody {
                    for _ in 0..8 {
                        tr { class: "border-b border-[var(--color-paper-border)]/60 last:border-0",
                            td { class: "px-5 py-3.5",
                                div { class: "space-y-1.5",
                                    SkeletonBox { class: "h-4 w-2/3" }
                                    SkeletonBox { class: "h-3 w-1/3" }
                                }
                            }
                            td { class: "px-4 py-3.5",
                                SkeletonBox { class: "h-5 w-14 mx-auto rounded-full" }
                            }
                            td { class: "px-4 py-3.5 hidden md:table-cell",
                                SkeletonBox { class: "h-4 w-16" }
                            }
                            td { class: "px-4 py-3.5",
                                SkeletonBox { class: "h-4 w-24" }
                            }
                            td { class: "px-5 py-3.5",
                                SkeletonBox { class: "h-6 w-32 ml-auto rounded" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 后台文章管理全页骨架屏组件（供 AdminLayout 路由级 fallback 使用）。
#[component]
pub fn PostsSkeleton() -> Element {
    rsx! {
        div { class: "w-full max-w-7xl mx-auto space-y-6",
            // 页头：标题 + 操作按钮
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70",
                div { class: "space-y-1.5",
                    SkeletonBox { class: "h-9 w-36 rounded-lg" }
                    SkeletonBox { class: "h-4 w-56 rounded" }
                }
                div { class: "flex items-center gap-3",
                    SkeletonBox { class: "h-9 w-24 rounded-full" }
                    SkeletonBox { class: "h-9 w-24 rounded-full" }
                    SkeletonBox { class: "h-9 w-28 rounded-full" }
                }
            }

            // 搜索/筛选工具栏
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 mb-4",
                div { class: "flex gap-3",
                    SkeletonBox { class: "h-8 w-16 rounded-full" }
                    SkeletonBox { class: "h-8 w-20 rounded-full" }
                    SkeletonBox { class: "h-8 w-16 rounded-full" }
                }
                div { class: "flex items-center gap-2",
                    SkeletonBox { class: "h-9 w-72 rounded-2xl" }
                    SkeletonBox { class: "h-9 w-16 rounded-full" }
                }
            }

            // 文章列表表格
            PostsTableSkeleton {}

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
