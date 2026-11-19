//! 后台仪表盘骨架屏
//!
//! 仅在 AdminLayout 的内容区展示，校验登录状态与路由挂起期间与仪表盘同一版式：
//! 标题行、四格数字、左栏柱图与近期文章、右栏评论与站点。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 仪表盘内容区骨架屏组件。
#[component]
pub fn AdminDashboardSkeleton() -> impl IntoView {
    view! {
        <div class="admin-dash" aria-hidden="true">
            <AdminPageHeadSkeleton title_width="11rem" subtitle_width="14rem"/>
            <div class="dash-tiles">
                {(0..4)
                    .map(|_| {
                        view! {
                            <div class="admin-panel dash-tile">
                                <SkeletonBox height="0.875rem" width="3rem" radius="0.25rem"/>
                                <SkeletonBox class="dash-tile-value-skeleton"/>
                                <SkeletonBox height="0.75rem" width="6rem" radius="0.25rem" style="--skeleton-margin-top: 0.5rem"/>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
            <div class="dash-cols">
                <div class="dash-col">
                    <div class="admin-panel dash-chart">
                        <SkeletonBox height="1rem" width="7rem" radius="0.25rem"/>
                        <SkeletonBox class="dash-chart-skeleton"/>
                    </div>
                    <div class="admin-panel dash-list">
                        <div class="dash-panel-head dash-panel-head-rule">
                            <SkeletonBox height="1rem" width="5rem" radius="0.25rem"/>
                        </div>
                        <div class="dash-list-body">
                            {(0..5)
                                .map(|_| view! { <SkeletonBox class="dash-row-skeleton"/> })
                                .collect_view()}
                        </div>
                    </div>
                </div>
                <div class="dash-col">
                    <div class="admin-panel dash-list">
                        <div class="dash-panel-head dash-panel-head-rule">
                            <SkeletonBox height="1rem" width="5rem" radius="0.25rem"/>
                        </div>
                        <div class="dash-list-body">
                            {(0..4)
                                .map(|_| view! { <SkeletonBox class="dash-comment-skeleton"/> })
                                .collect_view()}
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
