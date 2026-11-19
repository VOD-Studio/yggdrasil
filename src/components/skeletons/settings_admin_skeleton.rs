//! 后台设置骨架屏
//!
//! 与设置页同一版式：左侧分区目录、右侧标题与分区卡片。根节点沿用页面的内部滚动
//! 容器类，撑满主内容区高度。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 后台设置骨架屏组件。
#[component]
pub fn SettingsAdminSkeleton() -> impl IntoView {
    view! {
        <div class="settings-page" aria-hidden="true">
            <div class="settings-layout">
                <div class="settings-nav">
                    {(0..9)
                        .map(|_| view! { <SkeletonBox height="30px" width="100%" radius="7px"/> })
                        .collect_view()}
                </div>
                <div class="settings-main">
                    <AdminPageHeadSkeleton title_width="4rem" subtitle_width="20rem"/>
                    {(0..3)
                        .map(|_| {
                            view! {
                                <div class="settings-slot">
                                    <div class="admin-panel settings-card">
                                        <div class="settings-card-head">
                                            <div>
                                                <SkeletonBox height="1rem" width="5rem" radius="0.25rem"/>
                                                <SkeletonBox height="0.75rem" width="16rem" radius="0.25rem" style="--skeleton-margin-top: 0.5rem"/>
                                            </div>
                                        </div>
                                        <div class="settings-rows">
                                            {(0..2)
                                                .map(|_| {
                                                    view! {
                                                        <div class="settings-row">
                                                            <SkeletonBox height="0.875rem" width="5rem" radius="0.25rem"/>
                                                            <SkeletonBox height="2.25rem" width="100%" radius="0.5rem"/>
                                                        </div>
                                                    }
                                                })
                                                .collect_view()}
                                        </div>
                                    </div>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
        </div>
    }
}
