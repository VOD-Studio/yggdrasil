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
                                <SkeletonBox class="h-[30px] w-[30px] rounded-lg"/>
                                <span class="admin-notes-main">
                                    <SkeletonBox class="h-4 w-3/5 rounded"/>
                                    <SkeletonBox class="h-3 w-2/5 rounded mt-2"/>
                                </span>
                                <SkeletonBox class="h-3 w-10 rounded"/>
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
                title_width="w-20"
                subtitle_width="w-64"
                actions=|| {
                    view! {
                        <SkeletonBox class="h-[34px] w-24 rounded-lg"/>
                        <SkeletonBox class="h-[34px] w-24 rounded-lg"/>
                    }
                }
            />
            <SkeletonBox class="h-[120px] w-full rounded-[20px]"/>
            <div class="admin-toolbar admin-notes-toolbar">
                <SkeletonBox class="h-8 w-56 rounded-full"/>
                <div class="admin-toolbar-filters">
                    <SkeletonBox class="h-[30px] w-60 rounded-lg"/>
                    <SkeletonBox class="h-[30px] w-24 rounded-lg"/>
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
                    <SkeletonBox class="h-[30px] w-full rounded-lg"/>
                </div>
                {(0..4)
                    .map(|_| {
                        view! {
                            <div class="px-4 py-2.5">
                                <SkeletonBox class="h-4 w-3/4 rounded"/>
                                <SkeletonBox class="h-3 w-1/3 rounded mt-1.5"/>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
            <div class="admin-panel admin-notebook-editor">
                <div class="admin-notebook-fields">
                    <SkeletonBox class="h-5 w-24 rounded"/>
                    <SkeletonBox class="h-9 w-full rounded-lg"/>
                    <SkeletonBox class="h-16 w-full rounded-lg"/>
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
                title_width="w-24"
                subtitle_width="w-64"
                actions=|| view! { <SkeletonBox class="h-[34px] w-28 rounded-lg"/> }
            />
            <NotebooksSkeleton/>
        </div>
    }
}
