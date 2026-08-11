//! 草稿/文章预览页骨架屏
//!
//! 镜像 `/admin/preview/:slug`（[`crate::pages::admin::preview::PostPreview`]）的真实结构。
//! 与公开文章详情页骨架屏（[`crate::components::skeletons::post_detail_skeleton::PostDetailSkeleton`]）的唯一差异：顶部多一组预览横幅占位
//! （状态徽章 + 继续编辑 / 返回列表按钮），其余标题/摘要/元信息/封面/正文/页脚占位一致。
//!
//! 同时服务于两处加载态，保证视觉连续：
//! - [`crate::components::admin_layout`] 登录态校验期（避免落入默认的仪表盘骨架屏）。
//! - [`crate::pages::admin::preview::PostPreview`] 内 `use_server_future` 取数 pending 期。

use crate::components::skeletons::atoms::SkeletonBox;
use dioxus::prelude::*;

/// 草稿/文章预览页骨架屏组件。
///
/// 结构（与真实页面逐段对应）：
/// 预览横幅 + 面包屑 + 标题(h1) + 摘要 + 元信息 + 封面图 + 正文(多段) + 页脚占位。
#[component]
pub fn PostPreviewSkeleton() -> Element {
    rsx! {
        // w-full：admin_layout 登录校验期会把骨架屏挂进 div.flex.flex-col 包裹层，
        // 此时 article 作为 flex item，其 margin:auto(来自 .post-single)会禁用 stretch、
        // 无显式宽度则 shrink-to-fit 收缩变窄。w-full 强制撑满交叉轴，与真实文章
        // (直接挂 main block 下、自然满宽)几何对齐。其他后台骨架屏根节点同理用 w-full。
        article { class: "post-single w-full",
            // 预览横幅占位：左侧「预览模式」文字 + 状态徽章，右侧继续编辑 / 返回列表两枚按钮。
            // 类名与 preview.rs 真实横幅一致，确保占位与真实元素几何对齐。
            div { class: "flex flex-wrap items-center justify-between gap-3 mb-6 p-3 rounded-2xl bg-[var(--color-paper-entry)] border border-[var(--color-paper-border)]",
                div { class: "flex items-center gap-2",
                    SkeletonBox { class: "h-4 w-16 rounded" }
                    SkeletonBox { class: "h-5 w-14 rounded-full" }
                }
                div { class: "flex items-center gap-2",
                    SkeletonBox { class: "h-9 w-24 rounded-full" }
                    SkeletonBox { class: "h-9 w-20 rounded-full" }
                }
            }

            // 面包屑占位（PostHeader 内的 Breadcrumbs）
            SkeletonBox { class: "h-4 w-48 rounded mb-6" }

            // 标题占位（模拟 h1.post-title）
            SkeletonBox { class: "h-10 w-4/5 rounded mb-4" }

            // 摘要占位（post-description）
            SkeletonBox { class: "h-5 w-2/3 rounded mb-4" }

            // 元信息行（PostMeta：作者 · 日期 · 阅读时间）
            div { class: "flex items-center gap-2 mb-8",
                SkeletonBox { class: "h-4 w-16 rounded" }
                SkeletonBox { class: "h-4 w-1 rounded" }
                SkeletonBox { class: "h-4 w-24 rounded" }
                SkeletonBox { class: "h-4 w-1 rounded" }
                SkeletonBox { class: "h-4 w-20 rounded" }
            }

            // 封面图占位（PostCover 16:9 比例）
            SkeletonBox { class: "w-full aspect-video rounded-lg mb-8" }

            // 正文段落占位（PostContent 多段）
            div { class: "space-y-4 mb-8",
                SkeletonBox { class: "h-4 w-full rounded" }
                SkeletonBox { class: "h-4 w-full rounded" }
                SkeletonBox { class: "h-4 w-5/6 rounded" }
                SkeletonBox { class: "h-4 w-full rounded" }
                SkeletonBox { class: "h-4 w-full rounded" }
                SkeletonBox { class: "h-4 w-3/4 rounded" }
                // 空行模拟段落间距
                div { class: "h-2" }
                SkeletonBox { class: "h-4 w-full rounded" }
                SkeletonBox { class: "h-4 w-full rounded" }
                SkeletonBox { class: "h-4 w-2/3 rounded" }
            }

            // PostFooter 占位
            div { class: "border-t border-gray-200 dark:border-gray-700 pt-6 mt-8",
                div { class: "flex items-center justify-between",
                    SkeletonBox { class: "h-4 w-32 rounded" }
                    SkeletonBox { class: "h-4 w-24 rounded" }
                }
            }
        }
    }
}
