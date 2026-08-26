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
