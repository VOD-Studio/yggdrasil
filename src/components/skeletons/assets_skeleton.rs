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
                                <SkeletonBox class="aspect-[4/3] w-full rounded-none"/>
                                <span class="asset-tile-text">
                                    <SkeletonBox class="h-3.5 w-3/4 rounded"/>
                                    <SkeletonBox class="h-3 w-1/2 rounded mt-1.5"/>
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
                    title_width="w-20"
                    subtitle_width="w-64"
                    actions=|| {
                        view! {
                            <SkeletonBox class="h-[34px] w-[34px] rounded-lg"/>
                            <SkeletonBox class="h-[34px] w-20 rounded-lg"/>
                        }
                    }
                />
                <div class="admin-toolbar">
                    <SkeletonBox class="h-8 w-64 rounded-full"/>
                    <div class="admin-toolbar-filters">
                        <SkeletonBox class="h-[30px] w-60 rounded-lg"/>
                        <SkeletonBox class="h-[30px] w-24 rounded-lg"/>
                    </div>
                </div>
                <AssetsGridSkeleton/>
            </div>
        </div>
    }
}
