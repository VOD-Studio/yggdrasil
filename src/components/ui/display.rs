//! Badges, avatars, placeholders and CSS tooltips.
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

/// 首页封面与头像共用的树芽占位图。
#[component]
pub fn SproutPlaceholder(class: &'static str) -> Element {
    rsx! {
        svg { class, view_box: "0 0 32 32", fill: "none", "aria-hidden": "true",
            path { d: "M16 27V15M16 21C7 21 4 15 5 8C12 8 17 12 16 21ZM16 16C16 7 21 4 28 5C28 12 23 17 16 16M10 27H22", stroke: "currentColor", stroke_width: "1.2", stroke_linecap: "round", stroke_linejoin: "round" }
        }
    }
}

/// 用户头像：加载失败显示树芽，无图回退展示名首字符（accent 软底）。
///
/// 头像三态（图片 / 树芽 / 首字符）的统一实现，调用方经 `class` 控制尺寸与形状
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
                SproutPlaceholder { class: "w-2/3 h-2/3" }
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

/// Tooltip 基础样式（胶囊：黑底白字，hover 显现）。水平定位由调用方经 `align` 选择，
/// 不在此处写死居中——触发器贴容器边缘时居中会让 tooltip 溢出被 `overflow-hidden` 裁掉。
const TOOLTIP_STYLE: &str =
    "pointer-events-none absolute px-3 py-1.5 text-xs font-medium whitespace-nowrap rounded-lg opacity-0 group-hover/tooltip:opacity-100 transition-opacity duration-200 bg-paper-primary text-paper-theme shadow-lg z-50";

/// Tooltip 包裹组件。
///
/// 将任意触发器（按钮等）包裹后，鼠标 hover 时在上方或下方弹出提示。
/// 用 CSS `group/tooltip` + `group-hover/tooltip:opacity-100` 实现，无 JS 状态，`pointer-events-none`
/// 保证不拦截点击。
///
/// Props：
/// - `tip`：提示文案
/// - `children`：触发器元素（按钮 / 链接等）
/// - `placement`：垂直方向，`"top"`（默认，弹出在触发器上方）或 `"bottom"`（下方）
/// - `align`：水平对齐，`"center"`（默认，居中）/ `"start"`（左对齐，向右延伸）/
///   `"end"`（右对齐，向左延伸）
///
/// 注意：父容器若有 `overflow-hidden`（如 `ADMIN_TABLE_CLASS`），`position:absolute`
/// 的 tooltip 会被裁掉。此时除选朝外的 `placement` 外，还须用 `align` 让 tooltip
/// 朝容器内侧延伸——否则居中的宽 tooltip 越过容器边缘即被裁（见 issue #14：
/// 表格最右列按钮的 tooltip 右半段被裁、左半段压住相邻内容）。
#[component]
pub fn Tooltip(
    tip: String,
    children: Element,
    #[props(default = "top")] placement: &'static str,
    #[props(default = "center")] align: &'static str,
) -> Element {
    // 朝上：tooltip 在触发器上方（bottom-full + mb-2）；朝下：在下方（top-full + mt-2）。
    let position_class = if placement == "bottom" {
        "top-full mt-2"
    } else {
        "bottom-full mb-2"
    };
    // 水平对齐：center 居中 / start 左对齐向右 / end 右对齐向左。
    let align_class = match align {
        "start" => "left-0",
        "end" => "right-0",
        _ => "left-1/2 -translate-x-1/2",
    };
    rsx! {
        div { class: "group/tooltip relative inline-flex",
            {children}
            div { class: "{TOOLTIP_STYLE} {position_class} {align_class}", "{tip}" }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_uses_named_group_to_prevent_ancestor_trigger() {
        assert!(
            TOOLTIP_STYLE.contains("group-hover/tooltip:opacity-100"),
            "Tooltip 必须使用专属命名空间 group-hover/tooltip:opacity-100，避免被外层祖先 group 误触"
        );
    }
}
