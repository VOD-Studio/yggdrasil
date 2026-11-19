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
            <table class="admin-table admin-post-table admin-trash-table">
                <thead>
                    <tr>
                        <th class="admin-table-check">
                            <SkeletonBox height="1rem" width="1rem" radius="0.25rem"/>
                        </th>
                        <th>
                            <SkeletonBox height="0.75rem" width="2.5rem" radius="0.25rem"/>
                        </th>
                        <th>
                            <SkeletonBox height="0.75rem" width="3rem" radius="0.25rem"/>
                        </th>
                        <th class="admin-trash-deleted">
                            <SkeletonBox height="0.75rem" width="3rem" radius="0.25rem"/>
                        </th>
                        <th class="admin-trash-left">
                            <SkeletonBox height="0.75rem" width="3.5rem" radius="0.25rem"/>
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
                                        <SkeletonBox height="1rem" width="1rem" radius="0.25rem"/>
                                    </td>
                                    <td class="admin-trash-title">
                                        <SkeletonBox height="1rem" width="calc(3 / 4 * 100%)" radius="0.25rem"/>
                                        <SkeletonBox height="0.75rem" width="calc(1 / 3 * 100%)" radius="0.25rem" style="--skeleton-margin-top: 0.375rem"/>
                                    </td>
                                    <td class="admin-trash-status">
                                        <SkeletonBox height="0.875rem" width="3.5rem" radius="0.25rem"/>
                                    </td>
                                    <td class="admin-trash-deleted">
                                        <SkeletonBox height="0.875rem" width="5rem" radius="0.25rem"/>
                                    </td>
                                    <td class="admin-trash-left">
                                        <SkeletonBox height="var(--spacing)" width="100%" radius="9999px"/>
                                    </td>
                                    <td class="admin-trash-actions">
                                        <SkeletonBox height="30px" width="5rem" radius="0.375rem" style="--skeleton-margin-left: auto"/>
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
                title_width="5rem"
                subtitle_width="20rem"
                actions=|| view! { <SkeletonBox height="34px" width="6rem" radius="0.5rem"/> }
            />
            <PostsTrashTableSkeleton/>
        </div>
    }
}
