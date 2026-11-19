//! 后台代码运行器骨架屏
//!
//! 与运行器页同一版式：标题行、语言分段与运行器、右侧资源上限。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 后台代码运行器骨架屏组件。
#[component]
pub fn RunnerSkeleton() -> impl IntoView {
    view! {
        <div class="admin-runner" aria-hidden="true">
            <AdminPageHeadSkeleton title_width="7rem" subtitle_width="18rem"/>
            <div class="runner-admin-grid">
                <div>
                    <SkeletonBox height="2rem" width="20rem" radius="9999px" style="--skeleton-margin-bottom: 0.75rem"/>
                    <SkeletonBox height="18rem" width="100%" radius="0.75rem"/>
                </div>
                <div class="admin-panel runner-admin-side">
                    <SkeletonBox height="1rem" width="5rem" radius="0.25rem"/>
                    <SkeletonBox height="5rem" width="100%" radius="0.25rem"/>
                    <SkeletonBox height="6rem" width="100%" radius="0.5rem"/>
                </div>
            </div>
        </div>
    }
}
