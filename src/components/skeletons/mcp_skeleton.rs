//! 后台 MCP 接入骨架屏
//!
//! 与 MCP 页同一版式：标题行、一句话接入卡片与令牌表格。

use leptos::prelude::*;

use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};

/// 后台 MCP 接入骨架屏组件。
#[component]
pub fn McpSkeleton() -> impl IntoView {
    view! {
        <div class="admin-mcp" aria-hidden="true">
            <AdminPageHeadSkeleton
                title_width="7rem"
                subtitle_width="24rem"
                actions=|| view! { <SkeletonBox height="34px" width="7rem" radius="0.5rem"/> }
            />
            <div class="admin-panel mcp-oneline">
                <SkeletonBox height="1rem" width="6rem" radius="0.25rem"/>
                <SkeletonBox height="3rem" width="100%" radius="0.5rem" style="--skeleton-margin-top: 0.75rem"/>
                <SkeletonBox height="2rem" width="12rem" radius="0.5rem" style="--skeleton-margin-top: 0.75rem"/>
            </div>
            <div class="mcp-tokens">
                <SkeletonBox height="1rem" width="7rem" radius="0.25rem" style="--skeleton-margin-bottom: 0.75rem"/>
                <div class="admin-panel">
                    {(0..3)
                        .map(|_| {
                            view! {
                                <div class="mcp-skeleton-row">
                                    <SkeletonBox height="1rem" width="10rem" radius="0.25rem"/>
                                    <SkeletonBox height="1rem" width="4rem" radius="0.25rem"/>
                                    <SkeletonBox height="1rem" width="6rem" radius="0.25rem"/>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
        </div>
    }
}
