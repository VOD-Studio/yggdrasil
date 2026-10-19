//! 首页骨架屏：与「书桌」布局逐块对齐，数据到达后不跳动。
//! 最新一篇面板的右栏固定 6 行目录占位。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 目录占位每行的宽度，错落排布更像真实标题。
const OUTLINE_WIDTHS: [&str; 6] = [
    "width: 70%",
    "width: 90%",
    "width: 60%",
    "width: 85%",
    "width: 75%",
    "width: 50%",
];
/// 索引每行标题的宽度。
const ROW_WIDTHS: [&str; 5] = [
    "width: 70%",
    "width: 55%",
    "width: 80%",
    "width: 62%",
    "width: 74%",
];

#[component]
pub fn HomeSkeleton(#[props(default = 1)] current_page: i32) -> Element {
    if current_page > 1 {
        return rsx! {
            div { class: "home home-paged", role: "status", aria_label: "正在加载文章",
                header { class: "home-paged-header",
                    SkeletonBox { class: "h-4 w-20" }
                    SkeletonBox { class: "home-skeleton-title w-40" }
                    SkeletonBox { class: "h-4 w-16" }
                }
                HomePostsSkeleton { current_page }
            }
        };
    }
    rsx! {
        div { class: "home", role: "status", aria_label: "正在加载首页",
            div { class: "home-hero",
                SkeletonBox { class: "h-3 w-56 max-w-full" }
                SkeletonBox { class: "home-skeleton-title mt-6 w-[22rem] max-w-full" }
                SkeletonBox { class: "home-skeleton-title mt-3 w-[17rem] max-w-full" }
                SkeletonBox { class: "h-3.5 mt-6 w-[28rem] max-w-full" }
                SkeletonBox { class: "home-skeleton-composer mt-9" }
            }
            div { class: "home-section",
                SkeletonBox { class: "h-3 w-16 mb-4" }
                div { class: "home-latest",
                    div { class: "home-latest-main",
                        SkeletonBox { class: "h-3 w-40" }
                        SkeletonBox { class: "h-7 mt-4 w-4/5" }
                        SkeletonBox { class: "h-3 mt-4 w-full" }
                        SkeletonBox { class: "h-3 mt-2.5 w-3/4" }
                    }
                    div { class: "home-latest-outline",
                        SkeletonBox { class: "h-3 w-16 mb-3" }
                        div { class: "flex flex-col gap-3",
                            for (i, width) in OUTLINE_WIDTHS.iter().enumerate() {
                                SkeletonBox { key: "{i}", class: "h-3", style: *width }
                            }
                        }
                    }
                }
            }
            HomePostsSkeleton { current_page }
        }
    }
}

/// 文章索引占位：月份分隔与五行条目。
#[component]
pub fn HomePostsSkeleton(current_page: i32) -> Element {
    rsx! {
        div { class: "home-index", aria_hidden: "true",
            if current_page == 1 {
                div { class: "home-index-heading",
                    SkeletonBox { class: "h-5 w-28" }
                    SkeletonBox { class: "h-4 w-12" }
                }
            }
            div { class: "home-month",
                SkeletonBox { class: "home-month-label h-3 w-16" }
                for (i, width) in ROW_WIDTHS.iter().enumerate() {
                    div { key: "{i}", class: "home-row home-row-skeleton",
                        SkeletonBox { class: "h-3 w-10" }
                        div {
                            SkeletonBox { class: "h-4", style: *width }
                            SkeletonBox { class: "h-3 mt-2 w-[90%]" }
                        }
                        SkeletonBox { class: "home-row-meta h-3 w-14" }
                    }
                }
            }
        }
    }
}
