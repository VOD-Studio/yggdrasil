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
                                <SkeletonBox height="2.75rem" width="2.75rem" radius="0.75rem" style="--skeleton-flex-shrink: 0"/>
                                <div class="friend-skeleton-copy">
                                    <SkeletonBox height="1rem" width="7rem"/>
                                    <SkeletonBox height="0.75rem" width="6rem" style="--skeleton-margin-top: 0.5rem"/>
                                    <SkeletonBox height="0.75rem" width="100%" style="--skeleton-margin-top: 0.75rem"/>
                                </div>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}
