//! 登录页面，以及登录 / 注册共用的外壳与输入框。
//!
//! 页面居中一张 popover 级面板：上方文字标，下方「返回首页」。
//! 登录成功跳转管理后台首页。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::api::auth::{login, AuthResponse};
use crate::components::ui::{Button, ButtonSize, ButtonVariant, Icon, IconName};
use crate::context::UserContext;
use crate::router::Route;

/// 登录与注册共用的外壳：文字标、居中面板、返回首页。
#[component]
pub(crate) fn AuthShell(
    title: &'static str,
    #[props(default)] subtitle: Option<&'static str>,
    children: Element,
    footer: Element,
) -> Element {
    rsx! {
        div { class: "auth",
            Link { class: "auth-wordmark", to: Route::Home {},
                span { class: "wordmark-mark", aria_hidden: "true" }
                "Yggdrasil"
            }
            main { class: "auth-panel page-enter", aria_labelledby: "auth-title",
                h1 { id: "auth-title", class: "auth-title", "{title}" }
                if let Some(subtitle) = subtitle {
                    p { class: "auth-subtitle", "{subtitle}" }
                }
                {children}
                p { class: "auth-switch", {footer} }
            }
            Link { class: "auth-home", to: Route::Home {},
                Icon { name: IconName::ArrowLeft, size: 14 }
                "返回首页"
            }
        }
    }
}

/// 带标签、提示与错误的输入框。`error` 为 `Some` 时标红并朗读说明。
#[component]
pub(crate) fn AuthField(
    id: &'static str,
    label: &'static str,
    #[props(default = "text")] r#type: &'static str,
    #[props(default)] autocomplete: &'static str,
    #[props(default)] hint: &'static str,
    #[props(default)] error: Option<String>,
    #[props(default)] disabled: bool,
    value: String,
    oninput: EventHandler<String>,
    #[props(default)] onblur: Option<EventHandler<()>>,
) -> Element {
    let error_id = format!("{id}-error");
    let invalid = error.is_some();
    rsx! {
        div { class: "auth-field",
            label { class: "auth-label", r#for: id,
                span { "{label}" }
                if !hint.is_empty() {
                    span { class: "auth-hint", "{hint}" }
                }
            }
            div { class: "field auth-input",
                input {
                    id,
                    r#type,
                    autocomplete,
                    value: "{value}",
                    disabled,
                    aria_invalid: invalid.then_some("true"),
                    aria_describedby: invalid.then_some(error_id.clone()),
                    oninput: move |e| oninput.call(e.value()),
                    onblur: move |_| {
                        if let Some(handler) = onblur {
                            handler.call(());
                        }
                    },
                }
            }
            if let Some(error) = error {
                p { id: "{error_id}", class: "auth-error", "{error}" }
            }
        }
    }
}

/// 密码框：右侧「显示密码」切换。
#[component]
pub(crate) fn PasswordField(
    id: &'static str,
    label: &'static str,
    autocomplete: &'static str,
    #[props(default)] hint: &'static str,
    #[props(default)] error: Option<String>,
    #[props(default)] disabled: bool,
    value: String,
    oninput: EventHandler<String>,
    #[props(default)] onblur: Option<EventHandler<()>>,
) -> Element {
    let mut visible = use_signal(|| false);
    let error_id = format!("{id}-error");
    let invalid = error.is_some();
    rsx! {
        div { class: "auth-field",
            label { class: "auth-label", r#for: id,
                span { "{label}" }
                if !hint.is_empty() {
                    span { class: "auth-hint", "{hint}" }
                }
            }
            div { class: "field auth-input has-action",
                input {
                    id,
                    r#type: if visible() { "text" } else { "password" },
                    autocomplete,
                    value: "{value}",
                    disabled,
                    aria_invalid: invalid.then_some("true"),
                    aria_describedby: invalid.then_some(error_id.clone()),
                    oninput: move |e| oninput.call(e.value()),
                    onblur: move |_| {
                        if let Some(handler) = onblur {
                            handler.call(());
                        }
                    },
                }
                button {
                    r#type: "button",
                    class: "auth-eye",
                    aria_label: if visible() { "隐藏密码" } else { "显示密码" },
                    aria_pressed: "{visible}",
                    onclick: move |_| visible.toggle(),
                    Icon { name: if visible() { IconName::EyeOff } else { IconName::Eye }, size: 16 }
                }
            }
            if let Some(error) = error {
                p { id: "{error_id}", class: "auth-error", "{error}" }
            }
        }
    }
}

/// 表单级错误（服务端拒绝、网络失败）。
#[component]
pub(crate) fn AuthAlert(message: String) -> Element {
    rsx! {
        p { class: "alert auth-alert", role: "alert", "data-tone": "danger",
            Icon { name: IconName::CircleAlert, size: 15 }
            span { "{message}" }
        }
    }
}

/// 登录页面组件
#[component]
pub fn Login() -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);
    // 全局用户上下文，用于触发登录后的状态刷新
    let mut ctx: UserContext = use_context();

    let on_submit = move |event: FormEvent| {
        event.prevent_default();
        if loading() {
            return;
        }
        if username().trim().is_empty() || password().is_empty() {
            error.set(Some("请填写用户名和密码。".to_string()));
            return;
        }
        error.set(None);
        loading.set(true);
        let (username_val, password_val) = (username(), password());
        spawn(async move {
            match login(username_val, password_val).await {
                Ok(AuthResponse { success: true, .. }) => {
                    // 登录成功：重置上下文检查标记并跳转到后台
                    ctx.checked.set(false);
                    let _ = dioxus::router::navigator().push(Route::Admin {});
                }
                Ok(AuthResponse { message, .. }) => error.set(Some(message)),
                Err(e) => error.set(Some(format!("请求失败：{e}"))),
            }
            loading.set(false);
        });
    };

    rsx! {
        document::Title { "登录 · Yggdrasil" }
        AuthShell {
            title: "登录",
            subtitle: "进入后台，管理文章、笔记与站点。",
            footer: rsx! {
                "还没有账号？"
                Link { to: Route::Register {}, "去注册" }
            },
            form { class: "auth-form", onsubmit: on_submit,
                AuthField {
                    id: "login-username",
                    label: "用户名或邮箱",
                    autocomplete: "username",
                    value: username(),
                    disabled: loading(),
                    oninput: move |v| username.set(v),
                }
                PasswordField {
                    id: "login-password",
                    label: "密码",
                    autocomplete: "current-password",
                    value: password(),
                    disabled: loading(),
                    oninput: move |v| password.set(v),
                }
                if let Some(message) = error() {
                    AuthAlert { message }
                }
                Button {
                    r#type: "submit",
                    variant: ButtonVariant::Primary,
                    size: ButtonSize::Lg,
                    loading: loading(),
                    class: "auth-submit",
                    "登录"
                }
            }
        }
    }
}
