//! 后台回收站骨架屏
//!
//! 与回收站同一版式：标题行（说明、清空回收站）、表格与底栏分页。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 回收站表格骨架屏（列表加载中使用）。
#[component]
pub fn PostsTrashTableSkeleton() -> impl IntoView {
    view! {
        <div class="admin-panel admin-table-wrap" aria-hidden="true">
            <table class="admin-table admin-trash-table">
                <thead>
                    <tr>
                        <th class="admin-table-check">
                            <SkeletonBox class="h-4 w-4 rounded"/>
                        </th>
                        <th>
                            <SkeletonBox class="h-3 w-10 rounded"/>
                        </th>
                        <th>
                            <SkeletonBox class="h-3 w-12 rounded"/>
                        </th>
                        <th class="admin-trash-deleted">
                            <SkeletonBox class="h-3 w-12 rounded"/>
                        </th>
                        <th class="admin-trash-left">
                            <SkeletonBox class="h-3 w-14 rounded"/>
                        </th>
                        <th class="admin-trash-actions"></th>
                    </tr>
                </thead>
                <tbody>
                    {(0..6)
                        .map(|_| {
                            view! {
                                <tr>
                                    <td class="admin-table-check">
                                        <SkeletonBox class="h-4 w-4 rounded"/>
                                    </td>
                                    <td class="admin-trash-title">
                                        <SkeletonBox class="h-4 w-3/4 rounded"/>
                                        <SkeletonBox class="h-3 w-1/3 rounded mt-1.5"/>
                                    </td>
                                    <td class="admin-trash-status">
                                        <SkeletonBox class="h-3.5 w-14 rounded"/>
                                    </td>
                                    <td class="admin-trash-deleted">
                                        <SkeletonBox class="h-3.5 w-20 rounded"/>
                                    </td>
                                    <td class="admin-trash-left">
                                        <SkeletonBox class="h-1 w-full rounded-full"/>
                                    </td>
                                    <td class="admin-trash-actions">
                                        <SkeletonBox class="h-[30px] w-20 rounded-md ml-auto"/>
                                    </td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
}

/// 回收站全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn PostsTrashSkeleton() -> impl IntoView {
    view! {
        <div class="admin-trash" aria-hidden="true">
            <AdminPageHeadSkeleton
                title_width="w-20"
                subtitle_width="w-80 max-w-full"
                actions=|| view! { <SkeletonBox class="h-[34px] w-24 rounded-lg"/> }
            />
            <PostsTrashTableSkeleton/>
        </div>
    }
}
