//! 与友链页同尺寸的占位：页头照常显示，名片与交换信息占位。

use leptos::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::pages::friends::FriendsHeader;

#[component]
pub fn FriendsSkeleton() -> impl IntoView {
    view! {
        <FriendsHeader/>
        <div role="status" aria-label="正在加载友链">
            <div class="friends-grid" aria-hidden="true">
                {(0..4)
                    .map(|_| {
                        view! {
                            <div class="friend-card friend-card-skeleton">
                                <SkeletonBox class="h-11 w-11 shrink-0 rounded-xl"/>
                                <div class="flex-1">
                                    <SkeletonBox class="h-4 w-28"/>
                                    <SkeletonBox class="h-3 mt-2 w-24"/>
                                    <SkeletonBox class="h-3 mt-3 w-full"/>
                                </div>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}
