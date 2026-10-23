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
            <AdminPageHeadSkeleton title_width="w-16" subtitle_width="w-80"/>
            <SkeletonBox class="h-8 w-96 max-w-full rounded-full sys-tabs"/>
            <div class="sys-metrics">
                {(0..8)
                    .map(|_| {
                        view! {
                            <div class="admin-panel sys-metric">
                                <SkeletonBox class="h-3 w-16 rounded"/>
                                <SkeletonBox class="h-6 w-24 rounded mt-2"/>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}
