//! 骨架屏原子组件
//!
//! 提供通用的扫光占位块，供各页面骨架屏组合使用。

use std::borrow::Cow;

use dioxus::prelude::*;

/// 通用骨架占位块。
///
/// Props：
/// - `class`：Tailwind CSS 类，控制尺寸与形状
/// - `style`：可选的内联样式字符串
/// - `animate`：是否带扫光动画（缺省 `true`）。少数骨架屏在切换动画/骨架屏截断
///   场景需要关闭，避免动画被截断显得突兀。
///
/// 外观见 ui.css 的 `.skeleton`：`ink-1` 到 `ink-2` 的柔和扫光，减少动态效果时静止。
#[component]
pub fn SkeletonBox(
    #[props(into)] class: Cow<'static, str>,
    style: Option<&'static str>,
    #[props(default = true)] animate: bool,
) -> Element {
    rsx! {
        div {
            class: "skeleton {class}",
            "data-static": (!animate).then_some("true"),
            style: style.unwrap_or(""),
        }
    }
}

/// 后台页面标题行骨架，结构与 `admin_layout::AdminPageHead` 一致。
/// `title_width` / `subtitle_width` 只传宽度类（如 `"w-20"`、`"w-96 max-w-full"`）。
#[component]
pub fn AdminPageHeadSkeleton(
    title_width: &'static str,
    subtitle_width: &'static str,
    #[props(default)] class: Option<&'static str>,
    #[props(default)] actions: Option<Element>,
) -> Element {
    let extra = class.unwrap_or_default();
    rsx! {
        div { class: "admin-page-head {extra}",
            div {
                SkeletonBox { class: format!("h-7 {title_width} rounded-md") }
                SkeletonBox { class: format!("h-3.5 {subtitle_width} rounded mt-2") }
            }
            if let Some(actions) = actions {
                div { class: "admin-page-actions", {actions} }
            }
        }
    }
}
