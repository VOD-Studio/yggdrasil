//! 后台回收站骨架屏
//!
//! 镜像后台 PostsTrash 页面的结构：Header（标题+副标题）+ 自动清理配置卡片 + 表格。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::ADMIN_CARD_CLASS;
/// 后台回收站骨架屏组件。
#[component]
pub fn PostsTrashSkeleton() -> Element {
    rsx! {
        div { class: "w-full max-w-7xl mx-auto space-y-6",
            // 页头：标题与副标题 + 按钮
            div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70",
                div { class: "space-y-1.5",
                    SkeletonBox { class: "h-9 w-28 rounded-lg" }
                    SkeletonBox { class: "h-4 w-60 rounded" }
                }
                div { class: "flex items-center gap-3",
                    SkeletonBox { class: "h-9 w-28 rounded-full" }
                    SkeletonBox { class: "h-9 w-28 rounded-full" }
                }
            }

            div { class: "space-y-6",
                // 自动清理配置卡片占位
                div { class: "{ADMIN_CARD_CLASS} p-6 space-y-4",
                    div { class: "flex justify-between items-center",
                        div { class: "space-y-1",
                            SkeletonBox { class: "h-5 w-32 rounded" }
                            SkeletonBox { class: "h-3.5 w-64 rounded" }
                        }
                        SkeletonBox { class: "h-6 w-12 rounded-full" }
                    }
                }

                // 回收站表格
                div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 overflow-hidden",
                    table { class: "w-full text-sm",
                        thead {
                            tr { class: "bg-[var(--color-paper-entry)]/80 border-b border-[var(--color-paper-border)]/70",
                                th { class: "px-4 py-3.5 w-10 text-center",
                                    SkeletonBox { class: "h-4 w-4 rounded mx-auto" }
                                }
                                th { class: "px-5 py-3.5",
                                    SkeletonBox { class: "h-3 w-16" }
                                }
                                th { class: "px-4 py-3.5 w-24",
                                    SkeletonBox { class: "h-3 w-14 mx-auto" }
                                }
                                th { class: "px-4 py-3.5 w-32",
                                    SkeletonBox { class: "h-3 w-16" }
                                }
                                th { class: "px-4 py-3.5 w-24",
                                    SkeletonBox { class: "h-3 w-14 mx-auto" }
                                }
                                th { class: "px-5 py-3.5 w-36",
                                    SkeletonBox { class: "h-3 w-16 ml-auto" }
                                }
                            }
                        }
                        tbody {
                            for _ in 0..8 {
                                tr { class: "border-b border-[var(--color-paper-border)]/60 last:border-0",
                                    td { class: "px-4 py-3.5 text-center",
                                        SkeletonBox { class: "h-4 w-4 rounded mx-auto" }
                                    }
                                    td { class: "px-5 py-3.5",
                                        div { class: "space-y-1.5",
                                            SkeletonBox { class: "h-4 w-1/3" }
                                            SkeletonBox { class: "h-3 w-1/4" }
                                        }
                                    }
                                    td { class: "px-4 py-3.5",
                                        SkeletonBox { class: "h-5 w-14 mx-auto rounded-full" }
                                    }
                                    td { class: "px-4 py-3.5",
                                        SkeletonBox { class: "h-4 w-20" }
                                    }
                                    td { class: "px-4 py-3.5",
                                        SkeletonBox { class: "h-5 w-14 mx-auto rounded-full" }
                                    }
                                    td { class: "px-5 py-3.5",
                                        SkeletonBox { class: "h-6 w-24 ml-auto rounded" }
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
