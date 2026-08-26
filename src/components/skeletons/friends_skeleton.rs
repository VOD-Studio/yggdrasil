//! 友链页骨架屏
//!
//! 在友链数据加载期间展示卡片网格占位。纯色块，无动画
//! （符合规范「避免骨架屏截断动画」的反模式）。

use dioxus::prelude::*;

/// 友链页骨架屏组件。
///
/// 结构：两列网格内 6 个与真实卡片高度相近的圆角色块。
#[component]
pub fn FriendsSkeleton() -> Element {
    rsx! {
        div { class: "grid grid-cols-1 sm:grid-cols-2 gap-6",
            for i in 0..6 {
                div { key: "{i}", class: "h-40 rounded-card bg-paper-entry/60" }
            }
        }
    }
}
