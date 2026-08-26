//! 后台系统管理骨架屏
//!
//! 镜像后台 System 页面的结构：Header（标题+副标题）+ 5 个功能 Tabs + 4 个统计卡片 + 数据行列表。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{
    SkeletonBox, SkeletonCard, SkeletonCellShape, SkeletonTable, SkeletonTableColumn,
};

/// 后台系统管理骨架屏组件。
#[component]
pub fn SystemSkeleton() -> Element {
    let table_columns = vec![
        SkeletonTableColumn {
            header_class: "px-5 py-3.5",
            header_box_class: "h-3 w-16",
            cell_class: "px-5 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-28"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5",
            header_box_class: "h-3 w-12",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-16"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5",
            header_box_class: "h-3 w-16",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-20"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5",
            header_box_class: "h-3 w-16",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-20"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5",
            header_box_class: "h-3 w-16",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-24"),
        },
        SkeletonTableColumn {
            header_class: "px-5 py-3.5",
            header_box_class: "h-3 w-12 ml-auto",
            cell_class: "px-5 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-12 ml-auto"),
        },
    ];
    rsx! {
        div { class: "w-full max-w-7xl mx-auto space-y-6",
            // 页头：标题与副标题 + 运行状态指示
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70 mb-6",
                div { class: "space-y-1.5",
                    SkeletonBox { class: "h-9 w-36 rounded-lg" }
                    SkeletonBox { class: "h-4 w-64 rounded" }
                }
                SkeletonBox { class: "h-8 w-32 rounded-full" }
            }

            // 5 个功能 Tabs
            div { class: "flex gap-3 border-b border-[var(--color-paper-border)]/70 pb-3 mb-6",
                SkeletonBox { class: "h-8 w-24 rounded-full" }
                SkeletonBox { class: "h-8 w-24 rounded-full" }
                SkeletonBox { class: "h-8 w-24 rounded-full" }
                SkeletonBox { class: "h-8 w-20 rounded-full" }
                SkeletonBox { class: "h-8 w-20 rounded-full" }
            }

            // 4 个统计卡片网格
            div { class: "grid grid-cols-2 lg:grid-cols-4 gap-4",
                for i in 0..4 {
                    SkeletonCard { key: "{i}", class: Some("p-5 shadow-xs space-y-2"),
                        SkeletonBox { class: "h-3.5 w-16 rounded" }
                        SkeletonBox { class: "h-8 w-24 rounded-lg" }
                    }
                }
            }

            // 数据表格卡片占位
            SkeletonCard { class: Some("shadow-xs overflow-hidden"),
                div { class: "px-5 py-4 border-b border-[var(--color-paper-border)]/70",
                    SkeletonBox { class: "h-4 w-40 rounded" }
                }
                SkeletonTable { columns: table_columns, rows: 6 }
            }
        }
    }
}
