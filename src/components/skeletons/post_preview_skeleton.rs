//! 草稿/文章预览页骨架屏
//!
//! 镜像 `/admin/preview/:slug`（`crate::pages::admin::preview` 的 `PostPreview`）的真实结构。
//! 与公开文章详情页骨架屏（[`crate::components::skeletons::post_detail_skeleton::PostDetailSkeleton`]）的唯一差异：顶部多一组预览横幅占位
//! （浮起的预览条），其余标题/摘要/元信息/封面/正文/页脚占位一致。
//!
//! 同时服务于两处加载态，保证视觉连续：
//! - [`crate::components::admin_layout`] 登录态校验期（避免落入默认的仪表盘骨架屏）。
//! - `crate::pages::admin::preview` 的 `PostPreview` 取数 pending 期。

use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::post_detail_skeleton::PostDetailBody;
use leptos::prelude::*;

/// 草稿/文章预览页骨架屏组件。
///
/// 结构（与真实页面逐段对应）：预览横幅 + 与 [`PostDetailSkeleton`] 共享的正文
/// （面包屑/标题/摘要/元信息/封面图/正文(多段)/页脚占位，见 [`PostDetailBody`]）。
///
/// [`PostDetailSkeleton`]: crate::components::skeletons::post_detail_skeleton::PostDetailSkeleton
#[component]
pub fn PostPreviewSkeleton() -> impl IntoView {
    // 结构与 preview.rs 一致：post-preview > 预览条 + preview-frame(max-width 680) > article。
    // 之前骨架直接把 article 铺满后台主画布，比真实正文宽得多，加载完成时宽度突变。
    // w-full：admin_layout 登录校验期会把骨架屏挂进 div.flex.flex-col 包裹层，
    // 无显式宽度时 flex item 会按内容收缩；其他后台骨架屏根节点同理用 w-full。
    view! {
        <div class="post-preview w-full">
            // 预览条占位：与 preview.rs 的浮起预览条同位置、同高度。
            <div class="preview-bar-slot">
                <SkeletonBox class="h-11 w-[34rem] max-w-full rounded-xl"/>
            </div>

            <div class="preview-frame">
                <article class="post-single">
                    <PostDetailBody/>
                </article>
            </div>
        </div>
    }
}
