//! 文章编辑器页骨架屏
//!
//! 在写文章/编辑文章页面加载时展示，镜像 Write 页面的左右两栏结构：
//! 左栏(标题+编辑器) + 右栏(链接/标签/摘要/封面) + 底部操作栏。

use crate::components::skeletons::atoms::*;
use dioxus::prelude::*;

/// 文章编辑器页骨架屏组件。
///
/// 镜像 Write 页面的左右两栏布局：左栏主写作区(标题、编辑器)，
/// 右栏侧边栏(链接、标签、摘要、封面图)，底部贴底操作栏。
#[component]
pub fn WriteSkeleton() -> Element {
    rsx! {
        // 根:父层是 flex 容器(write.rs 覆盖层 / admin_layout 包裹层均为 flex flex-col),
        // 用 flex-1 撑满父层高度(比 height:100% 更可靠,不依赖父显式 height)。
        div { class: "relative flex-1 flex flex-col min-h-0 overflow-hidden bg-[var(--color-paper-theme)]",
            // 顶部工具条骨架
            div { class: "flex-shrink-0 px-6 py-3 border-b border-[var(--color-paper-border)]/60 flex items-center justify-between gap-4 select-none",
                div { class: "flex items-center gap-3",
                    SkeletonBox { class: "h-7 w-20 rounded-full" }
                    div { class: "w-px h-4 bg-[var(--color-paper-border)]/60" }
                    SkeletonBox { class: "h-5 w-24 rounded-md" }
                    SkeletonBox { class: "h-4 w-10 rounded-full" }
                }
                div { class: "hidden md:flex items-center gap-3",
                    SkeletonBox { class: "h-4 w-28 rounded" }
                }
                SkeletonBox { class: "h-7 w-20 rounded-full" }
            }

            // 两栏容器：与真实页面一致，左 flex-1 + 右 w-80
            div { class: "flex-1 min-h-0 flex",
                // 左栏 (主写作区)
                div { class: "flex-1 min-w-0 min-h-0 overflow-y-auto px-6 sm:px-10 md:px-14 py-8 flex flex-col items-center",
                    div { class: "w-full max-w-4xl flex-1 flex flex-col",
                        // 标题输入骨架
                        SkeletonBox { class: "h-12 w-3/4 rounded-xl mb-4" }

                        // 编辑器区域骨架
                        div { class: "flex-1 min-h-[480px] flex flex-col mb-4",
                            SkeletonCard { class: Some("flex-1 min-h-0 w-full overflow-hidden p-6 space-y-4 shadow-xs"),
                                SkeletonBox { class: "h-5 w-[90%] rounded-md" }
                                SkeletonBox { class: "h-5 w-full rounded-md" }
                                SkeletonBox { class: "h-5 w-[85%] rounded-md" }
                                SkeletonBox { class: "h-5 w-[95%] rounded-md" }
                                SkeletonBox { class: "h-5 w-[60%] rounded-md" }
                                SkeletonBox { class: "h-5 w-full rounded-md" }
                                SkeletonBox { class: "h-5 w-[75%] rounded-md" }
                            }
                        }
                    }
                }

                // 右栏 (侧边栏)
                div { class: "w-80 sm:w-88 flex-shrink-0 min-h-0 overflow-y-auto border-l border-[var(--color-paper-border)]/70 flex flex-col bg-[var(--color-paper-theme)]",
                    // 侧栏标题栏
                    div { class: "px-5 py-4 border-b border-[var(--color-paper-border)]/60 flex items-center justify-between",
                        SkeletonBox { class: "h-4 w-20 rounded" }
                        SkeletonBox { class: "h-4 w-4 rounded" }
                    }
                    // 链接节
                    div { class: "p-5 border-b border-[var(--color-paper-border)]/60 space-y-2.5",
                        SkeletonBox { class: "h-3 w-16 rounded" }
                        SkeletonBox { class: "h-8 w-full rounded-2xl" }
                    }
                    // 标签节
                    div { class: "p-5 border-b border-[var(--color-paper-border)]/60 space-y-2.5",
                        SkeletonBox { class: "h-3 w-16 rounded" }
                        SkeletonBox { class: "h-8 w-full rounded-2xl" }
                    }
                    // 摘要节
                    div { class: "p-5 border-b border-[var(--color-paper-border)]/60 space-y-2.5",
                        SkeletonBox { class: "h-3 w-16 rounded" }
                        SkeletonBox { class: "h-20 w-full rounded-2xl" }
                    }
                    // 封面图节
                    div { class: "p-5 space-y-2.5",
                        SkeletonBox { class: "h-3 w-16 rounded" }
                        SkeletonBox { class: "h-14 w-full rounded-2xl" }
                    }
                }
            }

            // 底部操作栏
            div { class: "flex-shrink-0 px-6 py-3.5 flex items-center justify-between border-t border-[var(--color-paper-border)]/80 bg-[var(--color-paper-theme)] shadow-xs",
                SkeletonBox { class: "h-9 w-24 rounded-full" }
                div { class: "flex items-center gap-3",
                    SkeletonBox { class: "h-9 w-28 rounded-full" }
                    div { class: "w-px h-5 bg-[var(--color-paper-border)]/60" }
                    SkeletonBox { class: "h-9 w-24 rounded-full" }
                }
            }
        }
    }
}
