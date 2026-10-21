//! 后台回收站骨架屏
//!
//! 与回收站同一版式：标题行（说明、清空回收站）、表格与底栏分页。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 回收站表格骨架屏（列表加载中使用）。
#[component]
pub fn PostsTrashTableSkeleton() -> Element {
    rsx! {
        div { class: "admin-panel admin-table-wrap", aria_hidden: "true",
            table { class: "admin-table admin-trash-table",
                thead {
                    tr {
                        th { class: "admin-table-check", SkeletonBox { class: "h-4 w-4 rounded" } }
                        th { SkeletonBox { class: "h-3 w-10 rounded" } }
                        th { SkeletonBox { class: "h-3 w-12 rounded" } }
                        th { class: "admin-trash-deleted", SkeletonBox { class: "h-3 w-12 rounded" } }
                        th { class: "admin-trash-left", SkeletonBox { class: "h-3 w-14 rounded" } }
                        th { class: "admin-trash-actions" }
                    }
                }
                tbody {
                    for i in 0..6 {
                        tr { key: "{i}",
                            td { class: "admin-table-check", SkeletonBox { class: "h-4 w-4 rounded" } }
                            td { class: "admin-trash-title",
                                SkeletonBox { class: "h-4 w-3/4 rounded" }
                                SkeletonBox { class: "h-3 w-1/3 rounded mt-1.5" }
                            }
                            td { class: "admin-trash-status", SkeletonBox { class: "h-3.5 w-14 rounded" } }
                            td { class: "admin-trash-deleted", SkeletonBox { class: "h-3.5 w-20 rounded" } }
                            td { class: "admin-trash-left", SkeletonBox { class: "h-1 w-full rounded-full" } }
                            td { class: "admin-trash-actions", SkeletonBox { class: "h-[30px] w-20 rounded-md ml-auto" } }
                        }
                    }
                }
            }
        }
    }
}

/// 回收站全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn PostsTrashSkeleton() -> Element {
    rsx! {
        div { class: "admin-trash", aria_hidden: "true",
            div { class: "admin-page-head",
                div {
                    SkeletonBox { class: "h-7 w-20 rounded-md" }
                    SkeletonBox { class: "h-3.5 w-80 max-w-full rounded mt-2" }
                }
                div { class: "admin-page-actions",
                    SkeletonBox { class: "h-[34px] w-24 rounded-lg" }
                }
            }
            PostsTrashTableSkeleton {}
        }
    }
}
