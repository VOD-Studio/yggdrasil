//! 首页骨架屏
//!
//! 模拟首页文章卡片列表与分页区域。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::post_card_skeleton::PostCardSkeleton;

/// 首页骨架屏组件。
///
/// 路由级与文章流内部加载共用同一列表占位。
/// 封面是可选内容，不预留整张大图，避免无封面的文章加载后发生大幅跳动。
#[component]
pub fn HomeSkeleton() -> Element {
    rsx! {
        div { aria_hidden: "true",
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
