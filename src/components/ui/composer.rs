//! 输入栏：圆角外壳 + 输入框 + 陶土发送键。
use super::icons::{Icon, IconName};
use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ComposerSize {
    #[default]
    Md,
    Sm,
}

/// 输入栏外壳：20px（Sm 为 14px）圆角、surface-3、发丝线 + composer 阴影，
/// 聚焦时 Clay 双环。
///
/// Props：
/// - `size`：Md（默认）或 Sm
/// - `stacked`：块级排列（评论框、快速记录）：不做横排，也不加内边距
/// - `class`：外层附加类
#[component]
pub fn Composer(
    #[prop(optional)] size: ComposerSize,
    #[prop(optional)] stacked: bool,
    #[prop(optional, into)] class: String,
    children: Children,
) -> impl IntoView {
    let size_class = if size == ComposerSize::Sm {
        " composer-sm"
    } else {
        ""
    };
    let stacked_class = if stacked { " composer-stacked" } else { "" };
    view! { <div class=format!("composer{size_class}{stacked_class} {class}")>{children()}</div> }
}

/// 输入法组合中：composition 状态、isComposing、Process 键，或 keyCode 229
/// （Safari 确认候选的回车）。全仓输入法守卫只有这一种写法。
pub(crate) fn ime_composing(composing: bool, e: &KeyboardEvent) -> bool {
    if composing || e.is_composing() || e.key() == "Process" {
        return true;
    }
    #[cfg(target_arch = "wasm32")]
    {
        if e.key_code() == 229 {
            return true;
        }
    }
    false
}

/// 输入栏里的输入框：单行 input 或多行 textarea（`field-sizing: content`），
/// 自带输入法守卫。
///
/// Props：
/// - `value` / `oninput`：受控值
/// - `placeholder`、`id`、`aria_label`、`maxlength`、`autofocus`、`disabled`
/// - `multiline`：多行 textarea；否则单行 input
/// - `search`：单行搜索框，`type=search`、`name=q`、`enterkeyhint=search`
///   （无脚本时随表单 GET 提交）
/// - `on_submit`：多行时 ⌘/Ctrl+Enter 调用；单行时给了就由 Enter 调用并阻止默认，
///   否则交给所在表单原生提交
/// - `onkeydown`：调用方的其余按键（Escape、↓ 等）；输入法组合期间不转发
/// - `oncomposition`：组合输入开始（true）/ 结束（false）
/// - `input_ref` / `textarea_ref`：单行 input / 多行 textarea 的 `NodeRef`，供调用方聚焦
///   （替代 Dioxus 的 `onmounted`）
#[component]
pub fn ComposerInput(
    #[prop(into)] value: Signal<String>,
    #[prop(into)] oninput: Callback<String>,
    #[prop(into)] placeholder: String,
    #[prop(optional, into)] id: Option<String>,
    #[prop(optional, into)] aria_label: Option<String>,
    #[prop(optional)] multiline: bool,
    #[prop(optional)] search: bool,
    #[prop(optional)] maxlength: Option<u32>,
    #[prop(optional)] autofocus: bool,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] on_submit: Option<Callback<()>>,
    #[prop(optional, into)] onkeydown: Option<Callback<KeyboardEvent>>,
    #[prop(optional, into)] oncomposition: Option<Callback<bool>>,
    #[prop(optional)] input_ref: Option<NodeRef<leptos::html::Input>>,
    #[prop(optional)] textarea_ref: Option<NodeRef<leptos::html::Textarea>>,
) -> impl IntoView {
    let composing = RwSignal::new(false);
    let maxlength = maxlength.map(|n| n.to_string());

    let on_keydown = move |e: KeyboardEvent| {
        if ime_composing(composing.get_untracked(), &e) {
            return;
        }
        let submit = if multiline {
            (e.meta_key() || e.ctrl_key()) && e.key() == "Enter"
        } else {
            e.key() == "Enter" && on_submit.is_some()
        };
        if submit {
            if let Some(on_submit) = on_submit {
                e.prevent_default();
                on_submit.run(());
                return;
            }
        }
        if let Some(handler) = onkeydown {
            handler.run(e);
        }
    };
    let on_composition_start = move |_| {
        composing.set(true);
        if let Some(handler) = oncomposition {
            handler.run(true);
        }
    };
    let on_composition_end = move |_| {
        composing.set(false);
        if let Some(handler) = oncomposition {
            handler.run(false);
        }
    };

    if multiline {
        view! {
            <textarea
                node_ref=textarea_ref.unwrap_or_default()
                id=id
                class="composer-input"
                rows="2"
                aria-label=aria_label
                placeholder=placeholder
                maxlength=maxlength
                autofocus=autofocus
                disabled=move || disabled.get()
                prop:value=move || value.get()
                on:input=move |e| oninput.run(event_target_value(&e))
                on:keydown=on_keydown
                on:compositionstart=on_composition_start
                on:compositionend=on_composition_end
            >
                {value.get_untracked()}
            </textarea>
        }
        .into_any()
    } else {
        view! {
            <input
                node_ref=input_ref.unwrap_or_default()
                id=id
                class="composer-input"
                type=if search { "search" } else { "text" }
                name=search.then_some("q")
                enterkeyhint=search.then_some("search")
                autocomplete="off"
                aria-label=aria_label
                placeholder=placeholder
                maxlength=maxlength
                autofocus=autofocus
                disabled=move || disabled.get()
                value=value.get_untracked()
                prop:value=move || value.get()
                on:input=move |e| oninput.run(event_target_value(&e))
                on:keydown=on_keydown
                on:compositionstart=on_composition_start
                on:compositionend=on_composition_end
            />
        }
        .into_any()
    }
}

/// 陶土发送键。不给 `onclick` 时为 `type=submit`。
#[component]
pub fn ComposerSend(
    #[prop(into)] label: Signal<String>,
    #[prop(default = IconName::ArrowRight)] icon: IconName,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] onclick: Option<Callback<MouseEvent>>,
) -> impl IntoView {
    view! {
        <button
            class="composer-send"
            type=if onclick.is_some() { "button" } else { "submit" }
            aria-label=move || label.get()
            disabled=move || disabled.get()
            on:click=move |e| {
                if let Some(handler) = onclick {
                    handler.run(e);
                }
            }
        >
            <Icon name=icon size=16/>
        </button>
    }
}
