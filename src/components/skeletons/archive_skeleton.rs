//! 归档与标签详情骨架屏：与真实版式逐块对齐（摘要行、标签区、年份分组）。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::pages::tags::TagIntro;

/// 标签占位的宽度，错落排布更像真实标签。
const CHIP_WIDTHS: [&str; 10] = [
    "width: 4.5rem",
    "width: 3.5rem",
    "width: 5.5rem",
    "width: 4rem",
    "width: 6rem",
    "width: 3.75rem",
    "width: 5rem",
    "width: 4.25rem",
    "width: 3.25rem",
    "width: 5.25rem",
];
/// 文章行标题的宽度。
const ROW_WIDTHS: [&str; 5] = [
    "width: 72%",
    "width: 58%",
    "width: 80%",
    "width: 64%",
    "width: 50%",
];

/// 归档页整页占位：页标题之下的全部区块。
#[component]
pub fn ArchiveSkeleton() -> Element {
    rsx! {
        div { class: "archive",
            header { class: "archive-header",
                h1 { class: "archive-title", "归档" }
            }
            ArchiveBodySkeleton {}
        }
    }
}

/// 标签详情整页占位：开场照常显示，只有数据区占位。
#[component]
pub fn TagDetailSkeleton(tag: String) -> Element {
    rsx! {
        div { class: "archive",
            TagIntro { tag }
            ArchiveBodySkeleton {}
        }
    }
}

/// 摘要行、标签区与两个年份分组的占位。
#[component]
pub fn ArchiveBodySkeleton() -> Element {
    rsx! {
        div { role: "status", aria_label: "正在加载文章",
            div { aria_hidden: "true",
                SkeletonBox { class: "archive-summary h-4 w-60 max-w-full" }
                div { class: "archive-tags",
                    div { class: "archive-tags-head", SkeletonBox { class: "h-3 w-14" } }
                    div { class: "archive-tags-list",
                        for (i, width) in CHIP_WIDTHS.iter().enumerate() {
                            SkeletonBox { key: "{i}", class: "h-7 rounded-full", style: *width }
                        }
                    }
                }
                div { class: "archive-years",
                    for y in 0..2 {
                        div { key: "{y}", class: "archive-year",
                            div { class: "archive-year-label",
                                SkeletonBox { class: "h-8 w-20" }
                                SkeletonBox { class: "h-3 w-10 mt-2" }
                            }
                            div { class: "archive-rows",
                                for (i, width) in ROW_WIDTHS.iter().enumerate() {
                                    div { key: "{i}", class: "archive-entry archive-entry-skeleton",
                                        SkeletonBox { class: "h-3 w-10" }
                                        SkeletonBox { class: "h-4", style: *width }
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
