//! 写作指南页骨架屏：页头照常显示，三步与配置卡片占位。

use crate::components::skeletons::atoms::SkeletonBox;
use crate::pages::writing_guide::WritingHeader;
use dioxus::prelude::*;

/// 写作指南页骨架屏组件。
#[component]
pub fn WritingGuideSkeleton() -> Element {
    rsx! {
        article { class: "writing-page",
            WritingHeader {}
            div { role: "status", aria_label: "正在加载写作指南",
                div { class: "writing-steps", aria_hidden: "true",
                    for i in 0..2 {
                        div { key: "{i}", class: "writing-step",
                            div { class: "writing-step-rail",
                                SkeletonBox { class: "h-7 w-7 rounded-full" }
                            }
                            div { class: "writing-step-body",
                                SkeletonBox { class: "h-4 w-28" }
                                SkeletonBox { class: "h-3 mt-3 w-full" }
                                SkeletonBox { class: "h-28 mt-4 w-full rounded-[14px]" }
                            }
                        }
                    }
                }
            }
        }
    }
}
