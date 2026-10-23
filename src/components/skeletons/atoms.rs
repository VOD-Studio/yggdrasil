//! 骨架屏原子组件
//!
//! 提供通用的扫光占位块，供各页面骨架屏组合使用。

use leptos::prelude::*;

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
    #[prop(into)] class: String,
    #[prop(optional, into)] style: Option<&'static str>,
    #[prop(default = true)] animate: bool,
) -> impl IntoView {
    view! {
        <div
            class=format!("skeleton {class}")
            data-static=(!animate).then_some("true")
            style=style.unwrap_or("")
        ></div>
    }
}

/// 后台页面标题行骨架，结构与 `admin_layout::AdminPageHead` 一致。
/// `title_width` / `subtitle_width` 只传宽度类（如 `"w-20"`、`"w-96 max-w-full"`）。
/// `actions` 是右侧操作区占位，传闭包：`actions=|| view! { <SkeletonBox class="h-[34px] w-24 rounded-lg"/> }`。
#[component]
pub fn AdminPageHeadSkeleton(
    title_width: &'static str,
    subtitle_width: &'static str,
    #[prop(optional)] class: Option<&'static str>,
    #[prop(optional, into)] actions: Option<ViewFn>,
) -> impl IntoView {
    let extra = class.unwrap_or_default();
    view! {
        <div class=format!("admin-page-head {extra}")>
            <div>
                <SkeletonBox class=format!("h-7 {title_width} rounded-md")/>
                <SkeletonBox class=format!("h-3.5 {subtitle_width} rounded mt-2")/>
            </div>
            {actions.map(|actions| view! { <div class="admin-page-actions">{actions.run()}</div> })}
        </div>
    }
}
