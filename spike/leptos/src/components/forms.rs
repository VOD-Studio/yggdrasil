//! 表单控件组件
//!
//! 提供评论等页面共享的输入框与提示条（原型只移植切片页面用到的部分）。
//! 外观定义在 styles/components/ui.css（`.field`、`.alert`）。

use crate::components::ui::{Icon, IconName};
use leptos::ev::{FocusEvent, KeyboardEvent};
use leptos::prelude::*;

/// 输入框基础类：浅底、发丝线环，聚焦陶土双环，`aria-invalid` 时红环。
pub const INPUT_CLASS: &str = "field w-full";

/// 单行输入框。
///
/// Props：
/// - `id`：元素 id（可选）
/// - `r#type`：input 类型（缺省 `"text"`）
/// - `placeholder`：占位文字（可选）
/// - `value`：受控值
/// - `disabled`：是否禁用（缺省 `false`）
/// - `invalid`：校验失败时标 `aria-invalid`（缺省 `false`）
/// - `required`：是否必填（缺省 `false`）
/// - `readonly`：是否只读（缺省 `false`）
/// - `autofocus`：挂载时是否自动聚焦（缺省 `false`）
/// - `maxlength`：最大字符数（可选）
/// - `autocomplete`：浏览器自动填充提示（可选，如 `"new-password"`）
/// - `min` / `step`：number 类输入的下限与步进（可选）
/// - `aria_describedby`：关联说明元素 id（可选）
/// - `oninput`：输入事件回调，返回新的字符串值（可选；只读框不需要）
/// - `onkeydown` / `onfocus` / `onblur`：可选的键盘与焦点事件回调
/// - `class`：自定义 class（可选，缺省用 [`INPUT_CLASS`] 全宽表单款）。
///   内联/固定宽度场景传自定义类串，覆盖默认样式。
/// - `mono`：是否使用等宽字体（代码片段、表名、JSON 等，缺省 `false`）
/// - `inputmode` / `title` / `aria_label`：可选的原生属性
#[component]
pub fn FormInput(
    #[prop(optional, into)] id: Option<String>,
    #[prop(default = "text")] r#type: &'static str,
    #[prop(optional)] placeholder: Option<&'static str>,
    #[prop(into)] value: Signal<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] invalid: Signal<bool>,
    #[prop(optional)] required: bool,
    #[prop(optional)] readonly: bool,
    #[prop(optional)] autofocus: bool,
    #[prop(optional)] maxlength: Option<i32>,
    #[prop(optional)] autocomplete: Option<&'static str>,
    #[prop(optional)] min: Option<&'static str>,
    #[prop(optional)] step: Option<&'static str>,
    #[prop(optional)] aria_describedby: Option<&'static str>,
    #[prop(optional, into)] oninput: Option<Callback<String>>,
    #[prop(optional, into)] onkeydown: Option<Callback<KeyboardEvent>>,
    #[prop(optional, into)] onfocus: Option<Callback<FocusEvent>>,
    #[prop(optional, into)] onblur: Option<Callback<FocusEvent>>,
    #[prop(optional)] class: Option<&'static str>,
    #[prop(optional)] mono: bool,
    #[prop(optional)] inputmode: Option<&'static str>,
    #[prop(optional)] title: Option<&'static str>,
    #[prop(optional)] aria_label: Option<&'static str>,
) -> impl IntoView {
    let base = class.unwrap_or(INPUT_CLASS);
    let mono_class = if mono { " font-mono" } else { "" };
    let class = move || {
        let disabled_class = if disabled.get() {
            " opacity-60 cursor-not-allowed"
        } else {
            ""
        };
        format!("{base}{mono_class}{disabled_class}")
    };
    view! {
        <input
            id=id.unwrap_or_default()
            class=class
            type=r#type
            placeholder=placeholder
            value=value.get_untracked()
            prop:value=move || value.get()
            disabled=move || disabled.get()
            required=required
            readonly=readonly
            autofocus=autofocus
            maxlength=maxlength.map(|n| n.to_string())
            autocomplete=autocomplete
            min=min
            step=step
            aria-describedby=aria_describedby
            aria-invalid=move || invalid.get().then_some("true")
            inputmode=inputmode.unwrap_or_default()
            title=title.unwrap_or_default()
            aria-label=aria_label
            on:input=move |e| {
                if let Some(handler) = oninput {
                    handler.run(event_target_value(&e));
                }
            }
            on:keydown=move |e| {
                if let Some(handler) = onkeydown {
                    handler.run(e);
                }
            }
            on:focus=move |e| {
                if let Some(handler) = onfocus {
                    handler.run(e);
                }
            }
            on:blur=move |e| {
                if let Some(handler) = onblur {
                    handler.run(e);
                }
            }
        />
    }
}

/// 提示条，用于显示成功、错误等状态消息。
///
/// Props：
/// - `message`：提示文本
/// - `variant`：`"error"`、`"success"`，其他值为中性说明
#[component]
pub fn AlertBox(#[prop(into)] message: String, variant: &'static str) -> impl IntoView {
    let (tone, icon, role) = match variant {
        "error" => ("danger", IconName::CircleAlert, "alert"),
        "success" => ("success", IconName::CircleCheck, "status"),
        _ => ("", IconName::Info, "status"),
    };
    view! {
        <div class="alert mb-4" data-tone=tone role=role>
            <Icon name=icon size=16/>
            <span>{message}</span>
        </div>
    }
}
