//! 后台个人资料骨架屏
//!
//! 与个人资料页同一版式：个人信息、修改密码两个分区与退出登录行。

use leptos::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 后台个人资料骨架屏组件。
#[component]
pub fn ProfileSkeleton() -> impl IntoView {
    view! {
        <div class="admin-profile-skeleton" aria-hidden="true">
            {[4, 3]
                .into_iter()
                .map(|rows| {
                    view! {
                        <div class="admin-panel admin-profile-section">
                            <div class="admin-profile-section-head">
                                <SkeletonBox class="h-4 w-20 rounded"/>
                                <SkeletonBox class="h-3 w-64 rounded mt-2"/>
                            </div>
                            <div class="admin-profile-rows">
                                {(0..rows)
                                    .map(|_| {
                                        view! {
                                            <div class="admin-profile-row">
                                                <SkeletonBox class="h-3.5 w-16 rounded"/>
                                                <SkeletonBox class="h-9 w-full rounded-lg"/>
                                            </div>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}
