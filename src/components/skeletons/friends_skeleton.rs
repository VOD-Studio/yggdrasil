//! 与友链页同尺寸的占位：页头照常显示，名片与交换信息占位。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::pages::friends::FriendsHeader;

#[component]
pub fn FriendsSkeleton() -> Element {
    rsx! {
        FriendsHeader {}
        div { role: "status", aria_label: "正在加载友链",
            div { class: "friends-grid", aria_hidden: "true",
                for i in 0..4 {
                    div { key: "{i}", class: "friend-card friend-card-skeleton",
                        SkeletonBox { class: "h-11 w-11 shrink-0 rounded-xl" }
                        div { class: "flex-1",
                            SkeletonBox { class: "h-4 w-28" }
                            SkeletonBox { class: "h-3 mt-2 w-24" }
                            SkeletonBox { class: "h-3 mt-3 w-full" }
                        }
                    }
                }
            }
        }
    }
}
