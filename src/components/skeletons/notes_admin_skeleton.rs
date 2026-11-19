//! 后台笔记骨架屏
//!
//! 与笔记管理同一版式：标题行、快速记录、类型选项卡与筛选、笔记行；
//! 笔记本管理为左栏列表与右栏表单。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 笔记行骨架（列表加载中使用）。
#[component]
pub fn NoteRowsSkeleton() -> impl IntoView {
    view! {
        <div class="admin-panel admin-notes-list" aria-hidden="true">
            <ul>
                {(0..6)
                    .map(|_| {
                        view! {
                            <li class="admin-notes-row">
                                <SkeletonBox height="30px" width="30px" radius="0.5rem"/>
                                <span class="admin-notes-main">
                                    <SkeletonBox height="1rem" width="calc(3 / 5 * 100%)" radius="0.25rem"/>
                                    <SkeletonBox height="0.75rem" width="calc(2 / 5 * 100%)" radius="0.25rem" style="--skeleton-margin-top: 0.5rem"/>
                                </span>
                                <SkeletonBox height="0.75rem" width="2.5rem" radius="0.25rem"/>
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
        </div>
    }
}

/// 笔记管理全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn AdminNotesSkeleton() -> impl IntoView {
    view! {
        <div class="admin-notes" aria-hidden="true">
            <AdminPageHeadSkeleton
                title_width="5rem"
                subtitle_width="16rem"
                actions=|| {
                    view! {
                        <SkeletonBox height="34px" width="6rem" radius="0.5rem"/>
                        <SkeletonBox height="34px" width="6rem" radius="0.5rem"/>
                    }
                }
            />
            <SkeletonBox height="120px" width="100%" radius="20px"/>
            <div class="admin-toolbar admin-notes-toolbar">
                <SkeletonBox height="2rem" width="14rem" radius="9999px"/>
                <div class="admin-toolbar-filters">
                    <SkeletonBox height="30px" width="15rem" radius="0.5rem"/>
                    <SkeletonBox height="30px" width="6rem" radius="0.5rem"/>
                </div>
            </div>
            <NoteRowsSkeleton/>
        </div>
    }
}

/// 加载占位：左栏列表与右栏表单。
#[component]
pub fn NotebooksSkeleton() -> impl IntoView {
    view! {
        <div class="admin-notebooks-grid" aria-hidden="true">
            <div class="admin-panel admin-notebooks-list">
                <div class="admin-notebooks-tabs">
                    <SkeletonBox height="30px" width="100%" radius="0.5rem"/>
                </div>
                {(0..4)
                    .map(|_| {
                        view! {
                            <div class="note-row-skeleton-copy">
                                <SkeletonBox height="1rem" width="calc(3 / 4 * 100%)" radius="0.25rem"/>
                                <SkeletonBox height="0.75rem" width="calc(1 / 3 * 100%)" radius="0.25rem" style="--skeleton-margin-top: 0.375rem"/>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
            <div class="admin-panel admin-notebook-editor">
                <div class="admin-notebook-fields">
                    <SkeletonBox height="1.25rem" width="6rem" radius="0.25rem"/>
                    <SkeletonBox height="2.25rem" width="100%" radius="0.5rem"/>
                    <SkeletonBox height="4rem" width="100%" radius="0.5rem"/>
                </div>
            </div>
        </div>
    }
}

/// 笔记本管理全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn AdminNotebooksSkeleton() -> impl IntoView {
    view! {
        <div class="admin-notebooks" aria-hidden="true">
            <AdminPageHeadSkeleton
                title_width="6rem"
                subtitle_width="16rem"
                actions=|| view! { <SkeletonBox height="34px" width="7rem" radius="0.5rem"/> }
            />
            <NotebooksSkeleton/>
        </div>
    }
}
