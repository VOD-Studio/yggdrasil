//! 后台友链管理骨架屏
//!
//! 与友链页同一版式：标题行、左侧友链列表、右侧表单。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 友链列表骨架（列表加载中使用）。
#[component]
pub fn FriendsAdminListSkeleton() -> impl IntoView {
    view! {
        <ol class="admin-panel admin-friends-list" aria-hidden="true">
            {(0..5)
                .map(|_| {
                    view! {
                        <li class="admin-friend">
                            <SkeletonBox height="1rem" width="0.75rem" radius="0.25rem"/>
                            <SkeletonBox height="34px" width="34px" radius="10px"/>
                            <div class="admin-friend-text">
                                <SkeletonBox height="1rem" width="8rem" radius="0.25rem"/>
                                <SkeletonBox height="0.75rem" width="10rem" radius="0.25rem" style="--skeleton-margin-top: 0.375rem"/>
                            </div>
                            <SkeletonBox height="0.75rem" width="1.5rem" radius="0.25rem"/>
                        </li>
                    }
                })
                .collect_view()}
        </ol>
    }
}

/// 友链管理全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn FriendsAdminSkeleton() -> impl IntoView {
    view! {
        <div class="admin-friends" aria-hidden="true">
            <AdminPageHeadSkeleton
                title_width="4rem"
                subtitle_width="20rem"
                actions=|| view! { <SkeletonBox height="34px" width="7rem" radius="0.5rem"/> }
            />
            <div class="admin-friends-grid">
                <div class="admin-friends-list-col">
                    <FriendsAdminListSkeleton/>
                </div>
                <div class="admin-friend-form">
                    <SkeletonBox height="1.25rem" width="5rem" radius="0.25rem"/>
                    <SkeletonBox height="2.25rem" width="100%" radius="0.5rem"/>
                    <SkeletonBox height="2.25rem" width="100%" radius="0.5rem"/>
                    <SkeletonBox height="5rem" width="100%" radius="0.5rem"/>
                </div>
            </div>
        </div>
    }
}
