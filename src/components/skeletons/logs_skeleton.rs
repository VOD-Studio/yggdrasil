//! 后台运行日志页骨架屏
//!
//! 镜像 Logs 页面的内部滚动结构：页头（标题+状态/跟随/导出）+ 筛选栏
//! （级别 chips + target 选择器 + 关键字输入）+ 清理策略折叠卡 + 等宽日志行列表。

use dioxus::prelude::*;

use crate::components::skeletons::atoms::{SkeletonBox, SkeletonCard};

/// 后台运行日志页骨架屏组件。
///
/// Logs 是 internal-scroll 路由（AdminLayout 卡片不滚动、页面自组织分区），
/// 骨架根节点与真实页面同为 `flex-1 min-h-0 flex flex-col` + 自带内边距，
/// 保证骨架 → 内容切换时高度约束一致、无跳动。
#[component]
pub fn LogsSkeleton() -> Element {
    rsx! {
        div { class: "w-full flex-1 min-h-0 flex flex-col px-6 py-8",
            // 页头：标题 + 副标题 / 状态胶囊 + 跟随开关 + 导出按钮
            div { class: "flex-shrink-0 flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70 mb-4",
                div { class: "space-y-1.5",
                    SkeletonBox { class: "h-9 w-40 rounded-lg" }
                    SkeletonBox { class: "h-4 w-72 rounded" }
                }
                div { class: "flex items-center gap-3",
                    SkeletonBox { class: "h-8 w-20 rounded-full" }
                    SkeletonBox { class: "h-6 w-24 rounded-full" }
                    SkeletonBox { class: "h-8 w-24 rounded-full" }
                }
            }

            // 筛选栏卡片：级别 chips + target 选择器 + 搜索框
            SkeletonCard { class: Some("flex-shrink-0 p-4 sm:p-5 space-y-4 shadow-xs mb-4"),
                div { class: "flex flex-wrap gap-2",
                    SkeletonBox { class: "h-7 w-16 rounded-full" }
                    SkeletonBox { class: "h-7 w-14 rounded-full" }
                    SkeletonBox { class: "h-7 w-14 rounded-full" }
                    SkeletonBox { class: "h-7 w-16 rounded-full" }
                    SkeletonBox { class: "h-7 w-16 rounded-full" }
                }
                div { class: "flex flex-col sm:flex-row gap-3",
                    SkeletonBox { class: "h-8 w-full sm:w-40 rounded-lg" }
                    SkeletonBox { class: "h-10 flex-1 rounded-2xl" }
                }
            }

            // 清理策略折叠卡（收起态一条）
            SkeletonBox { class: "flex-shrink-0 h-14 w-full rounded-2xl mb-4" }

            // 日志区：等宽行占位（flex-1 占满剩余高度）
            SkeletonCard { class: Some("flex-1 min-h-0 shadow-xs p-4 space-y-3 overflow-hidden"),
                SkeletonBox { class: "h-4 w-[92%] rounded" }
                SkeletonBox { class: "h-4 w-[78%] rounded" }
                SkeletonBox { class: "h-4 w-[85%] rounded" }
                SkeletonBox { class: "h-4 w-[70%] rounded" }
                SkeletonBox { class: "h-4 w-[88%] rounded" }
                SkeletonBox { class: "h-4 w-[64%] rounded" }
                SkeletonBox { class: "h-4 w-[80%] rounded" }
            }
        }
    }
}
