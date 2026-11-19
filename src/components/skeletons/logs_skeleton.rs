//! 后台运行日志页骨架屏
//!
//! 与日志页同一版式：标题行、级别与筛选栏、占满剩余高度的日志区。根节点沿用页面的内部滚动
//! 容器类，撑满主内容区高度。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 后台运行日志页骨架屏组件。
#[component]
pub fn LogsSkeleton() -> impl IntoView {
    view! {
        <div class="logs-page" aria-hidden="true">
            <AdminPageHeadSkeleton
                title_width="7rem"
                subtitle_width="16rem"
                class="logs-head"
                actions=|| {
                    view! {
                        <SkeletonBox height="34px" width="6rem" radius="0.5rem"/>
                        <SkeletonBox height="34px" width="6rem" radius="0.5rem"/>
                    }
                }
            />
            <div class="logs-filters">
                <SkeletonBox height="30px" width="18rem" radius="0.5rem"/>
                <SkeletonBox height="30px" width="8rem" radius="0.5rem"/>
                <SkeletonBox height="30px" width="14rem" radius="0.5rem"/>
            </div>
            <div class="admin-panel logs-pane">
                <div class="logs-skeleton">
                    {(0..12)
                        .map(|_| view! { <SkeletonBox height="0.875rem" width="100%" radius="0.25rem"/> })
                        .collect_view()}
                </div>
            </div>
        </div>
    }
}
