//! 标签相关骨架屏
//!
//! 提供归档页标签索引与标签详情页的加载占位组件。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{SkeletonBox, SkeletonCard};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::pages::tags::TagIntro;

/// 归档页标签索引骨架屏组件。
///
/// 结构：图标与摘要，匹配默认折叠的卡片。
#[component]
pub fn TagsSkeleton() -> Element {
    rsx! {
        SkeletonCard { class: "archive-tags-skeleton mb-9 p-5 sm:p-6",
            div { class: "flex items-center gap-3",
                SkeletonBox { class: "h-11 w-11 shrink-0 rounded-2xl" }
                div { class: "flex-1 min-w-0",
                    SkeletonBox { class: "h-5 w-24 rounded mb-2" }
                    SkeletonBox { class: "h-3 w-48 max-w-full rounded" }
                }
                SkeletonBox { class: "h-4 w-4 rounded" }
            }
        }
    }
}

/// 标签详情页骨架屏组件。
///
/// 开场立即显示，数据区独立延迟，和真实页面使用相同的结构。
#[component]
pub fn TagDetailSkeleton(tag: String) -> Element {
    rsx! {
        div { class: "tag-page",
            TagIntro { tag }
            div { class: "tag-workspace", TagPostsLoading {} }
        }
    }
}

/// 状态即时播报、占位延迟出现，避免快速请求时闪屏及嵌套 pulse。
#[component]
pub fn TagPostsLoading() -> Element {
    rsx! {
        section { class: "tag-loading", aria_label: "主题文章", aria_busy: "true",
            p { class: "sr-only", role: "status", "正在加载文章，请稍候。" }
            DelayedSkeleton { pulse: false,
                div { class: "tag-skeleton", aria_hidden: "true", inert: true,
                    div { class: "tag-toolbar",
                        div { class: "tag-collection-meta",
                            span { class: "tag-skeleton-count tag-skeleton-block" }
                            span { class: "tag-skeleton-date tag-skeleton-block" }
                        }
                        span { class: "tag-skeleton-sort tag-skeleton-block" }
                    }
                    for index in 0..3 {
                        div { key: "{index}", class: if index == 0 { "tag-entry tag-featured" } else { "tag-entry" },
                            if index == 0 {
                                span { class: "tag-skeleton-label tag-skeleton-block" }
                            } else {
                                span { class: "tag-entry-number", span { class: "tag-skeleton-number tag-skeleton-block" } }
                            }
                            div { class: "tag-skeleton-body",
                                span { class: "tag-skeleton-date tag-skeleton-block" }
                                span { class: "tag-skeleton-title tag-skeleton-block" }
                                span { class: "tag-skeleton-title-short tag-skeleton-block" }
                                span { class: "tag-skeleton-summary tag-skeleton-block" }
                                div { class: "tag-skeleton-tags",
                                    for index in 0..3 { span { key: "{index}", class: "tag-skeleton-tag tag-skeleton-block" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
