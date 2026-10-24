//! 登录页面，以及登录 / 注册共用的外壳与输入框。
//!
//! 页面居中一张 popover 级面板：上方文字标，下方「返回首页」。
//! 登录成功跳转管理后台首页。

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::auth::{login, AuthResponse};
use crate::components::ui::{Button, ButtonSize, ButtonVariant, Icon, IconName};
use crate::context::UserContext;
use crate::routes::Route;
use crate::utils::error::failure_message;

/// 登录与注册共用的外壳：文字标、居中面板、返回首页。
#[component]
pub(crate) fn AuthShell(
    title: &'static str,
    #[prop(optional)] subtitle: Option<&'static str>,
    children: Children,
    #[prop(into)] footer: ViewFn,
) -> impl IntoView {
    view! {
        <div class="auth">
            <a class="auth-wordmark" href=Route::Home {}.to_string()>
                <span class="wordmark-mark" aria-hidden="true"></span>
                "Yggdrasil"
            </a>
            <main class="auth-panel page-enter" aria-labelledby="auth-title">
                <h1 id="auth-title" class="auth-title">
                    {title}
                </h1>
                {subtitle.map(|subtitle| view! { <p class="auth-subtitle">{subtitle}</p> })}
                {children()}
                <p class="auth-switch">{footer.run()}</p>
            </main>
            <a class="auth-home" href=Route::Home {}.to_string()>
                <Icon name=IconName::ArrowLeft size=14/>
                "返回首页"
            </a>
        </div>
    }
}

/// 带标签、提示与错误的输入框。`error` 为 `Some` 时标红并朗读说明。
#[component]
pub(crate) fn AuthField(
    id: &'static str,
    label: &'static str,
    #[prop(default = "text")] r#type: &'static str,
    #[prop(default = "")] autocomplete: &'static str,
    #[prop(default = "")] hint: &'static str,
    #[prop(optional, into)] error: Signal<Option<String>>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(into)] value: Signal<String>,
    #[prop(into)] oninput: Callback<String>,
    #[prop(optional, into)] onblur: Option<Callback<()>>,
) -> impl IntoView {
    let error_id = format!("{id}-error");
    let invalid = move || error.with(Option::is_some);
    view! {
        <div class="auth-field">
            <label class="auth-label" for=id>
                <span>{label}</span>
                {(!hint.is_empty()).then(|| view! { <span class="auth-hint">{hint}</span> })}
            </label>
            <div class="field auth-input">
                <input
                    id=id
                    type=r#type
                    autocomplete=autocomplete
                    prop:value=move || value.get()
                    disabled=move || disabled.get()
                    aria-invalid=move || invalid().then_some("true")
                    aria-describedby={
                        let error_id = error_id.clone();
                        move || invalid().then(|| error_id.clone())
                    }
                    on:input=move |e| oninput.run(event_target_value(&e))
                    on:blur=move |_| {
                        if let Some(handler) = onblur {
                            handler.run(());
                        }
                    }
                />
            </div>
            {move || {
                error
                    .get()
                    .map(|message| view! { <p id=error_id.clone() class="auth-error">{message}</p> })
            }}
        </div>
    }
}

