//! 更新日志页骨架屏：页头照常显示，版本目录与版本分节占位。

use crate::components::skeletons::atoms::SkeletonBox;
use crate::pages::changelog::ChangelogHeader;
use leptos::prelude::*;

/// 更新日志加载占位：镜像有界版本目录和小屏横向版本条。
#[component]
pub fn ChangelogSkeleton() -> impl IntoView {
    view! {
        <div class="changelog">
            <ChangelogHeader/>
            <div class="changelog-layout" role="status" aria-label="正在加载更新日志">
                <div class="changelog-nav" aria-hidden="true">
                    <div class="changelog-nav-panel">
                        <div class="changelog-nav-head">
                            <SkeletonBox height="0.75rem" width="2rem"/>
                        </div>
                        <div class="changelog-nav-scroll">
                            <div class="changelog-nav-items">
                                {(0..6)
                                    .map(|_| {
                                        view! {
                                            <div class="changelog-nav-link">
                                                <SkeletonBox height="0.75rem" width="3rem"/>
                                                <SkeletonBox height="0.75rem" width="2rem"/>
                                            </div>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        </div>
                    </div>
                </div>
                <div class="changelog-releases" aria-hidden="true">
                    {(0..2)
                        .map(|_| {
                            view! {
                                <div class="changelog-version">
                                    <SkeletonBox height="1.5rem" width="6rem"/>
                                    {(0..2)
                                        .map(|_| {
                                            view! {
                                                <div class="changelog-group">
                                                    <SkeletonBox height="1.25rem" width="3rem" radius="9999px"/>
                                                    <div class="changelog-card-skeleton-lines">
                                                        <SkeletonBox height="0.875rem" width="100%"/>
                                                        <SkeletonBox height="0.875rem" width="calc(5 / 6 * 100%)"/>
                                                        <SkeletonBox height="0.875rem" width="calc(3 / 4 * 100%)"/>
                                                    </div>
                                                </div>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
        </div>
    }
}
