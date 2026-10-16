//! Badges, avatars, placeholders and CSS tooltips.
use super::icons::{Icon, IconName};
use super::styles::BADGE_BASE;
use dioxus::prelude::*;
/// 状态徽章组件。
///
/// 外层固定 `BADGE_BASE`，颜色类由调用方传入。之所以用 `color_class` prop
/// 而非枚举变体，是因为部分场景（如回收站剩余天数）的颜色由动态逻辑决定
/// （>7 天中性 / ≤7 天主题绿 / ≤0 琥珀），硬编码 variant 反而不够灵活。
///
/// Props：
/// - `color_class`：背景与文字颜色类（如 `post.status.badge_class()` 的返回值）
/// - `label`：徽章文本
#[component]
pub fn StatusBadge(color_class: &'static str, label: String) -> Element {
    rsx! {
        span { class: "{BADGE_BASE} {color_class}", "{label}" }
    }
}

/// 封面与头像加载失败时的图片占位：Lucide `image-off` 图标，尺寸与颜色由 `class` 控制。
#[component]
pub fn ImagePlaceholder(class: &'static str) -> Element {
    rsx! {
        Icon { name: IconName::ImageOff, size: 24, class }
    }
}

/// 用户头像：加载失败显示图片占位，无图回退展示名首字符（accent 软底）。
///
/// 头像三态（图片 / 占位 / 首字符）的统一实现，调用方经 `class` 控制尺寸与形状
/// （须含 `w-* h-* rounded-full` 与字号如 `text-xs`）。使用处：后台侧栏
/// 用户卡片（28px）、个人信息页身份卡（96px，外层按钮带 hover 遮罩）、
/// 前台评论表单身份行（24px）与评论列表（32px）。
///
/// Props：
/// - `name`：展示名（用于首字符兜底与 alt 文本）
/// - `avatar_url`：头像 URL；`None` 或空白串时渲染首字符兜底
/// - `class`：尺寸与形状类；组件内补 `object-cover`（图片）/ flex 居中（首字符）
#[component]
pub fn UserAvatar(name: String, avatar_url: Option<String>, class: &'static str) -> Element {
    // 按 URL 记录失败，更换头像后仍可重新加载。
    let mut failed_url = use_signal(|| None::<String>);
    let initial = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "?".to_string());
    match avatar_url.filter(|u| !u.trim().is_empty()) {
        Some(url) if failed_url.read().as_ref() == Some(&url) => rsx! {
            span {
                class: "{class} flex items-center justify-center bg-paper-entry text-paper-tertiary",
                role: "img",
                aria_label: "{name} 的头像",
                ImagePlaceholder { class: "w-1/2 h-1/2" }
            }
        },
        Some(url) => {
            #[cfg(target_arch = "wasm32")]
            let url_for_mount = url.clone();
            rsx! {
                img {
                    class: "{class} object-cover",
                    src: "{url}",
                    alt: "{name} 的头像",
                    loading: "lazy",
                    decoding: "async",
                    onerror: move |_| failed_url.set(Some(url.clone())),
                    // SSR 图片可能在 hydration 前已失败，挂载时补查完成状态。
                    onmounted: move |_event| {
                        #[cfg(target_arch = "wasm32")]
                        {
                            use wasm_bindgen::JsCast;
                            if let Some(img) = _event.data().downcast::<web_sys::Element>()
                                .and_then(|element| element.dyn_ref::<web_sys::HtmlImageElement>())
                            {
                                if img.complete() && img.natural_width() == 0 {
                                    failed_url.set(Some(url_for_mount.clone()));
                                }
                            }
                        }
                    },
                }
            }
        }
        None => rsx! {
            span { class: "{class} flex items-center justify-center bg-[var(--color-paper-accent-soft)] text-[var(--color-paper-accent)] font-bold select-none",
                "{initial}"
            }
        },
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
    tip: String,
    children: Element,
    #[props(default)] shortcut: Option<&'static str>,
    #[props(default = "top")] placement: &'static str,
    #[props(default = "center")] align: &'static str,
) -> Element {
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
    rsx! {
        span { class: "tooltip-wrap",
            {children}
            span {
                class: "tooltip",
                role: "tooltip",
                "data-placement": placement,
                "data-align": align,
                "{tip}"
                if let Some(shortcut) = shortcut {
                    kbd { "{shortcut}" }
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_renders_shortcut_and_normalizes_position() {
        let html = dioxus::ssr::render_element(rsx! {
            Tooltip { tip: "搜索", shortcut: "⌘K", placement: "left", align: "middle",
                button { "?" }
            }
        });
        assert!(html.contains("role=\"tooltip\""), "{html}");
        assert!(html.contains("<kbd>⌘K</kbd>"), "{html}");
        // 非法取值回落到默认的上方居中，不会输出未定义的 data 值。
        assert!(html.contains("data-placement=\"top\""), "{html}");
        assert!(html.contains("data-align=\"center\""), "{html}");
    }
}
