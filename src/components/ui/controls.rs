//! Buttons and native selection controls.
use super::display::Tooltip;
use super::icons::{Icon, IconName};
use dioxus::prelude::*;

/// 按钮样式，对应 styles/components/ui.css 的 `.btn-*`。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// 陶土底：一屏只放一个的主操作。
    #[default]
    Primary,
    /// 浅灰底：并列的次要操作。
    Secondary,
    /// 透明底：取消、工具栏里的弱操作。
    Ghost,
    /// 发丝线描边：与主操作并排、需要一点分量的操作。
    Outline,
    /// 红底：不可撤销的删除。
    Danger,
    /// 浅红底：危险但可撤销，或危险操作的入口。
    DangerSoft,
}

impl ButtonVariant {
    const fn class(self) -> &'static str {
        match self {
            ButtonVariant::Primary => "btn-primary",
            ButtonVariant::Secondary => "btn-secondary",
            ButtonVariant::Ghost => "btn-ghost",
            ButtonVariant::Outline => "btn-outline",
            ButtonVariant::Danger => "btn-danger",
            ButtonVariant::DangerSoft => "btn-danger-soft",
        }
    }
}

/// 按钮尺寸：XS 26px、SM 30px、MD 34px、LG 40px。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
}

impl ButtonSize {
    const fn class(self) -> &'static str {
        match self {
            ButtonSize::Xs => "btn-xs",
            ButtonSize::Sm => "btn-sm",
            ButtonSize::Md => "",
            ButtonSize::Lg => "btn-lg",
        }
    }

    const fn icon_px(self) -> u32 {
        match self {
            ButtonSize::Xs => 14,
            ButtonSize::Sm | ButtonSize::Md => 16,
            ButtonSize::Lg => 18,
        }
    }
}

/// 组合按钮类名，供 `Link` 等需要按钮外观的非按钮元素使用。
pub fn button_class(variant: ButtonVariant, size: ButtonSize) -> String {
    format!("btn {} {}", size.class(), variant.class())
}

/// 通用按钮。
///
/// - `icon` 放在文字前；`loading` 时文字隐去、转圈居中，按钮宽度不变，并标 `aria-busy`。
/// - `loading` 或 `disabled` 时不响应点击。
#[component]
pub fn Button(
    #[props(default)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    #[props(default)] icon: Option<IconName>,
    #[props(default)] loading: bool,
    #[props(default)] disabled: bool,
    #[props(default = "button")] r#type: &'static str,
    #[props(default)] class: String,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    let base = button_class(variant, size);
    rsx! {
        button {
            class: "{base} {class}",
            r#type,
            disabled: disabled || loading,
            "aria-busy": loading.then_some("true"),
            onclick: move |e| {
                if let Some(handler) = &onclick {
                    handler.call(e);
                }
            },
            span { class: "btn-label inline-flex items-center gap-[inherit]",
                if let Some(icon) = icon {
                    Icon { name: icon, size: size.icon_px() }
                }
                {children}
            }
            if loading {
                span { class: "btn-spinner",
                    Icon { name: IconName::LoaderCircle, size: size.icon_px() }
                }
            }
        }
    }
}

/// 只有图标的方形按钮。`label` 必填：作为读屏名称，同时显示为悬浮提示；
/// `shortcut` 会跟在提示文字后面。
#[component]
pub fn IconButton(
    icon: IconName,
    label: String,
    #[props(default = ButtonVariant::Ghost)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    #[props(default)] disabled: bool,
    #[props(default)] pressed: Option<bool>,
    #[props(default)] shortcut: Option<&'static str>,
    #[props(default = "top")] tooltip_placement: &'static str,
    #[props(default = "center")] tooltip_align: &'static str,
    #[props(default)] class: String,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    let base = button_class(variant, size);
    rsx! {
        Tooltip { tip: label.clone(), shortcut, placement: tooltip_placement, align: tooltip_align,
            button {
                class: "{base} btn-icon {class}",
                r#type: "button",
                disabled,
                "aria-label": "{label}",
                "aria-pressed": pressed.map(|p| if p { "true" } else { "false" }),
                onclick: move |e| {
                    if let Some(handler) = &onclick {
                        handler.call(e);
                    }
                },
                Icon { name: icon, size: size.icon_px() }
            }
        }
    }
}

/// 主操作按钮，内置 loading 转圈（旧接口，内部用 `Button` 实现）。
///
/// Props：
/// - `label`：文案（loading 时隐去，转圈占位，宽度不变）
/// - `loading`：是否处于加载态
/// - `disabled`：是否禁用（loading 优先级更高）
/// - `variant`：`"primary"`（默认）或 `"sm"`（小号）
/// - `onclick`：点击回调
#[component]
pub fn LoadingButton(
    label: String,
    loading: bool,
    #[props(default = false)] disabled: bool,
    #[props(default = "primary")] variant: &'static str,
    onclick: EventHandler<()>,
) -> Element {
    let size = if variant == "sm" {
        ButtonSize::Sm
    } else {
        ButtonSize::Md
    };
    rsx! {
        Button {
            size,
            loading,
            disabled,
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}

/// 带勾画动画的复选框：对勾 SVG 用 stroke-dashoffset 描边绘制 + scale 弹入。
///
/// 原生 `<input type="checkbox">` 是替换元素，吃不到 `::after`/`::before`，
/// 无法做 transform 描边动画。本组件用透明 input 做命中/无障碍层，同级 SVG
/// path 做对勾视觉层；勾选时 path 的 stroke-dashoffset 归零「画出」对勾，
/// 同时 SVG scale 从 0.6 弹入（弹跳曲线）——双层动画带来盖戳质感。
///
/// 与旧的 `.ygg-checkbox`（纯 background-size 缩放）相比，描边动画更像手写勾画，
/// 且 SVG 是矢量合成层，比光栅 background-image 缩放更锐利。
///
/// Props：
/// - `checked`：受控勾选态
/// - `onchange`：状态变化回调，返回新的 bool
/// - `danger`：危险态（红色语义，SQL 控制台「我了解后果」），缺省 `false`
#[component]
pub fn Checkbox(
    checked: bool,
    onchange: EventHandler<bool>,
    #[props(default)] danger: bool,
) -> Element {
    let wrap = if danger {
        "ygg-cb ygg-cb-danger"
    } else {
        "ygg-cb"
    };
    rsx! {
        span { class: "{wrap}",
            input {
                r#type: "checkbox",
                checked,
                onchange: move |e: Event<FormData>| onchange.call(e.checked()),
            }
            svg { class: "ygg-cb-mark", view_box: "0 0 16 16",
                path { class: "ygg-cb-check", d: "M3.5 8.5l3 3 6-6.5" }
            }
        }
    }
}

/// 主题化单选框，与 Checkbox 使用相同尺寸、主题色和焦点描边。
///
/// 同组使用相同 `name`，每项提供独立 `value`；用外层 `label` 关联可见文案。
/// 保留原生 radio 的互斥、方向键和表单语义，`onchange` 返回选中项的值。
#[component]
pub fn Radio(
    name: String,
    value: String,
    checked: bool,
    onchange: EventHandler<String>,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        input {
            class: "ygg-radio",
            r#type: "radio",
            name,
            value,
            checked,
            disabled,
            onchange: move |event: Event<FormData>| onchange.call(event.value()),
        }
    }
}
