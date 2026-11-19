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
                                <SkeletonBox height="1rem" width="5rem" radius="0.25rem"/>
                                <SkeletonBox height="0.75rem" width="16rem" radius="0.25rem" style="--skeleton-margin-top: 0.5rem"/>
                            </div>
                            <div class="admin-profile-rows">
                                {(0..rows)
                                    .map(|_| {
                                        view! {
                                            <div class="admin-profile-row">
                                                <SkeletonBox height="0.875rem" width="4rem" radius="0.25rem"/>
                                                <SkeletonBox height="2.25rem" width="100%" radius="0.5rem"/>
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
