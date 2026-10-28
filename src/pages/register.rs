//! 注册页面
//!
//! 首个注册成功的用户将自动成为管理员，后续注册请求会被服务端拒绝。
//! 字段离开焦点后即时校验（规则与服务端一致），提交时再校验一次全部字段。

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::auth::{register, AuthResponse};
use crate::components::ui::{Button, ButtonSize, ButtonVariant};
use crate::pages::login::{AuthAlert, AuthField, AuthShell, PasswordField};
use crate::routes::Route;
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

/// 注册页面路由（无 SSR 数据）。
pub struct RegisterRoute;

#[lazy_route]
impl LazyRoute for RegisterRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <Register/> }.into_any()
    }
}

/// 注册页面组件
#[component]
fn Register() -> impl IntoView {
    let username = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let confirm = RwSignal::new(String::new());
    // 填过的字段离开焦点后才显示校验结果（空字段失焦不报错，免得点按钮时布局跳动）；
    // 提交时全部显示。
    let touched = RwSignal::new([false; 4]);
    let error = RwSignal::new(None::<String>);
    let success = RwSignal::new(false);
    let loading = RwSignal::new(false);

    let errors = Memo::new(move |_| {
        [
            username.with(|v| username_error(v)),
            email.with(|v| email_error(v)),
            password.with(|v| password_error(v)),
            password.with(|p| confirm.with(|c| confirm_error(p, c))),
        ]
    });
    let shown = move |i: usize| {
        Signal::derive(move || {
            if touched.with(|t| t[i]) {
                errors.with(|e| e[i].clone())
            } else {
                None
            }
        })
    };
    let touch = move |i: usize| touched.update(|t| t[i] = true);

    let on_submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        if loading.get_untracked() {
            return;
        }
        touched.set([true; 4]);
        error.set(None);
        if errors.with_untracked(|e| e.iter().any(Option::is_some)) {
            return;
        }
        loading.set(true);
        let (u, e, p) = (
            username.get_untracked(),
            email.get_untracked(),
            password.get_untracked(),
        );
        spawn_local(async move {
            match register(u, e, p).await {
                Ok(AuthResponse { success: true, .. }) => {
                    let _ = success.try_set(true);
                }
                Ok(AuthResponse { message, .. }) => {
                    let _ = error.try_set(Some(message));
                }
                Err(e) => {
                    let _ = error.try_set(Some(failure_message("注册失败", &e)));
                }
            }
            let _ = loading.try_set(false);
        });
    };

    view! {
        <Title text="注册 · Yggdrasil"/>
        <AuthShell
            title="注册"
            footer=|| {
                view! {
                    "已有账号？"
                    <a href=Route::Login {}.to_string()>"去登录"</a>
                }
            }
        >
            <p class="auth-note">
                <span class="auth-note-dot" aria-hidden="true"></span>
                "首个注册的账号会自动成为管理员。"
            </p>
            {move || {
                if success.get() {
                    view! {
                        <div class="alert auth-alert" role="status" data-tone="success">
                            <span>
                                "注册成功，"
                                <a href=Route::Login {}.to_string()>"现在去登录"</a>
                                "。"
                            </span>
                        </div>
                    }
                        .into_any()
                } else {
                    view! {
                        <form class="auth-form" on:submit=on_submit novalidate>
                            <AuthField
                                id="register-username"
                                label="用户名"
                                hint="3–50 个字符"
                                autocomplete="username"
                                value=username
                                error=shown(0)
                                disabled=loading
                                oninput=move |v| username.set(v)
                                onblur=move |_| {
                                    if !username.with_untracked(String::is_empty) {
                                        touch(0)
                                    }
                                }
                            />
                            <AuthField
                                id="register-email"
                                label="邮箱"
                                r#type="email"
                                autocomplete="email"
                                value=email
                                error=shown(1)
                                disabled=loading
                                oninput=move |v| email.set(v)
                                onblur=move |_| {
                                    if !email.with_untracked(String::is_empty) {
                                        touch(1)
                                    }
                                }
                            />
                            <PasswordField
                                id="register-password"
                                label="密码"
                                hint="至少 8 位"
                                autocomplete="new-password"
                                value=password
                                error=shown(2)
                                disabled=loading
                                oninput=move |v| password.set(v)
                                onblur=move |_| {
                                    if !password.with_untracked(String::is_empty) {
                                        touch(2)
                                    }
                                }
                            />
                            <PasswordField
                                id="register-confirm"
                                label="确认密码"
                                autocomplete="new-password"
                                value=confirm
                                error=shown(3)
                                disabled=loading
                                oninput=move |v| confirm.set(v)
                                onblur=move |_| {
                                    if !confirm.with_untracked(String::is_empty) {
                                        touch(3)
                                    }
                                }
                            />
                            {move || error.get().map(|message| view! { <AuthAlert message=message/> })}
                            <Button
                                r#type="submit"
                                variant=ButtonVariant::Primary
                                size=ButtonSize::Lg
                                loading=loading
                                class="auth-submit"
                            >
                                "创建账号"
                            </Button>
                        </form>
                    }
                        .into_any()
                }
            }}
        </AuthShell>
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
