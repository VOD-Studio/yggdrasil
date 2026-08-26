//! 后台回收站骨架屏
//!
//! 镜像后台 PostsTrash 页面的结构：Header（标题+副标题）+ 自动清理配置卡片 + 表格。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{
    SkeletonBox, SkeletonCard, SkeletonCellShape, SkeletonTable, SkeletonTableColumn,
};
/// 后台回收站表格骨架屏组件（供 PostsTrash 页面内部加载态使用）。
#[component]
pub fn PostsTrashTableSkeleton() -> Element {
    let columns = vec![
        SkeletonTableColumn {
            header_class: "px-4 py-3.5 w-10 text-center",
            header_box_class: "h-4 w-4 rounded mx-auto",
            cell_class: "px-4 py-3.5 text-center",
            cell_shape: SkeletonCellShape::Single("h-4 w-4 rounded mx-auto"),
        },
        SkeletonTableColumn {
            header_class: "px-5 py-3.5",
            header_box_class: "h-3 w-16",
            cell_class: "px-5 py-3.5",
            cell_shape: SkeletonCellShape::Stacked("h-4 w-1/3", "h-3 w-1/4"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5 w-24",
            header_box_class: "h-3 w-14 mx-auto",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-5 w-14 mx-auto rounded-full"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5 w-32",
            header_box_class: "h-3 w-16",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-20"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5 w-24",
            header_box_class: "h-3 w-14 mx-auto",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-5 w-14 mx-auto rounded-full"),
        },
        SkeletonTableColumn {
            header_class: "px-5 py-3.5 w-36",
            header_box_class: "h-3 w-16 ml-auto",
            cell_class: "px-5 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-6 w-24 ml-auto rounded"),
        },
    ];
    rsx! {
        SkeletonCard { class: Some("shadow-xs overflow-hidden"),
            SkeletonTable { columns, rows: 8 }
        }
    }
}

/// 后台回收站全页骨架屏组件（供 AdminLayout 路由级 fallback 使用）。
#[component]
pub fn PostsTrashSkeleton() -> Element {
    rsx! {
        div { class: "w-full max-w-7xl mx-auto space-y-6",
            // 页头：标题与副标题 + 按钮
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70",
                div { class: "space-y-1.5",
                    SkeletonBox { class: "h-9 w-28 rounded-lg" }
                    SkeletonBox { class: "h-4 w-60 rounded" }
                }
                div { class: "flex items-center gap-3",
                    SkeletonBox { class: "h-9 w-28 rounded-full" }
                    SkeletonBox { class: "h-9 w-28 rounded-full" }
                }
            }

            div { class: "space-y-6",
                // 自动清理配置卡片占位
                SkeletonCard { class: Some("p-6 space-y-4"),
                    div { class: "flex justify-between items-center",
                        div { class: "space-y-1",
                            SkeletonBox { class: "h-5 w-32 rounded" }
                            SkeletonBox { class: "h-3.5 w-64 rounded" }
                        }
                        SkeletonBox { class: "h-6 w-12 rounded-full" }
                    }
                }

                // 回收站表格
                PostsTrashTableSkeleton {}
            }
        }
    }
}
