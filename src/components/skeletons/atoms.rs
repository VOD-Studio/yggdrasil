//! 骨架屏原子组件
//!
//! 提供通用的扫光占位块，供各页面骨架屏组合使用。

use leptos::prelude::*;

/// 通用骨架占位块。
///
/// Props：
/// - `width`、`height`、`radius`：可选的 CSS 尺寸与圆角
/// - `class`：页面或组件的附加样式
/// - `style`：可选的内联样式字符串
/// - `animate`：是否带扫光动画（缺省 `true`）。少数骨架屏在切换动画/骨架屏截断
///   场景需要关闭，避免动画被截断显得突兀。
///
/// 外观见 ui.css 的 `.skeleton`：`ink-1` 到 `ink-2` 的柔和扫光，减少动态效果时静止。
#[component]
pub fn SkeletonBox(
    #[prop(optional, into)] class: String,
    #[prop(optional)] width: Option<&'static str>,
    #[prop(optional)] height: Option<&'static str>,
    #[prop(optional)] radius: Option<&'static str>,
    #[prop(optional, into)] style: Option<&'static str>,
    #[prop(default = true)] animate: bool,
) -> impl IntoView {
    view! {
        <div
            class=format!("skeleton {class}")
            data-static=(!animate).then_some("true")
            style=style.unwrap_or("")
            style:--skeleton-width=width
            style:--skeleton-height=height
            style:--skeleton-radius=radius
        ></div>
    }
}

/// 后台页面标题行骨架，结构与 `admin_layout::AdminPageHead` 一致。
/// `title_width` / `subtitle_width` 接受 CSS 长度，如 `"5rem"`。
/// `actions` 是右侧操作区占位，传闭包：`actions=|| view! { <SkeletonBox height="34px" width="6rem" radius="0.5rem"/> }`。
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
                <SkeletonBox width=title_width height="1.75rem" radius="0.375rem"/>
                <SkeletonBox width=subtitle_width height="0.875rem" radius="0.25rem" class="admin-head-skeleton-subtitle"/>
            </div>
            {actions.map(|actions| view! { <div class="admin-page-actions">{actions.run()}</div> })}
        </div>
    }
}
