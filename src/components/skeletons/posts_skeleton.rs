//! 后台文章列表骨架屏
//!
//! 与文章列表同一版式：标题行、状态选项卡与筛选、表格与底栏分页。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 文章表格骨架屏（列表加载中使用）。
#[component]
pub fn PostsTableSkeleton() -> Element {
    rsx! {
        div { class: "admin-panel admin-table-wrap", aria_hidden: "true",
            table { class: "admin-table admin-posts-table",
                thead {
                    tr {
                        th { class: "admin-table-check", SkeletonBox { class: "h-4 w-4 rounded" } }
                        th { SkeletonBox { class: "h-3 w-10 rounded" } }
                        th { SkeletonBox { class: "h-3 w-10 rounded" } }
                        th { class: "admin-posts-tags", SkeletonBox { class: "h-3 w-10 rounded" } }
                        th { class: "admin-posts-words", SkeletonBox { class: "h-3 w-8 rounded ml-auto" } }
                        th { SkeletonBox { class: "h-3 w-14 rounded" } }
                        th { class: "admin-table-actions" }
                    }
                }
                tbody {
                    for i in 0..8 {
                        tr { key: "{i}",
                            td { class: "admin-table-check", SkeletonBox { class: "h-4 w-4 rounded" } }
                            td { class: "admin-posts-title",
                                SkeletonBox { class: "h-4 w-3/4 rounded" }
                                SkeletonBox { class: "h-3 w-1/3 rounded mt-1.5" }
                            }
                            td { class: "admin-posts-status", SkeletonBox { class: "h-3.5 w-14 rounded" } }
                            td { class: "admin-posts-tags", SkeletonBox { class: "h-3.5 w-20 rounded" } }
                            td { class: "admin-posts-words", SkeletonBox { class: "h-3.5 w-10 rounded ml-auto" } }
                            td { class: "admin-posts-date", SkeletonBox { class: "h-3.5 w-28 rounded" } }
                            td { class: "admin-table-actions", SkeletonBox { class: "h-6 w-6 rounded-md ml-auto" } }
                        }
                    }
                }
            }
        }
    }
}

/// 文章列表全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn PostsSkeleton() -> Element {
    rsx! {
        div { class: "admin-posts", aria_hidden: "true",
            div { class: "admin-page-head",
                div {
                    SkeletonBox { class: "h-7 w-20 rounded-md" }
                    SkeletonBox { class: "h-3.5 w-44 rounded mt-2" }
                }
                div { class: "admin-page-actions",
                    SkeletonBox { class: "h-[34px] w-[34px] rounded-lg" }
                    SkeletonBox { class: "h-[34px] w-28 rounded-lg" }
                }
            }
            div { class: "admin-toolbar",
                SkeletonBox { class: "h-8 w-72 rounded-full" }
                div { class: "admin-toolbar-filters",
                    SkeletonBox { class: "h-[30px] w-60 rounded-lg" }
                    SkeletonBox { class: "h-[30px] w-28 rounded-lg" }
                }
            }
            PostsTableSkeleton {}
        }
    }
}
