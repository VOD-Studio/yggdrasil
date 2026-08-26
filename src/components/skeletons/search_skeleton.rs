//! 搜索页骨架屏
//!
//! 在搜索结果加载期间展示搜索卡片列表占位。

use dioxus::prelude::*;

use crate::components::skeletons::post_card_skeleton::PostCardSkeleton;

/// 搜索页骨架屏组件。
///
/// 模拟 3 个搜索结果卡片，与搜索页现有内联骨架结构一致。
#[component]
pub fn SearchSkeleton() -> Element {
    rsx! {
        div { class: "space-y-6 py-4",
            // 3 个结果卡片骨架
            for i in 0..3 {
                PostCardSkeleton { key: "{i}" }
            }
        }
    }
}