/// 密码框：右侧「显示密码」切换。
#[component]
pub(crate) fn PasswordField(
    id: &'static str,
    label: &'static str,
    autocomplete: &'static str,
    #[prop(default = "")] hint: &'static str,
    #[prop(optional, into)] error: Signal<Option<String>>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(into)] value: Signal<String>,
    #[prop(into)] oninput: Callback<String>,
    #[prop(optional, into)] onblur: Option<Callback<()>>,
) -> impl IntoView {
    let visible = RwSignal::new(false);
    let error_id = format!("{id}-error");
    let invalid = move || error.with(Option::is_some);
    view! {
        <div class="auth-field">
            <label class="auth-label" for=id>
                <span>{label}</span>
                {(!hint.is_empty()).then(|| view! { <span class="auth-hint">{hint}</span> })}
            </label>
            <div class="field auth-input has-action">
                <input
                    id=id
                    type=move || if visible.get() { "text" } else { "password" }
                    autocomplete=autocomplete
                    prop:value=move || value.get()
                    disabled=move || disabled.get()
                    aria-invalid=move || invalid().then_some("true")
                    aria-describedby={
                        let error_id = error_id.clone();
                        move || invalid().then(|| error_id.clone())
                    }
                    on:input=move |e| oninput.run(event_target_value(&e))
                    on:blur=move |_| {
                        if let Some(handler) = onblur {
                            handler.run(());
                        }
                    }
                />
                <button
                    type="button"
                    class="auth-eye"
                    aria-label=move || if visible.get() { "隐藏密码" } else { "显示密码" }
                    aria-pressed=move || visible.get().to_string()
                    on:click=move |_| visible.update(|v| *v = !*v)
                >
                    {move || {
                        let name = if visible.get() { IconName::EyeOff } else { IconName::Eye };
                        view! { <Icon name=name size=16/> }
                    }}
                </button>
            </div>
            {move || {
                error
                    .get()
                    .map(|message| view! { <p id=error_id.clone() class="auth-error">{message}</p> })
            }}
        </div>
    }
}

/// 表单级错误（服务端拒绝、网络失败）。
#[component]
pub(crate) fn AuthAlert(#[prop(into)] message: String) -> impl IntoView {
    view! {
        <p class="alert auth-alert" role="alert" data-tone="danger">
            <Icon name=IconName::CircleAlert size=15/>
            <span>{message}</span>
        </p>
    }
}

/// 登录页面路由（无 SSR 数据）。
pub struct LoginRoute;

#[lazy_route]
impl LazyRoute for LoginRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <Login/> }.into_any()
    }
}

/// 登录页面组件
#[component]
fn Login() -> impl IntoView {
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let loading = RwSignal::new(false);
    // 全局用户上下文，用于触发登录后的状态刷新
    let ctx = expect_context::<UserContext>();

    let on_submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        if loading.get_untracked() {
            return;
        }
        let (username_val, password_val) = (username.get_untracked(), password.get_untracked());
        if username_val.trim().is_empty() || password_val.is_empty() {
            error.set(Some("请填写用户名和密码。".to_string()));
            return;
        }
        error.set(None);
        loading.set(true);
        spawn_local(async move {
            match login(username_val, password_val).await {
                Ok(AuthResponse { success: true, .. }) => {
                    // 登录成功：重置上下文检查标记并跳转到后台
                    let _ = ctx.checked.try_set(false);
                    crate::navigation::push("/admin");
                }
                Ok(AuthResponse { message, .. }) => {
                    let _ = error.try_set(Some(message));
                }
                Err(e) => {
                    let _ = error.try_set(Some(failure_message("登录失败", &e)));
                }
            }
            let _ = loading.try_set(false);
        });
    };

    view! {
        <Title text="登录 · Yggdrasil"/>
        <AuthShell
            title="登录"
            subtitle="进入后台，管理文章、笔记与站点。"
            footer=|| {
                view! {
                    "还没有账号？"
                    <a href=Route::Register {}.to_string()>"去注册"</a>
                }
            }
        >
            <form class="auth-form" on:submit=on_submit>
                <AuthField
                    id="login-username"
                    label="用户名或邮箱"
                    autocomplete="username"
                    value=username
                    disabled=loading
                    oninput=move |v| username.set(v)
                />
                <PasswordField
                    id="login-password"
                    label="密码"
                    autocomplete="current-password"
                    value=password
                    disabled=loading
                    oninput=move |v| password.set(v)
                />
                {move || error.get().map(|message| view! { <AuthAlert message=message/> })}
                <Button
                    r#type="submit"
                    variant=ButtonVariant::Primary
                    size=ButtonSize::Lg
                    loading=loading
                    class="auth-submit"
                >
                    "登录"
                </Button>
            </form>
        </AuthShell>
    }
}
