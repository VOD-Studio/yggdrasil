//! 骨架屏原子组件
//!
//! 提供通用的扫光占位块，供各页面骨架屏组合使用。

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
    class: &'static str,
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
