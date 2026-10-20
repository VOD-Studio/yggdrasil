//! 更新日志页骨架屏：页头照常显示，版本目录与版本分节占位。

use crate::components::skeletons::atoms::SkeletonBox;
use crate::pages::changelog::ChangelogHeader;
use dioxus::prelude::*;

/// 更新日志加载占位：镜像有界版本目录和小屏横向版本条。
#[component]
pub fn ChangelogSkeleton() -> Element {
    rsx! {
        div { class: "changelog",
            ChangelogHeader {}
            div { class: "changelog-layout", role: "status", aria_label: "正在加载更新日志",
                div { class: "changelog-nav", aria_hidden: "true",
                    div { class: "changelog-nav-panel",
                        div { class: "changelog-nav-head",
                            SkeletonBox { class: "h-3 w-8" }
                        }
                        div { class: "changelog-nav-scroll",
                            div { class: "changelog-nav-items",
                                for i in 0..6 {
                                    div { key: "{i}", class: "changelog-nav-link",
                                        SkeletonBox { class: "h-3 w-12" }
                                        SkeletonBox { class: "h-3 w-8" }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "changelog-releases", aria_hidden: "true",
                    for i in 0..2 {
                        div { key: "{i}", class: "changelog-version",
                            SkeletonBox { class: "h-6 w-24" }
                            for group in 0..2 {
                                div { key: "{group}", class: "changelog-group",
                                    SkeletonBox { class: "h-5 w-12 rounded-full" }
                                    div { class: "flex flex-col gap-3 mt-4",
                                        SkeletonBox { class: "h-3.5 w-full" }
                                        SkeletonBox { class: "h-3.5 w-5/6" }
                                        SkeletonBox { class: "h-3.5 w-3/4" }
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
