//! 骨架屏原子组件
//!
//! 提供通用的脉冲动画占位块，供各页面骨架屏组合使用。

use dioxus::prelude::*;

/// 通用骨架占位块。
///
/// Props：
/// - `class`：Tailwind CSS 类，控制尺寸与形状
/// - `style`：可选的内联样式字符串
/// - `animate`：是否带 `animate-pulse` 呼吸动画（缺省 `true`）。少数骨架屏在
///   切换动画/骨架屏截断场景需要关闭，避免动画被截断显得突兀。
///
/// 默认带有 `animate-pulse` 动画与半透明的占位背景。
#[component]
pub fn SkeletonBox(
    class: &'static str,
    style: Option<&'static str>,
    #[props(default = true)] animate: bool,
) -> Element {
    let pulse = if animate { " animate-pulse" } else { "" };
    rsx! {
        div {
            class: "bg-paper-tertiary/30 dark:bg-gray-600{pulse} {class}",
            style: style.unwrap_or(""),
        }
    }
}

/// 通用骨架「幽灵卡片」外壳：半透明背景 + 圆角 + 描边，供各页面骨架屏包裹
/// 内容占位（列表行、表格、日志区等）。收敛此前散落在各骨架屏文件里手打的
/// 同一段 `bg-[var(--color-paper-entry)]/40 rounded-2xl border ...` 前缀。
///
/// Props：
/// - `class`：追加类（内边距、间距、`overflow-hidden`、`shadow-xs` 等按调用方需要传入）
/// - `children`：卡片内的占位内容
#[component]
pub fn SkeletonCard(#[props(default)] class: Option<&'static str>, children: Element) -> Element {
    let extra = class.unwrap_or_default();
    rsx! {
        div {
            class: "bg-[var(--color-paper-entry)]/40 rounded-2xl border border-[var(--color-paper-border)]/70 {extra}",
            {children}
        }
    }
}

/// 表格骨架屏单元格占位形状：不同列的表体内容并非全部是单个占位块，
/// 部分列（如「用户」列）需要头像+两行文字堆叠，因此用枚举描述形状而非
/// 单一 `&'static str`。
#[derive(Clone, Copy, PartialEq)]
pub enum SkeletonCellShape {
    /// 单个占位块，`SkeletonBox` 的 `class`。
    Single(&'static str),
    /// 两行堆叠占位（如标题 + 副标题），依次为两行 `SkeletonBox` 的 `class`。
    Stacked(&'static str, &'static str),
    /// 圆形头像占位 + 两行堆叠占位（如「用户」列：头像 + 昵称/邮箱）。
    AvatarStacked {
        avatar: &'static str,
        line1: &'static str,
        line2: &'static str,
    },
}

/// 表格骨架屏一列的规格：表头单元格类名/占位块类名 + 表体单元格类名/占位形状。
#[derive(Clone, PartialEq)]
pub struct SkeletonTableColumn {
    /// 表头 `th` 的 class（含列宽、对齐、响应式隐藏等）
    pub header_class: &'static str,
    /// 表头占位块（`SkeletonBox`）的 class
    pub header_box_class: &'static str,
    /// 表体 `td` 的 class
    pub cell_class: &'static str,
    /// 表体单元格占位形状
    pub cell_shape: SkeletonCellShape,
}

/// 通用表格骨架屏：渲染 `<table>` 本身（表头一行 + N 行占位表体），不含外层
/// 卡片包裹——各调用方的外层卡片形态并不完全一致（有的独立成卡，有的与标题栏
/// 共享同一张卡，有的嵌套在已带阴影的父卡片内无需再叠一层阴影），故由调用方
/// 自行用 [`SkeletonCard`] 或既有容器包裹。收敛此前散落在多个后台列表页骨架屏
/// 里几乎逐字重复的 `thead`/`tbody` 结构。
///
/// Props：
/// - `columns`：每列的表头/表体占位规格
/// - `rows`：占位行数
#[component]
pub fn SkeletonTable(columns: Vec<SkeletonTableColumn>, rows: usize) -> Element {
    rsx! {
        table { class: "w-full text-sm",
            thead {
                tr { class: "bg-[var(--color-paper-entry)]/80 border-b border-[var(--color-paper-border)]/70",
                    for (i, col) in columns.iter().enumerate() {
                        th { key: "{i}", class: col.header_class,
                            SkeletonBox { class: col.header_box_class }
                        }
                    }
                }
            }
            tbody {
                for r in 0..rows {
                    tr { key: "{r}", class: "border-b border-[var(--color-paper-border)]/60 last:border-0",
                        for (i, col) in columns.iter().enumerate() {
                            td { key: "{i}", class: col.cell_class,
                                match col.cell_shape {
                                    SkeletonCellShape::Single(w) => rsx! {
                                        SkeletonBox { class: w }
                                    },
                                    SkeletonCellShape::Stacked(w1, w2) => rsx! {
                                        div { class: "space-y-1.5",
                                            SkeletonBox { class: w1 }
                                            SkeletonBox { class: w2 }
                                        }
                                    },
                                    SkeletonCellShape::AvatarStacked { avatar, line1, line2 } => rsx! {
                                        div { class: "flex items-center gap-2.5",
                                            SkeletonBox { class: avatar }
                                            div { class: "space-y-1 min-w-0 flex-1",
                                                SkeletonBox { class: line1 }
                                                SkeletonBox { class: line2 }
                                            }
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
