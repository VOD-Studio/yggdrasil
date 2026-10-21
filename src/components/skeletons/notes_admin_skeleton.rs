//! 后台笔记骨架屏
//!
//! 与笔记管理同一版式：标题行、快速记录、类型选项卡与筛选、笔记行。

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
