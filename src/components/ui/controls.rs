//! Buttons and native selection controls.
use super::styles::{BTN_PRIMARY, BTN_PRIMARY_SM, SPINNER_SVG};
use dioxus::prelude::*;
/// 主操作按钮，内置 loading spinner 叠加（统一三态）。
///
/// 用于全站所有主题绿 CTA（执行 / 刷新 / 创建备份 / 发布 / 保存设置）。
/// 三态：
/// - `loading=true`：主题绿底 + 文字隐藏（`opacity-0`）+ spinner 绝对居中
///   （按钮宽度不变，避免加载时布局抖动）
/// - `disabled=true`（且未 loading）：灰色底（`bg-paper-tertiary`）+
///   `cursor-not-allowed`
/// - 正常：主题绿底 + `hover:brightness-110` + `active:scale-[0.98]`
///
/// Props：
/// - `label`：正常态显示的文案（loading 时隐藏，由 spinner 占位）
/// - `loading`：是否处于加载态
/// - `disabled`：是否禁用（loading 优先级更高）
/// - `variant`：`"primary"`（默认，`px-5 py-2`）或 `"sm"`（`px-4 py-1.5`）
/// - `onclick`：点击回调
#[component]
pub fn LoadingButton(
    label: String,
    loading: bool,
    #[props(default = false)] disabled: bool,
    #[props(default = "primary")] variant: &'static str,
    onclick: EventHandler<()>,
) -> Element {
    let base = if variant == "sm" {
        BTN_PRIMARY_SM
    } else {
        BTN_PRIMARY
    };
    // 尺寸随 base 常量而定；禁用态需要独立灰色配色（BTN_PRIMARY/_SM 未建模禁用态），
    // 故仅在此分支手写尺寸 + 灰色板，正常态直接复用常量，避免两套配色定义分叉。
    let class = if disabled && !loading {
        let size = if variant == "sm" {
            "px-4 py-1.5"
        } else {
            "px-5 py-2"
        };
        format!(
            "relative inline-flex items-center justify-center {size} rounded-full text-sm font-medium transition-all bg-[var(--color-paper-tertiary)] text-[var(--color-paper-secondary)] cursor-not-allowed"
        )
    } else {
        format!("relative {base}")
    };

    rsx! {
        button {
            class: "{class}",
            disabled: loading || disabled,
            onclick: move |_| onclick.call(()),
            span { class: if loading { "opacity-0" } else { "" }, "{label}" }
            if loading {
                span {
                    class: "absolute inset-0 flex items-center justify-center",
                    dangerous_inner_html: SPINNER_SVG,
                }
            }
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
