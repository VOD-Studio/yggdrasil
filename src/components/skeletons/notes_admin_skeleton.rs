//! 后台笔记骨架屏
//!
//! 与笔记管理同一版式：标题行、快速记录、类型选项卡与筛选、笔记行；
//! 笔记本管理为左栏列表与右栏表单。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 笔记行骨架（列表加载中使用）。
#[component]
pub fn NoteRowsSkeleton() -> Element {
    rsx! {
        div { class: "admin-panel admin-notes-list", aria_hidden: "true",
            ul {
                for i in 0..6 {
                    li { key: "{i}", class: "admin-notes-row",
                        SkeletonBox { class: "h-[30px] w-[30px] rounded-lg" }
                        span { class: "admin-notes-main",
                            SkeletonBox { class: "h-4 w-3/5 rounded" }
                            SkeletonBox { class: "h-3 w-2/5 rounded mt-2" }
                        }
                        SkeletonBox { class: "h-3 w-10 rounded" }
                    }
                }
            }
        }
    }
}

/// 笔记管理全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn AdminNotesSkeleton() -> Element {
    rsx! {
        div { class: "admin-notes", aria_hidden: "true",
            div { class: "admin-page-head",
                div {
                    SkeletonBox { class: "h-7 w-20 rounded-md" }
                    SkeletonBox { class: "h-3.5 w-64 rounded mt-2" }
                }
                div { class: "admin-page-actions",
                    SkeletonBox { class: "h-[34px] w-24 rounded-lg" }
                    SkeletonBox { class: "h-[34px] w-24 rounded-lg" }
                }
            }
            SkeletonBox { class: "h-[120px] w-full rounded-[20px]" }
            div { class: "admin-toolbar admin-notes-toolbar",
                SkeletonBox { class: "h-8 w-56 rounded-full" }
                div { class: "admin-toolbar-filters",
                    SkeletonBox { class: "h-[30px] w-60 rounded-lg" }
                    SkeletonBox { class: "h-[30px] w-24 rounded-lg" }
                }
            }
            NoteRowsSkeleton {}
        }
    }
}

/// 加载占位：左栏列表与右栏表单。
#[component]
pub fn NotebooksSkeleton() -> Element {
    rsx! {
        div { class: "admin-notebooks-grid", aria_hidden: "true",
            div { class: "admin-panel admin-notebooks-list",
                div { class: "admin-notebooks-tabs", SkeletonBox { class: "h-[30px] w-full rounded-lg" } }
                for i in 0..4 {
                    div { key: "{i}", class: "px-4 py-2.5",
                        SkeletonBox { class: "h-4 w-3/4 rounded" }
                        SkeletonBox { class: "h-3 w-1/3 rounded mt-1.5" }
                    }
                }
            }
            div { class: "admin-panel admin-notebook-editor",
                div { class: "admin-notebook-fields",
                    SkeletonBox { class: "h-5 w-24 rounded" }
                    SkeletonBox { class: "h-9 w-full rounded-lg" }
                    SkeletonBox { class: "h-16 w-full rounded-lg" }
                }
            }
        }
    }
}

/// 笔记本管理全页骨架屏（AdminLayout 路由级 fallback）。
#[component]
pub fn AdminNotebooksSkeleton() -> Element {
    rsx! {
        div { class: "admin-notebooks", aria_hidden: "true",
            div { class: "admin-page-head",
                div {
                    SkeletonBox { class: "h-7 w-24 rounded-md" }
                    SkeletonBox { class: "h-3.5 w-64 rounded mt-2" }
                }
                SkeletonBox { class: "h-[34px] w-28 rounded-lg" }
            }
            NotebooksSkeleton {}
        }
    }
}
