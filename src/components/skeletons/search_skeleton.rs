//! 与搜索结果行对齐的骨架屏，也用于搜索路由的加载占位。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 结果标题的宽度，错落排布更像真实结果。
const TITLE_WIDTHS: [&str; 4] = ["width: 62%", "width: 78%", "width: 54%", "width: 70%"];

#[component]
pub fn SearchSkeleton() -> Element {
    rsx! {
        div { class: "search-skeleton", role: "status", aria_label: "正在搜索",
            div { aria_hidden: "true",
                SkeletonBox { class: "h-4 w-36 mb-4" }
                for (i, width) in TITLE_WIDTHS.iter().enumerate() {
                    div { key: "{i}", class: "search-result search-result-skeleton",
                        SkeletonBox { class: "h-4", style: *width }
                        SkeletonBox { class: "h-3 mt-3 w-full" }
                        SkeletonBox { class: "h-3 mt-2 w-4/5" }
                        SkeletonBox { class: "h-3 mt-3 w-40" }
                    }
                }
            }
        }
    }
}
