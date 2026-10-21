//! 后台 MCP 接入骨架屏
//!
//! 与 MCP 页同一版式：标题行、一句话接入卡片与令牌表格。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 后台 MCP 接入骨架屏组件。
#[component]
pub fn McpSkeleton() -> Element {
    rsx! {
        div { class: "admin-mcp", aria_hidden: "true",
            div { class: "admin-page-head",
                div {
                    SkeletonBox { class: "h-7 w-28 rounded-md" }
                    SkeletonBox { class: "h-3.5 w-96 max-w-full rounded mt-2" }
                }
                SkeletonBox { class: "h-[34px] w-28 rounded-lg" }
            }
            div { class: "admin-panel mcp-oneline",
                SkeletonBox { class: "h-4 w-24 rounded" }
                SkeletonBox { class: "h-12 w-full rounded-lg mt-3" }
                SkeletonBox { class: "h-8 w-48 rounded-lg mt-3" }
            }
            div { class: "mcp-tokens",
                SkeletonBox { class: "h-4 w-28 rounded mb-3" }
                div { class: "admin-panel",
                    for i in 0..3 {
                        div { key: "{i}", class: "mcp-skeleton-row",
                            SkeletonBox { class: "h-4 w-40 rounded" }
                            SkeletonBox { class: "h-4 w-16 rounded" }
                            SkeletonBox { class: "h-4 w-24 rounded" }
                        }
                    }
                }
            }
        }
    }
}
