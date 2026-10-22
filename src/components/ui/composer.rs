//! 输入栏：圆角外壳 + 输入框 + 陶土发送键。
use super::icons::{Icon, IconName};
use dioxus::prelude::*;

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
    #[props(default)] size: ComposerSize,
    #[props(default)] stacked: bool,
    #[props(default)] class: &'static str,
    children: Element,
) -> Element {
    let size_class = if size == ComposerSize::Sm {
        " composer-sm"
    } else {
        ""
    };
    let stacked_class = if stacked { " composer-stacked" } else { "" };
    rsx! {
        div { class: "composer{size_class}{stacked_class} {class}", {children} }
    }
}

/// 输入法组合中：composition 状态、isComposing、Process 键，或 keyCode 229
/// （Safari 确认候选的回车）。全仓输入法守卫只有这一种写法。
pub(crate) fn ime_composing(composing: bool, e: &KeyboardEvent) -> bool {
    if composing || e.data().is_composing() || e.key() == Key::Process {
        return true;
    }
    #[cfg(target_arch = "wasm32")]
    {
        use dioxus::web::WebEventExt;
        if e.as_web_event().key_code() == 229 {
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
/// - `onmounted`：挂载回调
#[component]
pub fn ComposerInput(
    value: String,
    oninput: EventHandler<String>,
    #[props(into)] placeholder: String,
    #[props(default)] id: Option<String>,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] multiline: bool,
    #[props(default)] search: bool,
    #[props(default)] maxlength: Option<u32>,
    #[props(default)] autofocus: bool,
    #[props(default)] disabled: bool,
    #[props(default)] on_submit: Option<EventHandler<()>>,
    #[props(default)] onkeydown: Option<EventHandler<KeyboardEvent>>,
    #[props(default)] oncomposition: Option<EventHandler<bool>>,
    #[props(default)] onmounted: Option<EventHandler<MountedEvent>>,
) -> Element {
    let mut composing = use_signal(|| false);
    let maxlength = maxlength.map(|n| n.to_string());

    let on_keydown = move |e: KeyboardEvent| {
        if ime_composing(composing(), &e) {
            return;
        }
        let modifiers = e.modifiers();
        let submit = if multiline {
            (modifiers.meta() || modifiers.ctrl()) && e.key() == Key::Enter
        } else {
            e.key() == Key::Enter && on_submit.is_some()
        };
        if submit {
            if let Some(on_submit) = on_submit {
                e.prevent_default();
                on_submit.call(());
                return;
            }
        }
        if let Some(handler) = onkeydown {
            handler.call(e);
        }
    };
    let on_composition_start = move |_| {
        composing.set(true);
        if let Some(handler) = oncomposition {
            handler.call(true);
        }
    };
    let on_composition_end = move |_| {
        composing.set(false);
        if let Some(handler) = oncomposition {
            handler.call(false);
        }
    };
    let on_mounted = move |e: MountedEvent| {
        if let Some(handler) = onmounted {
            handler.call(e);
        }
    };

    if multiline {
        rsx! {
            textarea {
                id,
                class: "composer-input",
                rows: "2",
                aria_label,
                placeholder,
                maxlength,
                autofocus,
                disabled,
                value: "{value}",
                oninput: move |e| oninput.call(e.value()),
                onkeydown: on_keydown,
                oncompositionstart: on_composition_start,
                oncompositionend: on_composition_end,
                onmounted: on_mounted,
            }
        }
    } else {
        rsx! {
            input {
                id,
                class: "composer-input",
                r#type: if search { "search" } else { "text" },
                name: search.then_some("q"),
                "enterkeyhint": search.then_some("search"),
                autocomplete: "off",
                aria_label,
                placeholder,
                maxlength,
                autofocus,
                disabled,
                value: "{value}",
                oninput: move |e| oninput.call(e.value()),
                onkeydown: on_keydown,
                oncompositionstart: on_composition_start,
                oncompositionend: on_composition_end,
                onmounted: on_mounted,
            }
        }
    }
}

/// 陶土发送键。不给 `onclick` 时为 `type=submit`。
#[component]
pub fn ComposerSend(
    label: &'static str,
    #[props(default = IconName::ArrowRight)] icon: IconName,
    #[props(default)] disabled: bool,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    rsx! {
        button {
            class: "composer-send",
            r#type: if onclick.is_some() { "button" } else { "submit" },
            aria_label: label,
            disabled,
            onclick: move |e| {
                if let Some(handler) = onclick {
                    handler.call(e);
                }
            },
            Icon { name: icon, size: 16 }
        }
    }
}
