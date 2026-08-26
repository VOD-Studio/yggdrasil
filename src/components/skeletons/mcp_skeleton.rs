//! 后台 MCP 服务骨架屏
//!
//! 镜像后台 Mcp 页面的结构：Header（标题+描述）+ Token 列表表格卡片 + 新建 Token 表单卡片 + 客户端配置网格。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{
    SkeletonBox, SkeletonCard, SkeletonCellShape, SkeletonTable, SkeletonTableColumn,
};

/// 后台 MCP 服务骨架屏组件。
#[component]
pub fn McpSkeleton() -> Element {
    let token_columns = vec![
        SkeletonTableColumn {
            header_class: "px-5 py-3.5",
            header_box_class: "h-3 w-16",
            cell_class: "px-5 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-28"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5",
            header_box_class: "h-3 w-16",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-5 w-16 rounded-md"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5",
            header_box_class: "h-3 w-14",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-4 w-20"),
        },
        SkeletonTableColumn {
            header_class: "px-4 py-3.5",
            header_box_class: "h-3 w-14",
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
            header_class: "px-4 py-3.5",
            header_box_class: "h-3 w-10 mx-auto",
            cell_class: "px-4 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-5 w-12 mx-auto rounded-full"),
        },
        SkeletonTableColumn {
            header_class: "px-5 py-3.5",
            header_box_class: "h-3 w-16 ml-auto",
            cell_class: "px-5 py-3.5",
            cell_shape: SkeletonCellShape::Single("h-6 w-36 ml-auto rounded"),
        },
    ];
    rsx! {
        div { class: "w-full max-w-7xl mx-auto space-y-8",
            // 页头
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70",
                div { class: "space-y-1.5",
                    SkeletonBox { class: "h-9 w-36 rounded-lg" }
                    SkeletonBox { class: "h-4 w-96 rounded" }
                }
                SkeletonBox { class: "h-8 w-32 rounded-full" }
            }

            // Token 列表表格卡片占位
            SkeletonCard { class: Some("p-6 sm:p-8 space-y-6 shadow-xs"),
                div { class: "flex justify-between items-center border-b border-[var(--color-paper-border)]/60 pb-4",
                    SkeletonBox { class: "h-6 w-36 rounded" }
                    SkeletonBox { class: "h-7 w-20 rounded-full" }
                }
                SkeletonCard { class: Some("overflow-hidden"),
                    SkeletonTable { columns: token_columns, rows: 3 }
                }
            }

            // 新建 Token 表单卡片占位
            SkeletonCard { class: Some("p-6 sm:p-8 space-y-6 shadow-xs"),
                SkeletonBox { class: "h-6 w-32 rounded" }
                div { class: "grid grid-cols-1 md:grid-cols-3 gap-5",
                    div { class: "space-y-2",
                        SkeletonBox { class: "h-3.5 w-16 rounded" }
                        SkeletonBox { class: "h-10 w-full rounded-2xl" }
                    }
                    div { class: "space-y-2",
                        SkeletonBox { class: "h-3.5 w-20 rounded" }
                        SkeletonBox { class: "h-10 w-full rounded-2xl" }
                    }
                    div { class: "space-y-2",
                        SkeletonBox { class: "h-3.5 w-16 rounded" }
                        SkeletonBox { class: "h-10 w-full rounded-2xl" }
                    }
                }
                SkeletonBox { class: "h-10 w-28 rounded-full" }
            }

            // 客户端配置卡片占位
            SkeletonCard { class: Some("p-6 sm:p-8 space-y-6 shadow-xs"),
                SkeletonBox { class: "h-6 w-36 rounded" }
                SkeletonBox { class: "h-10 w-full rounded-2xl" }
                div { class: "space-y-4",
                    for i in 0..3 {
                        div { key: "{i}", class: "p-5 rounded-2xl border border-[var(--color-paper-border)]/60 space-y-3",
                            div { class: "flex justify-between items-center",
                                SkeletonBox { class: "h-4 w-32 rounded" }
                                SkeletonBox { class: "h-7 w-16 rounded-full" }
                            }
                            SkeletonBox { class: "h-16 w-full rounded-xl" }
                        }
                    }
                }
            }
        }
    }
}
