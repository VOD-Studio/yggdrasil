//! 后台素材管理骨架屏
//!
//! 与素材页同一版式：标题行、引用状态选项卡与搜索排序、缩略图网格。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 缩略图网格骨架（列表加载中使用）。
#[component]
pub fn AssetsGridSkeleton() -> impl IntoView {
    view! {
        <ul class="asset-grid" aria-hidden="true">
            {(0..12)
                .map(|_| {
                    view! {
                        <li class="asset-tile">
                            <span class="asset-tile-open">
                                <SkeletonBox width="100%" radius="0" style="--skeleton-aspect-ratio: 4/3"/>
                                <span class="asset-tile-text">
                                    <SkeletonBox height="0.875rem" width="calc(3 / 4 * 100%)" radius="0.25rem"/>
                                    <SkeletonBox height="0.75rem" width="calc(1 / 2 * 100%)" radius="0.25rem" style="--skeleton-margin-top: 0.375rem"/>
                                </span>
                            </span>
                        </li>
                    }
                })
                .collect_view()}
        </ul>
    }
}

/// 素材管理全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn AssetsSkeleton() -> impl IntoView {
    view! {
        <div class="admin-assets" aria-hidden="true">
            <div class="admin-assets-main">
                <AdminPageHeadSkeleton
                    title_width="5rem"
                    subtitle_width="16rem"
                    actions=|| {
                        view! {
                            <SkeletonBox height="34px" width="34px" radius="0.5rem"/>
                            <SkeletonBox height="34px" width="5rem" radius="0.5rem"/>
                        }
                    }
                />
                <div class="admin-toolbar">
                    <SkeletonBox height="2rem" width="16rem" radius="9999px"/>
                    <div class="admin-toolbar-filters">
                        <SkeletonBox height="30px" width="15rem" radius="0.5rem"/>
                        <SkeletonBox height="30px" width="6rem" radius="0.5rem"/>
                    </div>
                </div>
                <AssetsGridSkeleton/>
            </div>
        </div>
    }
}
