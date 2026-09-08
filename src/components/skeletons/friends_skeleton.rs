//! 与友链页同尺寸的静态占位；由 DelayedSkeleton 统一控制加载提示。

use dioxus::prelude::*;

#[component]
pub fn FriendsSkeleton(#[props(default = true)] with_header: bool) -> Element {
    rsx! {
        div { class: "friends-skeleton", role: "status", aria_label: "正在加载友链",
            div { aria_hidden: "true",
                if with_header {
                    div { class: "friends-intro friends-skeleton-intro",
                        div { class: "friends-skeleton-line w-40 h-3" }
                        div { class: "friends-skeleton-line w-32 h-14" }
                        div { class: "friends-skeleton-line w-64 max-w-full h-5" }
                    }
                }
                div { class: "friends-section-heading",
                    div { class: "friends-skeleton-line w-28 h-5" }
                }
                div { class: "friends-grid",
                    for i in 0..4 {
                        div { key: "{i}", class: "friend-card friends-skeleton-card",
                            div { class: "friends-skeleton-line w-13 h-13 rounded-2xl" }
                            div { class: "friends-skeleton-line w-36 h-5 mt-5" }
                            div { class: "friends-skeleton-line w-full h-4 mt-3" }
                            div { class: "friends-skeleton-line w-2/3 h-4 mt-2" }
                            div { class: "friends-skeleton-line w-24 h-3 mt-auto" }
                        }
                    }
                }
            }
        }
    }
}
