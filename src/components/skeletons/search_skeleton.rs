//! 与搜索索引条目对齐的骨架屏，也用于搜索路由的加载占位。

use dioxus::prelude::*;

#[component]
pub fn SearchSkeleton() -> Element {
    rsx! {
        div { class: "search-skeleton", aria_hidden: "true",
            for i in 0..3 {
                div { key: "{i}", class: "search-skeleton-row",
                    span { class: "search-skeleton-number" }
                    div {
                        div { class: "search-skeleton-meta" }
                        div { class: "search-skeleton-title" }
                        div { class: "search-skeleton-summary" }
                        div { class: "search-skeleton-tag" }
                    }
                }
            }
        }
    }
}
