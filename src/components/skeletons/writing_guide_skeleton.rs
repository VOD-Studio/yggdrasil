//! 写作指南页骨架屏。
//!
//! 镜像 Agent 写作指南页面 `/about/writing` 的英雄区、能力条、安装步骤面板、
//! 客户端配置卡片以及技能文档正文，避免数据加载期间出现空白或无结构的文本。
//! 动画由外层 DelayedSkeleton 统一提供。

use crate::components::skeletons::atoms::SkeletonBox;
use dioxus::prelude::*;

/// 写作指南页骨架屏组件。
#[component]
pub fn WritingGuideSkeleton() -> Element {
    rsx! {
        article {
            class: "writing-page",
            role: "status",
            aria_label: "正在加载写作指南",
            div { aria_hidden: "true",
                // 返回关于页链接占位
                div { class: "py-2 mb-1",
                    SkeletonBox { class: "h-3.5 w-20 rounded", animate: false }
                }

                // 英雄区：左侧标语与操作按钮，右侧徽标剪影
                header { class: "writing-hero",
                    div {
                        // Eyebrow 眉题占位
                        div { class: "flex items-center gap-2 mb-6",
                            span { class: "w-1.5 h-1.5 rounded-full bg-paper-tertiary/40 shrink-0" }
                            SkeletonBox { class: "h-3 w-40 rounded", animate: false }
                        }
                        // 主标题两行占位
                        SkeletonBox { class: "h-9 sm:h-11 w-64 max-w-full rounded mb-3", animate: false }
                        SkeletonBox { class: "h-9 sm:h-11 w-80 max-w-full rounded mb-6", animate: false }
                        // 导语两行占位
                        SkeletonBox { class: "h-4 w-96 max-w-full rounded mb-2", animate: false }
                        SkeletonBox { class: "h-4 w-72 max-w-full rounded mb-7", animate: false }
                        // 操作按钮（复制 Skill / 下载 SKILL.md）
                        div { class: "writing-actions",
                            SkeletonBox { class: "h-11 w-36 rounded-lg", animate: false }
                            SkeletonBox { class: "h-11 w-28 rounded-lg", animate: false }
                        }
                    }
                    // 徽标剪影（小屏下随 CSS 自动隐藏）
                    div { class: "writing-emblem opacity-40", aria_hidden: "true",
                        span { class: "writing-emblem-orbit" }
                        span { class: "writing-emblem-star", "✳" }
                        span { class: "writing-emblem-code", "{{ }}" }
                        span { class: "writing-emblem-caption", "WORDS → WORLDS" }
                    }
                }

                // 能力清单条（4项：代码、Mermaid、公式、MCP）
                div { class: "writing-capabilities",
                    for i in 0..4 {
                        span { key: "{i}",
                            SkeletonBox { class: "w-4 h-4 rounded shrink-0", animate: false }
                            SkeletonBox { class: "h-3.5 w-20 rounded", animate: false }
                        }
                    }
                }

                // 第一节：连接 Agent 与站点
                section { class: "writing-connect",
                    div { class: "writing-section-heading",
                        div {
                            SkeletonBox { class: "h-3 w-32 rounded mb-2", animate: false }
                            SkeletonBox { class: "h-7 w-64 max-w-full rounded", animate: false }
                        }
                        SkeletonBox { class: "h-4 w-24 rounded", animate: false }
                    }

                    // 一键安装卡片
                    section { class: "writing-install",
                        div { class: "writing-install-heading",
                            div { class: "flex items-center gap-2.5",
                                SkeletonBox { class: "w-7 h-7 rounded-full shrink-0", animate: false }
                                SkeletonBox { class: "h-5 w-44 rounded", animate: false }
                            }
                            SkeletonBox { class: "h-6 w-16 rounded-full", animate: false }
                        }
                        div { class: "my-3 space-y-2",
                            SkeletonBox { class: "h-3.5 w-3/4 max-w-full rounded", animate: false }
                        }
                        div { class: "writing-install-prompt space-y-2.5 my-3",
                            SkeletonBox { class: "h-3.5 w-full rounded", animate: false }
                            SkeletonBox { class: "h-3.5 w-4/5 rounded", animate: false }
                        }
                        div { class: "writing-install-footer",
                            SkeletonBox { class: "h-3 w-40 rounded", animate: false }
                            SkeletonBox { class: "h-9 w-28 rounded-lg", animate: false }
                        }
                    }

                    // 三步指引网格
                    div { class: "writing-steps",
                        for i in 0..3 {
                            div { key: "{i}", class: "space-y-2",
                                SkeletonBox { class: "h-6 w-7 rounded mb-1", animate: false }
                                SkeletonBox { class: "h-4 w-20 rounded", animate: false }
                                SkeletonBox { class: "h-3 w-full rounded", animate: false }
                                SkeletonBox { class: "h-3 w-4/5 rounded", animate: false }
                            }
                        }
                    }

                    // 客户端配置区块
                    div { class: "writing-config",
                        div { class: "writing-config-tabs",
                            div { class: "flex items-center gap-2",
                                SkeletonBox { class: "h-8 w-16 rounded-lg", animate: false }
                                SkeletonBox { class: "h-8 w-14 rounded-lg", animate: false }
                                SkeletonBox { class: "h-8 w-20 rounded-lg", animate: false }
                            }
                            SkeletonBox { class: "h-3 w-32 rounded ml-auto", animate: false }
                        }
                        div { class: "writing-config-meta",
                            SkeletonBox { class: "h-4 w-52 max-w-full rounded", animate: false }
                            SkeletonBox { class: "h-8 w-20 rounded-lg", animate: false }
                        }
                        div { class: "writing-config-code space-y-2.5",
                            SkeletonBox { class: "h-3.5 w-3/4 max-w-full rounded", animate: false }
                            SkeletonBox { class: "h-3.5 w-1/2 max-w-full rounded", animate: false }
                            SkeletonBox { class: "h-3.5 w-2/3 max-w-full rounded", animate: false }
                            SkeletonBox { class: "h-3.5 w-3/5 max-w-full rounded", animate: false }
                        }
                        div { class: "writing-config-footer",
                            SkeletonBox { class: "h-3 w-64 max-w-full rounded", animate: false }
                            SkeletonBox { class: "h-3 w-28 rounded", animate: false }
                        }
                    }
                }

                // 第二节：技能正文与目录
                section { class: "writing-document",
                    div { class: "writing-section-heading",
                        div {
                            SkeletonBox { class: "h-3 w-28 rounded mb-2", animate: false }
                            SkeletonBox { class: "h-7 w-48 max-w-full rounded", animate: false }
                        }
                        SkeletonBox { class: "h-6 w-16 rounded", animate: false }
                    }
                    div { class: "writing-toc py-3 flex items-center justify-between",
                        SkeletonBox { class: "h-4 w-28 rounded", animate: false }
                        SkeletonBox { class: "h-4 w-4 rounded", animate: false }
                    }
                    div { class: "space-y-4 my-8",
                        SkeletonBox { class: "h-4 w-full rounded", animate: false }
                        SkeletonBox { class: "h-4 w-11/12 rounded", animate: false }
                        SkeletonBox { class: "h-4 w-4/5 rounded", animate: false }
                        div { class: "h-2" }
                        SkeletonBox { class: "h-6 w-44 rounded mt-6 mb-3", animate: false }
                        SkeletonBox { class: "h-4 w-full rounded", animate: false }
                        SkeletonBox { class: "h-4 w-5/6 rounded", animate: false }
                        SkeletonBox { class: "h-4 w-3/4 rounded", animate: false }
                    }
                    div { class: "writing-document-footer",
                        SkeletonBox { class: "h-4 w-60 max-w-full rounded", animate: false }
                        SkeletonBox { class: "h-10 w-36 rounded-lg", animate: false }
                    }
                }
            }
        }
    }
}
