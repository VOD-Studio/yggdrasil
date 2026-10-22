//! 后台评论管理骨架屏
//!
//! 与评论管理同一版式：标题行、状态选项卡与评论流。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 评论流骨架（列表加载中使用）。
#[component]
pub fn AdminCommentsListSkeleton() -> Element {
    rsx! {
        div { class: "admin-panel admin-comment-list", aria_hidden: "true",
            for i in 0..5 {
                div { key: "{i}", class: "admin-comment",
                    SkeletonBox { class: "h-4 w-4 rounded" }
                    SkeletonBox { class: "h-[30px] w-[30px] rounded-full" }
                    div { class: "admin-comment-body",
                        SkeletonBox { class: "h-3.5 w-40 rounded" }
                        SkeletonBox { class: "h-4 w-4/5 rounded mt-2.5" }
                        SkeletonBox { class: "h-3 w-48 rounded mt-3" }
                    }
                }
            }
        }
    }
}

/// 评论管理全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn AdminCommentsSkeleton() -> Element {
    rsx! {
        div { class: "admin-comments", aria_hidden: "true",
            AdminPageHeadSkeleton {
                title_width: "w-20",
                subtitle_width: "w-28",
            }
            div { class: "admin-toolbar",
                SkeletonBox { class: "h-8 w-80 rounded-full" }
            }
            AdminCommentsListSkeleton {}
        }
    }
}
