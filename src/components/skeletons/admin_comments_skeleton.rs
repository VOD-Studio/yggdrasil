//! 后台评论管理骨架屏
//!
//! 镜像后台 AdminComments 页面的结构：Header（标题+描述）+ 状态筛选 Tabs + 5 条评论卡片行。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{
    SkeletonBox, SkeletonCard, SkeletonCellShape, SkeletonTable, SkeletonTableColumn,
};
/// 评论表格骨架屏组件（纯表格部分，供 AdminCommentsPage 内部加载态使用）。
#[component]
pub fn AdminCommentsTableSkeleton() -> Element {
    let columns = vec![
        SkeletonTableColumn {
            header_class: "px-4 py-3.5 w-10 text-center",
            header_box_class: "h-4 w-4 rounded mx-auto",
            cell_class: "px-4 py-3.5 text-center",
            cell_shape: SkeletonCellShape::Single("h-4 w-4 rounded mx-auto"),
        },
        SkeletonTableColumn {
            header_class: "px-5 py-3.5 w-48",
            header_box_class: "h-3 w-16",
            cell_class: "px-5 py-3.5",
            cell_shape: SkeletonCellShape::AvatarStacked {
                avatar: "h-8 w-8 rounded-full shrink-0",
                line1: "h-3.5 w-20 rounded",
                line2: "h-2.5 w-28 rounded",
            },
        },
        SkeletonTableColumn {
            header_class: "px-5 py-3.5",
            header_box_class: "h-3 w-20",
            cell_class: "px-5 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-3/4 rounded"),
        },
        SkeletonTableColumn {
            header_class: "px-5 py-3.5 w-56",
            header_box_class: "h-3 w-16",
            cell_class: "px-5 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-3.5 w-32 rounded"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5 w-24",
            header_box_class: "h-3 w-10 mx-auto",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-5 w-14 mx-auto rounded-full"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5 w-28",
            header_box_class: "h-3 w-14",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-20"),
        },
        SkeletonTableColumn {
            header_class: "px-5 py-3.5 w-36",
            header_box_class: "h-3 w-12 ml-auto",
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
