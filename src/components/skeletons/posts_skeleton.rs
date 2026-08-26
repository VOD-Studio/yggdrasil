//! 后台文章管理列表骨架屏
//!
//! 镜像后台 Posts 页面的结构：Header（标题+按钮）+ 搜索栏 + 表格 + 分页栏。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{
    SkeletonBox, SkeletonCard, SkeletonCellShape, SkeletonTable, SkeletonTableColumn,
};
/// 后台文章管理表格骨架屏组件（供 AllPostsList 内部加载态使用）。
#[component]
pub fn PostsTableSkeleton() -> Element {
    let columns = vec![
        SkeletonTableColumn {
            header_class: "px-5 py-3.5",
            header_box_class: "h-3 w-16",
            cell_class: "px-5 py-3.5",
            cell_shape: SkeletonCellShape::Stacked("h-4 w-2/3", "h-3 w-1/3"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5 w-24",
            header_box_class: "h-3 w-10 mx-auto",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-5 w-14 mx-auto rounded-full"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5 w-28 hidden md:table-cell",
            header_box_class: "h-3 w-10",
            cell_class: "px-4 py-3.5 hidden md:table-cell",
            cell_shape: SkeletonCellShape::Single("h-4 w-16"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5 w-32",
            header_box_class: "h-3 w-16",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-24"),
        },
        SkeletonTableColumn {
            header_class: "px-5 py-3.5 w-48",
            header_box_class: "h-3 w-12 ml-auto",
            cell_class: "px-5 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-6 w-32 ml-auto rounded"),
        },
    ];
    rsx! {
        SkeletonCard { class: Some("shadow-xs overflow-hidden"),
            SkeletonTable { columns, rows: 8 }
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
