//! 写作指南页骨架屏：页头照常显示，三步与配置卡片占位。

use crate::components::skeletons::atoms::SkeletonBox;
use crate::pages::writing_guide::WritingHeader;
use leptos::prelude::*;

/// 写作指南页骨架屏组件。
#[component]
pub fn WritingGuideSkeleton() -> impl IntoView {
    view! {
        <article class="writing-page">
            <WritingHeader/>
            <div role="status" aria-label="正在加载写作指南">
                <div class="writing-steps" aria-hidden="true">
                    {(0..2)
                        .map(|_| {
                            view! {
                                <div class="writing-step">
                                    <div class="writing-step-rail">
                                        <SkeletonBox height="1.75rem" width="1.75rem" radius="9999px"/>
                                    </div>
                                    <div class="writing-step-body">
                                        <SkeletonBox height="1rem" width="7rem"/>
                                        <SkeletonBox height="0.75rem" width="100%" style="--skeleton-margin-top: 0.75rem"/>
                                        <SkeletonBox height="7rem" width="100%" radius="14px" style="--skeleton-margin-top: 1rem"/>
                                    </div>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
        </article>
    }
}
