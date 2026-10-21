//! 后台设置骨架屏
//!
//! 与设置页同一版式：左侧分区目录、右侧标题与分区卡片。根节点沿用页面的内部滚动
//! 容器类，撑满主内容区高度。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 后台设置骨架屏组件。
#[component]
pub fn SettingsAdminSkeleton() -> Element {
    rsx! {
        div { class: "settings-page", aria_hidden: "true",
            div { class: "settings-layout",
                div { class: "settings-nav",
                    for i in 0..9 {
                        SkeletonBox { key: "{i}", class: "h-[30px] w-full rounded-[7px]" }
                    }
                }
                div { class: "settings-main",
                    div { class: "admin-page-head",
                        div {
                            SkeletonBox { class: "h-7 w-16 rounded-md" }
                            SkeletonBox { class: "h-3.5 w-80 rounded mt-2" }
                        }
                    }
                    for i in 0..3 {
                        div { key: "{i}", class: "settings-slot",
                            div { class: "admin-panel settings-card",
                                div { class: "settings-card-head",
                                    div {
                                        SkeletonBox { class: "h-4 w-20 rounded" }
                                        SkeletonBox { class: "h-3 w-64 rounded mt-2" }
                                    }
                                }
                                div { class: "settings-rows",
                                    for j in 0..2 {
                                        div { key: "{j}", class: "settings-row",
                                            SkeletonBox { class: "h-3.5 w-20 rounded" }
                                            SkeletonBox { class: "h-9 w-full rounded-lg" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
