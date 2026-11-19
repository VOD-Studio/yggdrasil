//! 与搜索结果行对齐的骨架屏，也用于搜索路由的加载占位。

use leptos::prelude::*;

use crate::components::skeletons::atoms::SkeletonBox;

/// 结果标题的宽度，错落排布更像真实结果。
const TITLE_WIDTHS: [&str; 4] = ["width: 62%", "width: 78%", "width: 54%", "width: 70%"];

#[component]
pub fn SearchSkeleton() -> impl IntoView {
    view! {
        <div class="search-skeleton" role="status" aria-label="正在搜索">
            <div aria-hidden="true">
                <SkeletonBox height="1rem" width="9rem" style="--skeleton-margin-bottom: 1rem"/>
                {TITLE_WIDTHS
                    .iter()
                    .map(|width| {
                        view! {
                            <div class="search-result search-result-skeleton">
                                <SkeletonBox height="1rem" style=*width/>
                                <SkeletonBox height="0.75rem" width="100%" style="--skeleton-margin-top: 0.75rem"/>
                                <SkeletonBox height="0.75rem" width="calc(4 / 5 * 100%)" style="--skeleton-margin-top: 0.5rem"/>
                                <SkeletonBox height="0.75rem" width="10rem" style="--skeleton-margin-top: 0.75rem"/>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}
