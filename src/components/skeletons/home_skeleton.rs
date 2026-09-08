//! 首页骨架屏：静态开场与真实页面一致，文章区单独加载。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::post_card_skeleton::PostCardSkeleton;
use crate::pages::home::HomeIntro;

#[component]
pub fn HomeSkeleton(#[props(default = 1)] current_page: i32) -> Element {
    rsx! {
        div { class: "home-page", aria_hidden: "true", inert: true,
            HomeIntro { current_page }
            HomePostsSkeleton { current_page }
        }
    }
}

#[component]
pub fn HomePostsSkeleton(current_page: i32) -> Element {
    rsx! {
        div { aria_hidden: "true",
            div { class: "home-section-heading",
                SkeletonBox { class: "h-7 w-40" }
                SkeletonBox { class: "h-4 w-20" }
            }
            for i in 0..10 {
                div { key: "{i}", class: if current_page == 1 && i == 0 { "home-entry home-entry-featured" } else { "home-entry" },
                    if current_page == 1 && i == 0 {
                        div { class: "home-featured-label", SkeletonBox { class: "h-3 w-36" } }
                    } else {
                        span { class: "home-entry-number", SkeletonBox { class: "h-3 w-4" } }
                    }
                    PostCardSkeleton { compact: true }
                }
            }
            div { class: "flex mt-10 mb-6 justify-between",
                SkeletonBox { class: "h-10 w-24 rounded-full" }
                SkeletonBox { class: "h-10 w-24 rounded-full" }
            }
        }
    }
}
