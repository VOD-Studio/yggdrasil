//! 后台运行日志页骨架屏
//!
//! 与日志页同一版式：标题行、级别与筛选栏、占满剩余高度的日志区。根节点沿用页面的
//! 内部滚动容器类，撑满主内容区高度。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 后台运行日志页骨架屏组件。
#[component]
pub fn LogsSkeleton() -> Element {
    rsx! {
        div { class: "logs-page", aria_hidden: "true",
            AdminPageHeadSkeleton {
                title_width: "w-28",
                subtitle_width: "w-64",
                class: "logs-head",
                actions: rsx! {
                    SkeletonBox { class: "h-[34px] w-24 rounded-lg" }
                    SkeletonBox { class: "h-[34px] w-24 rounded-lg" }
                },
            }
            div { class: "logs-filters",
                SkeletonBox { class: "h-[30px] w-72 rounded-lg" }
                SkeletonBox { class: "h-[30px] w-32 rounded-lg" }
                SkeletonBox { class: "h-[30px] w-56 rounded-lg" }
            }
            div { class: "admin-panel logs-pane",
                div { class: "logs-skeleton",
                    for i in 0..12 {
                        SkeletonBox { key: "{i}", class: "h-3.5 w-full rounded" }
                    }
                }
            }
        }
    }
}
