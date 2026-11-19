//! 后台文章列表骨架屏
//!
//! 与文章列表同一版式：标题行、状态选项卡与筛选、表格与底栏分页。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 文章表格骨架屏（列表加载中使用）。
#[component]
pub fn PostsTableSkeleton() -> impl IntoView {
    view! {
        <div class="admin-panel admin-table-wrap" aria-hidden="true">
            <table class="admin-table admin-posts-table">
                <thead>
                    <tr>
                        <th class="admin-table-check">
                            <SkeletonBox height="1rem" width="1rem" radius="0.25rem"/>
                        </th>
                        <th>
                            <SkeletonBox height="0.75rem" width="2.5rem" radius="0.25rem"/>
                        </th>
                        <th>
                            <SkeletonBox height="0.75rem" width="2.5rem" radius="0.25rem"/>
                        </th>
                        <th class="admin-posts-tags">
                            <SkeletonBox height="0.75rem" width="2.5rem" radius="0.25rem"/>
                        </th>
                        <th class="admin-posts-words">
                            <SkeletonBox height="0.75rem" width="2rem" radius="0.25rem" style="--skeleton-margin-left: auto"/>
                        </th>
                        <th>
                            <SkeletonBox height="0.75rem" width="3.5rem" radius="0.25rem"/>
                        </th>
                        <th class="admin-table-actions"></th>
                    </tr>
                </thead>
                <tbody>
                    {(0..8)
                        .map(|_| {
                            view! {
                                <tr>
                                    <td class="admin-table-check">
                                        <SkeletonBox height="1rem" width="1rem" radius="0.25rem"/>
                                    </td>
                                    <td class="admin-posts-title">
                                        <SkeletonBox height="1rem" width="calc(3 / 4 * 100%)" radius="0.25rem"/>
                                        <SkeletonBox height="0.75rem" width="calc(1 / 3 * 100%)" radius="0.25rem" style="--skeleton-margin-top: 0.375rem"/>
                                    </td>
                                    <td class="admin-posts-status">
                                        <SkeletonBox height="0.875rem" width="3.5rem" radius="0.25rem"/>
                                    </td>
                                    <td class="admin-posts-tags">
                                        <SkeletonBox height="0.875rem" width="5rem" radius="0.25rem"/>
                                    </td>
                                    <td class="admin-posts-words">
                                        <SkeletonBox height="0.875rem" width="2.5rem" radius="0.25rem" style="--skeleton-margin-left: auto"/>
                                    </td>
                                    <td class="admin-posts-date">
                                        <SkeletonBox height="0.875rem" width="7rem" radius="0.25rem"/>
                                    </td>
                                    <td class="admin-table-actions">
                                        <SkeletonBox height="1.5rem" width="1.5rem" radius="0.375rem" style="--skeleton-margin-left: auto"/>
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

/// 文章列表全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn PostsSkeleton() -> impl IntoView {
    view! {
        <div class="admin-posts" aria-hidden="true">
            <AdminPageHeadSkeleton
                title_width="5rem"
                subtitle_width="11rem"
                actions=|| {
                    view! {
                        <SkeletonBox height="34px" width="34px" radius="0.5rem"/>
                        <SkeletonBox height="34px" width="7rem" radius="0.5rem"/>
                    }
                }
            />
            <div class="admin-toolbar">
                <SkeletonBox height="2rem" width="18rem" radius="9999px"/>
                <div class="admin-toolbar-filters">
                    <SkeletonBox height="30px" width="15rem" radius="0.5rem"/>
                    <SkeletonBox height="30px" width="7rem" radius="0.5rem"/>
                </div>
            </div>
            <PostsTableSkeleton/>
        </div>
    }
}
