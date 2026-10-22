//! 后台友链管理骨架屏
//!
//! 与友链页同一版式：标题行、左侧友链列表、右侧表单。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 友链列表骨架（列表加载中使用）。
#[component]
pub fn FriendsAdminListSkeleton() -> Element {
    rsx! {
        ol { class: "admin-panel admin-friends-list", aria_hidden: "true",
            for i in 0..5 {
                li { key: "{i}", class: "admin-friend",
                    SkeletonBox { class: "h-4 w-3 rounded" }
                    SkeletonBox { class: "h-[34px] w-[34px] rounded-[10px]" }
                    div { class: "admin-friend-text",
                        SkeletonBox { class: "h-4 w-32 rounded" }
                        SkeletonBox { class: "h-3 w-40 rounded mt-1.5" }
                    }
                    SkeletonBox { class: "h-3 w-6 rounded" }
                }
            }
        }
    }
}

/// 友链管理全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn FriendsAdminSkeleton() -> Element {
    rsx! {
        div { class: "admin-friends", aria_hidden: "true",
            AdminPageHeadSkeleton {
                title_width: "w-16",
                subtitle_width: "w-80",
                actions: rsx! {
                    SkeletonBox { class: "h-[34px] w-28 rounded-lg" }
                },
            }
            div { class: "admin-friends-grid",
                div { class: "admin-friends-list-col", FriendsAdminListSkeleton {} }
                div { class: "admin-friend-form",
                    SkeletonBox { class: "h-5 w-20 rounded" }
                    SkeletonBox { class: "h-9 w-full rounded-lg" }
                    SkeletonBox { class: "h-9 w-full rounded-lg" }
                    SkeletonBox { class: "h-20 w-full rounded-lg" }
                }
            }
        }
    }
}
