//! 首页骨架屏
//!
//! 模拟首页文章卡片列表与分页区域。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::post_card_skeleton::PostCardSkeleton;

/// 首页骨架屏组件。
///
/// `with_intro` 用于路由级加载占位；文章流内部加载时不重复渲染介绍侧栏。
/// 封面是可选内容，不预留整张大图，避免无封面的文章加载后发生大幅跳动。
#[component]
pub fn HomeSkeleton(
    #[props(default = false)] with_intro: bool,
    #[props(default = false)] paginated: bool,
) -> Element {
    rsx! {
        div { class: if with_intro { "home-layout" } else { "" }, aria_hidden: "true",
            if with_intro && paginated {
                div { class: "home-intro home-page-heading",
                    SkeletonBox { class: "home-intro-kicker h-4 w-20" }
                    SkeletonBox { class: "home-intro-title h-10 w-40" }
                    SkeletonBox { class: "home-back-link h-11 w-28 rounded-full" }
                }
            } else if with_intro {
                div { class: "home-intro flex flex-col gap-5",
                    SkeletonBox { class: "h-4 w-32" }
                    SkeletonBox { class: "h-10 w-56" }
                    SkeletonBox { class: "h-10 w-40" }
                    SkeletonBox { class: "h-16 w-full" }
                    SkeletonBox { class: "h-10 w-28 rounded-full" }
                }
            }
            div { class: "home-feed",
                div { class: "home-feed-heading",
                    SkeletonBox { class: "h-4 w-20" }
                    SkeletonBox { class: "h-4 w-16" }
                }
                for i in 0..10 {
                    PostCardSkeleton { key: "{i}", compact: true }
                }
                div { class: "flex mt-10 mb-6 justify-between",
                    SkeletonBox { class: "h-9 w-24 rounded-full" }
                    SkeletonBox { class: "h-9 w-24 rounded-full" }
                }
            }
        }
    }
}
