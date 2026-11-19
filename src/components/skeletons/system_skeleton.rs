//! 后台系统骨架屏
//!
//! 与系统页同一版式：标题行、五个 tab 与服务器指标卡片。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 后台系统骨架屏组件。
#[component]
pub fn SystemSkeleton() -> impl IntoView {
    view! {
        <div class="admin-system" aria-hidden="true">
            <AdminPageHeadSkeleton title_width="4rem" subtitle_width="20rem"/>
            <SkeletonBox height="2rem" width="24rem" radius="9999px" class="sys-tabs" style="--skeleton-max-width: 100%"/>
            <div class="sys-metrics">
                {(0..8)
                    .map(|_| {
                        view! {
                            <div class="admin-panel sys-metric">
                                <SkeletonBox height="0.75rem" width="4rem" radius="0.25rem"/>
                                <SkeletonBox height="1.5rem" width="6rem" radius="0.25rem" style="--skeleton-margin-top: 0.5rem"/>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}
