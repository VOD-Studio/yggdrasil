//! Buttons and native selection controls.
use super::display::Tooltip;
use super::icons::{Icon, IconName};
use leptos::ev::MouseEvent;
use leptos::prelude::*;

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

/// 组合按钮类名，供 `A` 等需要按钮外观的非按钮元素使用。
pub fn button_class(variant: ButtonVariant, size: ButtonSize) -> String {
    format!("btn {} {}", size.class(), variant.class())
}

/// 通用按钮。
///
/// - `icon` 放在文字前；`loading` 时文字隐去、转圈居中，按钮宽度不变，并标 `aria-busy`。
/// - `loading` 或 `disabled` 时不响应点击。
#[component]
pub fn Button(
    #[prop(optional)] variant: ButtonVariant,
    #[prop(optional)] size: ButtonSize,
    #[prop(optional)] icon: Option<IconName>,
    #[prop(optional, into)] loading: Signal<bool>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(default = "button")] r#type: &'static str,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] onclick: Option<Callback<MouseEvent>>,
    children: Children,
) -> impl IntoView {
    let base = button_class(variant, size);
    view! {
        <button
            class=format!("{base} {class}")
            type=r#type
            disabled=move || disabled.get() || loading.get()
            aria-busy=move || loading.get().then_some("true")
            on:click=move |e| {
                if let Some(handler) = onclick {
                    handler.run(e);
                }
            }
        >
            <span class="btn-label inline-flex items-center gap-[inherit]">
                {icon.map(|icon| view! { <Icon name=icon size=size.icon_px()/> })}
                {children()}
            </span>
            <Show when=move || loading.get()>
                <span class="btn-spinner">
                    <Icon name=IconName::LoaderCircle size=size.icon_px()/>
                </span>
            </Show>
        </button>
    }
}

/// 只有图标的方形按钮。`label` 必填：作为读屏名称，同时显示为悬浮提示；
/// `shortcut` 会跟在提示文字后面。
#[component]
pub fn IconButton(
    icon: IconName,
    #[prop(into)] label: String,
    #[prop(default = ButtonVariant::Ghost)] variant: ButtonVariant,
    #[prop(optional)] size: ButtonSize,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] pressed: Signal<Option<bool>>,
    #[prop(optional)] shortcut: Option<&'static str>,
    #[prop(default = "top")] tooltip_placement: &'static str,
    #[prop(default = "center")] tooltip_align: &'static str,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] onclick: Option<Callback<MouseEvent>>,
) -> impl IntoView {
    let base = button_class(variant, size);
    view! {
        <Tooltip tip=label.clone() shortcut=shortcut placement=tooltip_placement align=tooltip_align>
            <button
                class=format!("{base} btn-icon {class}")
                type="button"
                disabled=move || disabled.get()
                aria-label=label.clone()
                aria-pressed=move || pressed.get().map(|p| if p { "true" } else { "false" })
                on:click=move |e| {
                    if let Some(handler) = onclick {
                        handler.run(e);
                    }
                }
            >
                <Icon name=icon size=size.icon_px()/>
            </button>
        </Tooltip>
    }
}

/// 带勾画动画的复选框：对勾 SVG 用 stroke-dashoffset 描边绘制 + scale 弹入。
///
/// 原生 `<input type="checkbox">` 是替换元素，吃不到 `::after`/`::before`，
/// 无法做 transform 描边动画。本组件用透明 input 做命中/无障碍层，同级 SVG
/// path 做对勾视觉层；勾选时 path 的 stroke-dashoffset 归零「画出」对勾，
/// 同时 SVG scale 从 0.6 弹入（弹跳曲线）——双层动画带来盖戳质感。
///
/// Props：
/// - `checked`：受控勾选态
/// - `onchange`：状态变化回调，返回新的 bool
/// - `danger`：危险态（红色语义），缺省 `false`
/// - `label`：没有可见文字关联时的读屏名称，如表格行的「选择《标题》」
#[component]
pub fn Checkbox(
    #[prop(into)] checked: Signal<bool>,
    #[prop(into)] onchange: Callback<bool>,
    #[prop(optional)] danger: bool,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    let wrap = if danger {
        "ygg-cb ygg-cb-danger"
    } else {
        "ygg-cb"
    };
    view! {
        <span class=wrap>
            <input
                type="checkbox"
                prop:checked=move || checked.get()
                aria-label=label
                on:change=move |e| onchange.run(event_target_checked(&e))
            />
            <svg class="ygg-cb-mark" viewBox="0 0 16 16">
                <path class="ygg-cb-check" d="M3.5 8.5l3 3 6-6.5"></path>
            </svg>
        </span>
    }
}
