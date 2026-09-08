//! 归档页骨架屏
//!
//! 在归档数据加载期间展示标签索引与按年份/月份分组的文章列表占位。

use crate::components::skeletons::atoms::*;
use crate::components::skeletons::tags_skeleton::TagsSkeleton;
use dioxus::prelude::*;

/// 归档页骨架屏组件。
///
/// 结构：可选标签索引 + 统计行 + 左侧年份 + 月份时间线。
/// 页面内部的独立文章加载边界传 `include_tags: false`，避免重复标签卡片。
/// 模拟 2 个年份，每个年份 2 个月，每个月 3 篇文章。
#[component]
pub fn ArchiveSkeleton(#[props(default = true)] include_tags: bool) -> Element {
    rsx! {
        div { class: "archive-skeleton", role: "status", aria_label: "正在加载文章归档",
            if include_tags {
                TagsSkeleton {}
            }
            div { class: "archive-timeline", aria_hidden: "true",
                div { class: "archive-toolbar",
                    SkeletonBox { class: "h-5 w-40 rounded" }
                    SkeletonBox { class: "h-3 w-16 rounded" }
                }
                for y in 0..2 {
                    div { key: "{y}", class: "archive-year",
                        div { class: "archive-year-heading",
                            SkeletonBox { class: "h-2 w-20 rounded mb-3" }
                            SkeletonBox { class: "h-14 w-32 rounded mb-3" }
                            SkeletonBox { class: "h-3 w-28 rounded" }
                        }
                        div { class: "archive-months",
                            for m in 0..2 {
                                div { key: "{m}", class: "archive-month",
                                    div { class: "archive-month-header",
                                        SkeletonBox { class: "h-5 w-32 rounded" }
                                        SkeletonBox { class: "h-3 w-8 rounded" }
                                    }
                                    for e in 0..3 {
                                        div { key: "{e}", class: "archive-entry",
                                            SkeletonBox { class: "h-4 w-5 rounded mt-1" }
                                            div {
                                                SkeletonBox { class: "h-5 w-3/4 rounded mb-2" }
                                                SkeletonBox { class: "h-3 w-28 rounded" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
