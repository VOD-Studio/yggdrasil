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
                            <SkeletonBox class="h-3 w-8"/>
                        </div>
                        <div class="changelog-nav-scroll">
                            <div class="changelog-nav-items">
                                {(0..6)
                                    .map(|_| {
                                        view! {
                                            <div class="changelog-nav-link">
                                                <SkeletonBox class="h-3 w-12"/>
                                                <SkeletonBox class="h-3 w-8"/>
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
                                    <SkeletonBox class="h-6 w-24"/>
                                    {(0..2)
                                        .map(|_| {
                                            view! {
                                                <div class="changelog-group">
                                                    <SkeletonBox class="h-5 w-12 rounded-full"/>
                                                    <div class="flex flex-col gap-3 mt-4">
                                                        <SkeletonBox class="h-3.5 w-full"/>
                                                        <SkeletonBox class="h-3.5 w-5/6"/>
                                                        <SkeletonBox class="h-3.5 w-3/4"/>
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
