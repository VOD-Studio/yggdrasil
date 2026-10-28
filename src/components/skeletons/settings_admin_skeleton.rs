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
                        .map(|_| view! { <SkeletonBox class="h-[30px] w-full rounded-[7px]"/> })
                        .collect_view()}
                </div>
                <div class="settings-main">
                    <AdminPageHeadSkeleton title_width="w-16" subtitle_width="w-80"/>
                    {(0..3)
                        .map(|_| {
                            view! {
                                <div class="settings-slot">
                                    <div class="admin-panel settings-card">
                                        <div class="settings-card-head">
                                            <div>
                                                <SkeletonBox class="h-4 w-20 rounded"/>
                                                <SkeletonBox class="h-3 w-64 rounded mt-2"/>
                                            </div>
                                        </div>
                                        <div class="settings-rows">
                                            {(0..2)
                                                .map(|_| {
                                                    view! {
                                                        <div class="settings-row">
                                                            <SkeletonBox class="h-3.5 w-20 rounded"/>
                                                            <SkeletonBox class="h-9 w-full rounded-lg"/>
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
