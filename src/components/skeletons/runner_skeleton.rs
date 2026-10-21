//! 后台代码运行器骨架屏
//!
//! 与运行器页同一版式：标题行、语言分段与运行器、右侧资源上限。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 后台代码运行器骨架屏组件。
#[component]
pub fn RunnerSkeleton() -> Element {
    rsx! {
        div { class: "admin-runner", aria_hidden: "true",
            div { class: "admin-page-head",
                div {
                    SkeletonBox { class: "h-7 w-28 rounded-md" }
                    SkeletonBox { class: "h-3.5 w-72 rounded mt-2" }
                }
            }
            div { class: "runner-admin-grid",
                div {
                    SkeletonBox { class: "h-8 w-80 rounded-full mb-3" }
                    SkeletonBox { class: "h-72 w-full rounded-xl" }
                }
                div { class: "admin-panel runner-admin-side",
                    SkeletonBox { class: "h-4 w-20 rounded" }
                    SkeletonBox { class: "h-20 w-full rounded" }
                    SkeletonBox { class: "h-24 w-full rounded-lg" }
                }
            }
        }
    }
}
