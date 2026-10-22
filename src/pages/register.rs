//! 注册页面
//!
//! 首个注册成功的用户将自动成为管理员，后续注册请求会被服务端拒绝。
//! 字段离开焦点后即时校验（规则与服务端一致），提交时再校验一次全部字段。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::api::auth::{register, AuthResponse};
use crate::components::ui::{Button, ButtonSize, ButtonVariant};
use crate::pages::login::{AuthAlert, AuthField, AuthShell, PasswordField};
use crate::router::Route;
use crate::utils::error::failure_message;

/// 用户名：3–50 字节，只含字母、数字与下划线（与服务端 `validate_username` 一致）。
fn username_error(value: &str) -> Option<String> {
    if value.len() < 3 || value.len() > 50 {
        Some("用户名长度需在 3–50 个字符之间".into())
    } else if !value.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Some("只能包含字母、数字和下划线".into())
    } else {
        None
    }
}

/// 邮箱：本地部分、`@`、带点的域名与至少两位字母的后缀。
fn email_error(value: &str) -> Option<String> {
    let valid = value.split_once('@').is_some_and(|(local, domain)| {
        let ok_local = !local.is_empty()
            && local
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c));
        let ok_domain = domain.rsplit_once('.').is_some_and(|(host, tld)| {
            !host.is_empty()
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))
                && tld.len() >= 2
                && tld.chars().all(|c| c.is_ascii_alphabetic())
        });
        ok_local && ok_domain
    });
    (!valid).then(|| "邮箱格式不正确".into())
}

fn password_error(value: &str) -> Option<String> {
    if value.len() < 8 {
        Some("密码至少 8 位".into())
    } else if value.len() > 256 {
        Some("密码不能超过 256 位".into())
    } else {
        None
    }
}

fn confirm_error(password: &str, confirm: &str) -> Option<String> {
    (password != confirm).then(|| "两次输入的密码不一致".into())
}

/// 注册页面组件
#[component]
pub fn Register() -> Element {
    let mut username = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    // 填过的字段离开焦点后才显示校验结果（空字段失焦不报错，免得点按钮时布局跳动）；
    // 提交时全部显示。
    let mut touched = use_signal(|| [false; 4]);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let mut loading = use_signal(|| false);

    let errors = [
        username_error(&username()),
        email_error(&email()),
        password_error(&password()),
        confirm_error(&password(), &confirm()),
    ];
    let shown = |i: usize| touched()[i].then(|| errors[i].clone()).flatten();
    let mut touch = move |i: usize| touched.with_mut(|t| t[i] = true);

    let on_submit = move |event: FormEvent| {
        event.prevent_default();
        if loading() {
            return;
        }
        touched.set([true; 4]);
        error.set(None);
        let invalid = username_error(&username()).is_some()
            || email_error(&email()).is_some()
            || password_error(&password()).is_some()
            || confirm_error(&password(), &confirm()).is_some();
        if invalid {
            return;
        }
        loading.set(true);
        let (u, e, p) = (username(), email(), password());
        spawn(async move {
            match register(u, e, p).await {
                Ok(AuthResponse { success: true, .. }) => success.set(true),
                Ok(AuthResponse { message, .. }) => error.set(Some(message)),
                Err(e) => error.set(Some(failure_message("注册失败", &e))),
            }
            loading.set(false);
        });
    };

    rsx! {
        document::Title { "注册 · Yggdrasil" }
        AuthShell {
            title: "注册",
            footer: rsx! {
                "已有账号？"
                Link { to: Route::Login {}, "去登录" }
            },
            p { class: "auth-note",
                span { class: "auth-note-dot", aria_hidden: "true" }
                "首个注册的账号会自动成为管理员。"
            }
            if success() {
                div { class: "alert auth-alert", role: "status", "data-tone": "success",
                    span {
                        "注册成功，"
                        Link { to: Route::Login {}, "现在去登录" }
                        "。"
                    }
                }
            } else {
                form { class: "auth-form", onsubmit: on_submit, novalidate: true,
                    AuthField {
                        id: "register-username",
                        label: "用户名",
                        hint: "3–50 个字符",
                        autocomplete: "username",
                        value: username(),
                        error: shown(0),
                        disabled: loading(),
                        oninput: move |v| username.set(v),
                        onblur: move |_| if !username().is_empty() { touch(0) },
                    }
                    AuthField {
                        id: "register-email",
                        label: "邮箱",
                        r#type: "email",
                        autocomplete: "email",
                        value: email(),
                        error: shown(1),
                        disabled: loading(),
                        oninput: move |v| email.set(v),
                        onblur: move |_| if !email().is_empty() { touch(1) },
                    }
                    PasswordField {
                        id: "register-password",
                        label: "密码",
                        hint: "至少 8 位",
                        autocomplete: "new-password",
                        value: password(),
                        error: shown(2),
                        disabled: loading(),
                        oninput: move |v| password.set(v),
                        onblur: move |_| if !password().is_empty() { touch(2) },
                    }
                    PasswordField {
                        id: "register-confirm",
                        label: "确认密码",
                        autocomplete: "new-password",
                        value: confirm(),
                        error: shown(3),
                        disabled: loading(),
                        oninput: move |v| confirm.set(v),
                        onblur: move |_| if !confirm().is_empty() { touch(3) },
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
                        "创建账号"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_matches_server_rules() {
        assert!(username_error("ab").is_some());
        assert!(username_error("xfy_01").is_none());
        assert!(username_error("bad name").is_some());
        assert!(email_error("a@b.co").is_none());
        assert!(email_error("a@b").is_some());
        assert!(email_error("a b@c.com").is_some());
        assert!(password_error("1234567").is_some());
        assert!(password_error("12345678").is_none());
        assert!(confirm_error("a", "b").is_some());
        assert!(confirm_error("a", "a").is_none());
    }
}
