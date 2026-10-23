//! Badges, avatars, placeholders and CSS tooltips.
use super::icons::{Icon, IconName};
use super::styles::BADGE_BASE;
use leptos::prelude::*;

/// 状态徽章组件。
///
/// 外层固定 `.badge`，语气类由调用方传入（`badge-success`、`badge-warning` 等，
/// 空串为中性）。之所以用 `color_class` prop 而非枚举变体，是因为部分场景
/// （如回收站剩余天数）的语气由动态逻辑决定，硬编码 variant 反而不够灵活。
///
/// Props：
/// - `color_class`：语气类（如 `badge-success`）
/// - `label`：徽章文本
/// - `class`：附加类，用于所在位置的间距与对齐（如文章标题后的 `post-draft`），缺省为空
#[component]
pub fn StatusBadge(
    color_class: &'static str,
    #[prop(into)] label: String,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    view! { <span class=format!("{BADGE_BASE} {color_class} {class}")>{label}</span> }
}

/// 状态圆点：6px 圆点 + 文字（`.status-dot`），后台列表与面板的状态标识。
///
/// 比 [`StatusBadge`] 安静，适合表格列和元信息行里反复出现的状态。
#[component]
pub fn StatusDot(
    #[prop(into)] label: String,
    #[prop(default = "muted")] tone: &'static str,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    view! { <span class=format!("status-dot {class}") data-tone=tone>{label}</span> }
}

/// 封面与头像加载失败时的图片占位：Lucide `image-off` 图标，尺寸与颜色由 `class` 控制。
#[component]
pub fn ImagePlaceholder(class: &'static str) -> impl IntoView {
    view! { <Icon name=IconName::ImageOff size=24 class=class/> }
}

/// 用户头像：加载失败显示图片占位，无图回退展示名首字符（陶土浅底）。
///
/// 头像三态（图片 / 占位 / 首字符）的统一实现，调用方经 `class` 控制尺寸与形状
/// （须含 `w-* h-* rounded-full` 与字号如 `text-xs`）。
///
/// Props：
/// - `name`：展示名（用于首字符兜底与 alt 文本）
/// - `avatar_url`：头像 URL；`None` 或空白串时渲染首字符兜底
/// - `class`：尺寸与形状类；组件内补 `object-cover`（图片）/ flex 居中（首字符）
#[component]
pub fn UserAvatar(
    #[prop(into)] name: String,
    #[prop(into)] avatar_url: Option<String>,
    class: &'static str,
) -> impl IntoView {
    // 按 URL 记录失败，更换头像后仍可重新加载。
    let failed_url = RwSignal::new(None::<String>);
    let initial = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "?".to_string());
    let url = avatar_url.filter(|u| !u.trim().is_empty());
    let img_ref = NodeRef::<leptos::html::Img>::new();

    // SSR 图片可能在 hydration 前已失败，挂载时补查完成状态。
    {
        let url = url.clone();
        Effect::new(move |_| {
            if let (Some(img), Some(url)) = (img_ref.get(), url.as_ref()) {
                if img.complete() && img.natural_width() == 0 {
                    failed_url.set(Some(url.clone()));
                }
            }
        });
    }

    move || {
        match url.clone() {
        Some(url) if failed_url.read().as_ref() == Some(&url) => view! {
            <span
                class=format!("{class} flex items-center justify-center bg-ink-1 text-fg-3")
                role="img"
                aria-label=format!("{name} 的头像")
            >
                <ImagePlaceholder class="w-1/2 h-1/2"/>
            </span>
        }
        .into_any(),
        Some(url) => {
            let failed = url.clone();
            view! {
                <img
                    node_ref=img_ref
                    class=format!("{class} object-cover")
                    src=url
                    alt=format!("{name} 的头像")
                    loading="lazy"
                    decoding="async"
                    on:error=move |_| failed_url.set(Some(failed.clone()))
                />
            }
            .into_any()
        }
        None => view! {
            <span class=format!(
                "{class} flex items-center justify-center bg-clay-soft text-clay-text font-semibold select-none",
            )>{initial.clone()}</span>
        }
        .into_any(),
    }
    }
}

/// 悬浮提示：深色小气泡，悬停 300ms 后出现，键盘聚焦触发器时立即出现。
///
/// 只给图标按钮补名称或快捷键，不放可点击内容。样式见 ui.css 的 `.tooltip`：
/// 只认包裹层的直接子元素，外层祖先上的悬停不会误触。
///
/// Props：
/// - `tip`：提示文字
/// - `shortcut`：可选的快捷键，显示在文字右侧（如 `⌘K`）
/// - `placement`：`"top"`（默认）/ `"bottom"`
/// - `align`：`"center"`（默认）/ `"start"` / `"end"`；触发器贴容器边缘时朝内侧对齐，
///   避免居中气泡被 `overflow-hidden` 裁掉
#[component]
pub fn Tooltip(
    #[prop(into)] tip: String,
    children: Children,
    #[prop(default = None)] shortcut: Option<&'static str>,
    #[prop(default = "top")] placement: &'static str,
    #[prop(default = "center")] align: &'static str,
) -> impl IntoView {
    let placement = if placement == "bottom" {
        "bottom"
    } else {
        "top"
    };
    let align = match align {
        "start" => "start",
        "end" => "end",
        _ => "center",
    };
    view! {
        <span class="tooltip-wrap">
            {children()}
            <span class="tooltip" role="tooltip" data-placement=placement data-align=align>
                {tip}
                {shortcut.map(|shortcut| view! { <kbd>{shortcut}</kbd> })}
            </span>
        </span>
    }
}
