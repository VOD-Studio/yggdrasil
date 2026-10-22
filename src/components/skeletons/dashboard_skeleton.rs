//! 后台仪表盘骨架屏
//!
//! 仅在 AdminLayout 的内容区展示，校验登录状态与路由挂起期间与仪表盘同一版式：
//! 标题行、四格数字、左栏柱图与近期文章、右栏评论与站点。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 仪表盘内容区骨架屏组件。
#[component]
pub fn AdminDashboardSkeleton() -> Element {
    rsx! {
        div { class: "admin-dash", aria_hidden: "true",
            AdminPageHeadSkeleton {
                title_width: "w-44",
                subtitle_width: "w-56",
            }
            div { class: "dash-tiles",
                for i in 0..4 {
                    div { key: "{i}", class: "admin-panel dash-tile",
                        SkeletonBox { class: "h-3.5 w-12 rounded" }
                        SkeletonBox { class: "dash-tile-value-skeleton" }
                        SkeletonBox { class: "h-3 w-24 rounded mt-2" }
                    }
                }
            }
            div { class: "dash-cols",
                div { class: "dash-col",
                    div { class: "admin-panel dash-chart",
                        SkeletonBox { class: "h-4 w-28 rounded" }
                        SkeletonBox { class: "dash-chart-skeleton" }
                    }
                    div { class: "admin-panel dash-list",
                        div { class: "dash-panel-head dash-panel-head-rule",
                            SkeletonBox { class: "h-4 w-20 rounded" }
                        }
                        div { class: "dash-list-body",
                            for i in 0..5 {
                                SkeletonBox { key: "{i}", class: "dash-row-skeleton" }
                            }
                        }
                    }
                }
                div { class: "dash-col",
                    div { class: "admin-panel dash-list",
                        div { class: "dash-panel-head dash-panel-head-rule",
                            SkeletonBox { class: "h-4 w-20 rounded" }
                        }
                        div { class: "dash-list-body",
                            for i in 0..4 {
                                SkeletonBox { key: "{i}", class: "dash-comment-skeleton" }
                            }
                        }
                    }
                }
            }
        }
    }
}
