//! 后台「个人资料」页面（`/admin/profile`）。
//!
//! 三个分区纵向排列：个人信息（头像、只读用户名、显示名称、邮箱，改动后才能保存）、
//! 修改密码（当前密码、新密码与强度条、确认）、退出登录。username 是唯一登录凭据，
//! 只读展示。启用 ADMIN_* 环境变量时顶部提示重启后以环境变量为准。保存成功后用
//! 响应里的 `PublicUser` 刷新 `UserContext`，侧栏即时更新。密码强度条只在浏览器端
//! 提示，不改变服务端校验。样式见 styles/pages/admin-profile.css。

use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{lazy_route, LazyRoute};

use crate::components::admin_layout::{AdminActions, AdminPageHead};
use crate::components::assets::{AssetPickerModal, AssetSelection};
use crate::components::forms::FormInput;
use crate::components::skeletons::profile_skeleton::ProfileSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconName, LoadError,
};
use crate::context::UserContext;
use crate::models::user::PublicUser;
use crate::utils::error::failure_message;

/// 新密码最短字符数，与服务端校验一致。
const MIN_PASSWORD: usize = 8;

/// 密码强度 0–4：空、不足 8 位、8–11 位、12 位以上、12 位以上且含符号。
fn password_level(pw: &str) -> u8 {
    let len = pw.chars().count();
    if len == 0 {
        0
    } else if len < MIN_PASSWORD {
        1
    } else if len < 12 {
        2
    } else if pw.chars().any(|c| !c.is_ascii_alphanumeric()) {
        4
    } else {
        3
    }
}

/// 强度条下方的说明。
fn password_hint(pw: &str) -> String {
    let len = pw.chars().count();
    match password_level(pw) {
        0 => format!("至少 {MIN_PASSWORD} 位。"),
        1 => format!("还差 {} 位。", MIN_PASSWORD - len),
        4 => "强度不错。".into(),
        _ => "可以再长一些，或加入符号。".into(),
    }
}

/// 个人资料路由（无 SSR 数据，资料在客户端读取）。
pub struct ProfileRoute;

#[lazy_route]
impl LazyRoute for ProfileRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <Profile/> }.into_any()
    }
}

/// 后台个人资料页面。
#[component]
fn Profile() -> impl IntoView {
    let user = RwSignal::new(None::<PublicUser>);
    let env_admin = RwSignal::new(false);
    let error = RwSignal::new(false);
    let reload = RwSignal::new(0_u32);
    let generation = StoredValue::new(0_u64);
    let actions = use_context::<AdminActions>();

    // 仅客户端读取（Effect 不在服务端运行）；世代号丢弃过期响应。
    Effect::new(move |_| {
        reload.track();
        let mut id = 0;
        generation.update_value(|g| {
            *g += 1;
            id = *g;
        });
        spawn_local(async move {
            let result = crate::api::profile::get_profile().await;
            if generation.try_get_value() != Some(id) {
                return;
            }
            match result {
                Ok(resp) => {
                    let _ = env_admin.try_set(resp.admin_env_active);
                    let _ = user.try_set(Some(resp.user));
                    let _ = error.try_set(false);
                }
                Err(_) => {
                    let _ = error.try_set(true);
                }
            }
        });
    });

    // 按用户 id 重建分区：保存资料后 `user` 更新，但表单与密码输入保持原样。
    let user_id = Memo::new(move |_| user.with(|u| u.as_ref().map(|u| u.id)));

    view! {
        <div class="admin-profile">
            <AdminPageHead title="个人资料">"当前登录账号的展示资料与登录密码。"</AdminPageHead>
            {move || {
                env_admin
                    .get()
                    .then(|| {
                        view! {
                            <p class="admin-profile-note" role="note">
                                <Icon name=IconName::TriangleAlert size=16/>
                                "已启用 ADMIN_* 环境变量：每次服务重启后，邮箱与密码会以环境变量为准。"
                            </p>
                        }
                    })
            }}
            {move || {
                if error.get() {
                    view! {
                        <LoadError
                            title="个人资料加载失败"
                            on_retry=move |()| reload.update(|n| *n += 1)
                        />
                    }
                        .into_any()
                } else if user_id.get().is_some() {
                    view! {
                        <InfoSection user=user on_saved=move |u: PublicUser| user.set(Some(u))/>
                        <PasswordSection/>
                        <section class="admin-profile-logout" aria-labelledby="profile-out-h">
                            <div>
                                <h2 id="profile-out-h">"退出登录"</h2>
                                <p>"只退出当前设备。"</p>
                            </div>
                            <button
                                class=button_class(ButtonVariant::Outline, ButtonSize::Md)
                                data-danger="true"
                                type="button"
                                on:click=move |_| {
                                    if let Some(actions) = actions {
                                        actions.logout.run(());
                                    }
                                }
                            >
                                <Icon name=IconName::LogOut size=15/>
                                "退出登录"
                            </button>
                        </section>
                    }
                        .into_any()
                } else {
                    view! { <ProfileSkeleton/> }.into_any()
                }
            }}
        </div>
    }
}

/// 个人信息：头像、用户名、显示名称、邮箱。
#[component]
fn InfoSection(
    user: RwSignal<Option<PublicUser>>,
    #[prop(into)] on_saved: Callback<PublicUser>,
) -> impl IntoView {
    let toast = use_toast();
    let ctx = expect_context::<UserContext>();
    let base_display = Memo::new(move |_| {
        user.with(|u| {
            u.as_ref()
                .and_then(|u| u.display_name.clone())
                .unwrap_or_default()
        })
    });
    let base_email =
        Memo::new(move |_| user.with(|u| u.as_ref().map(|u| u.email.clone()).unwrap_or_default()));
    let base_avatar =
        Memo::new(move |_| user.with(|u| u.as_ref().and_then(|u| u.avatar_url.clone())));
    let username = Memo::new(move |_| {
        user.with(|u| u.as_ref().map(|u| u.username.clone()).unwrap_or_default())
    });
    let display = RwSignal::new(base_display.get_untracked());
    let email = RwSignal::new(base_email.get_untracked());
    let avatar = RwSignal::new(base_avatar.get_untracked());
    let saving = RwSignal::new(false);
    let picker = RwSignal::new(false);
    let uploading = RwSignal::new(false);
    let avatar_failed = RwSignal::new(false);

    let dirty = Memo::new(move |_| {
        display.with(|d| d.trim() != base_display.get())
            || email.with(|e| e.trim() != base_email.get())
            || avatar.get() != base_avatar.get()
    });
    let initial = Memo::new(move |_| {
        let label = display.with(|d| {
            if d.trim().is_empty() {
                username.get()
            } else {
                d.trim().to_string()
            }
        });
        label
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
    });

    let save = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        if saving.get_untracked() {
            return;
        }
        saving.set(true);
        let (mail, name, av) = (
            email.get_untracked().trim().to_string(),
            display.get_untracked().trim().to_string(),
            avatar.get_untracked(),
        );
        spawn_local(async move {
            match crate::api::profile::update_profile(
                mail,
                Some(name).filter(|n| !n.is_empty()),
                av,
            )
            .await
            {
                Ok(resp) if resp.success => {
                    if let Some(u) = resp.user {
                        let _ = ctx.user.try_set(Some(Arc::new(u.clone())));
                        on_saved.run(u);
                    }
                    toast.success("资料已保存");
                }
                Ok(resp) => toast.error(resp.message),
                Err(e) => toast.error(failure_message("保存失败", &e)),
            }
            let _ = saving.try_set(false);
        });
    };
    let upload = move |evt: leptos::ev::Event| {
        #[cfg(target_arch = "wasm32")]
        {
            let Some(file) = event_target::<web_sys::HtmlInputElement>(&evt)
                .files()
                .and_then(|list| list.item(0))
            else {
                return;
            };
            if let Err(msg) = crate::components::assets::upload_pool::validate_file(
                &file.type_(),
                file.size() as u64,
            ) {
                toast.error(msg);
                return;
            }
            uploading.set(true);
            spawn_local(async move {
                match crate::bridges::tiptap::upload_image_file(file).await {
                    Ok(url) => {
                        let _ = avatar.try_set(Some(url));
                        let _ = avatar_failed.try_set(false);
                    }
                    Err(msg) => toast.error(format!("上传失败：{msg}")),
                }
                let _ = uploading.try_set(false);
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (evt, toast);
    };

    // 带泛型的闭包参数类型放在 `view!` 外，避免宏把 `<` 当作标签。
    let on_pick = move |picks: Vec<AssetSelection>| {
        if let Some(first) = picks.into_iter().next() {
            avatar.set(Some(first.url));
            avatar_failed.set(false);
        }
    };

    view! {
        <form
            class="admin-panel admin-profile-section"
            aria-labelledby="profile-info-h"
            on:submit=save
        >
            <div class="admin-profile-section-head">
                <h2 id="profile-info-h">"个人信息"</h2>
                <p>"显示名称会出现在后台侧栏和文章署名；邮箱用于登录。"</p>
            </div>
            <div class="admin-profile-rows">
                <div class="admin-profile-row">
                    <span class="admin-profile-label">"头像"</span>
                    <div class="admin-profile-avatar-row">
                        <span class="admin-profile-avatar" aria-hidden="true">
                            {move || match avatar.get().filter(|_| !avatar_failed.get()) {
                                Some(url) => {
                                    view! {
                                        <img src=url alt="" on:error=move |_| avatar_failed.set(true)/>
                                    }
                                        .into_any()
                                }
                                None => initial.get().into_any(),
                            }}
                        </span>
                        <button
                            class=button_class(ButtonVariant::Secondary, ButtonSize::Sm)
                            type="button"
                            on:click=move |_| picker.set(true)
                        >
                            "从素材库选择"
                        </button>
                        <label
                            class=format!(
                                "{} admin-profile-upload",
                                button_class(ButtonVariant::Secondary, ButtonSize::Sm),
                            )
                            aria-disabled=move || uploading.get().then_some("true")
                        >
                            {move || if uploading.get() { "上传中…" } else { "上传" }}
                            <input
                                type="file"
                                class="sr-only"
                                accept="image/jpeg,image/png,image/gif,image/webp"
                                disabled=move || uploading.get()
                                on:change=upload
                            />
                        </label>
                        {move || {
                            avatar
                                .with(Option::is_some)
                                .then(|| {
                                    view! {
                                        <button
                                            class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                            type="button"
                                            on:click=move |_| avatar.set(None)
                                        >
                                            "移除"
                                        </button>
                                    }
                                })
                        }}
                    </div>
                </div>
                <div class="admin-profile-row">
                    <span class="admin-profile-label">"用户名"</span>
                    <div>
                        <div class="admin-profile-readonly">{move || username.get()}</div>
                        <p class="admin-profile-hint">"用户名是唯一的登录凭据，不能修改。"</p>
                    </div>
                </div>
                <div class="admin-profile-row">
                    <label class="admin-profile-label" for="profile-display">
                        "显示名称"
                    </label>
                    <FormInput
                        id="profile-display"
                        placeholder="留空则显示用户名"
                        maxlength=50
                        value=display
                        oninput=move |v: String| display.set(v)
                    />
                </div>
                <div class="admin-profile-row">
                    <label class="admin-profile-label" for="profile-email">
                        "邮箱"
                    </label>
                    <FormInput
                        id="profile-email"
                        class="field admin-profile-mono"
                        r#type="email"
                        placeholder="name@example.com"
                        value=email
                        oninput=move |v: String| email.set(v)
                    />
                </div>
            </div>
            <div class="admin-profile-foot">
                <button
                    class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                    type="submit"
                    disabled=move || !dirty.get() || saving.get()
                >
                    {move || if saving.get() { "保存中…" } else { "保存" }}
                </button>
            </div>
        </form>
        <AssetPickerModal
            visible=picker
            cover_uploading=uploading
            title="选择头像"
            on_select=on_pick
        />
    }
}

/// 修改密码：当前密码、新密码（强度条）、确认新密码。
#[component]
fn PasswordSection() -> impl IntoView {
    let toast = use_toast();
    let current = RwSignal::new(String::new());
    let next = RwSignal::new(String::new());
    let confirm = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let level = Memo::new(move |_| next.with(|n| password_level(n)));
    let mismatch = Memo::new(move |_| confirm.with(|c| next.with(|n| !c.is_empty() && c != n)));
    let can_submit = Memo::new(move |_| {
        !busy.get()
            && current.with(|c| !c.is_empty())
            && next.with(|n| n.chars().count() >= MIN_PASSWORD)
            && confirm.with(|c| next.with(|n| c == n))
    });

    let submit = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        if !can_submit.get_untracked() {
            return;
        }
        busy.set(true);
        let (old, new) = (current.get_untracked(), next.get_untracked());
        spawn_local(async move {
            match crate::api::profile::change_password(old, new).await {
                Ok(resp) if resp.success => {
                    let _ = current.try_set(String::new());
                    let _ = next.try_set(String::new());
                    let _ = confirm.try_set(String::new());
                    toast.success("密码已修改，其他设备上的登录已退出");
                }
                Ok(resp) => toast.error(resp.message),
                Err(e) => toast.error(failure_message("修改失败", &e)),
            }
            let _ = busy.try_set(false);
        });
    };

    view! {
        <form
            class="admin-panel admin-profile-section"
            aria-labelledby="profile-pw-h"
            on:submit=submit
        >
            <div class="admin-profile-section-head">
                <h2 id="profile-pw-h">"修改密码"</h2>
                <p>"修改成功后，其他设备上的登录会自动退出，当前设备保持登录。"</p>
            </div>
            <div class="admin-profile-rows">
                <div class="admin-profile-row">
                    <label class="admin-profile-label" for="profile-pw0">
                        "当前密码"
                    </label>
                    <FormInput
                        id="profile-pw0"
                        r#type="password"
                        autocomplete="current-password"
                        value=current
                        oninput=move |v: String| current.set(v)
                    />
                </div>
                <div class="admin-profile-row">
                    <label class="admin-profile-label" for="profile-pw1">
                        "新密码"
                    </label>
                    <div>
                        <FormInput
                            id="profile-pw1"
                            r#type="password"
                            autocomplete="new-password"
                            aria_describedby="profile-pw1-hint"
                            value=next
                            oninput=move |v: String| next.set(v)
                        />
                        <div
                            class="admin-profile-meter"
                            data-level=move || level.get().to_string()
                            aria-hidden="true"
                        >
                            {(1..=4u8)
                                .map(|i| {
                                    view! { <span data-on=move || (i <= level.get()).then_some("true")></span> }
                                })
                                .collect_view()}
                        </div>
                        <p id="profile-pw1-hint" class="admin-profile-hint">
                            {move || next.with(|n| password_hint(n))}
                        </p>
                    </div>
                </div>
                <div class="admin-profile-row">
                    <label class="admin-profile-label" for="profile-pw2">
                        "确认新密码"
                    </label>
                    <div>
                        <FormInput
                            id="profile-pw2"
                            r#type="password"
                            autocomplete="new-password"
                            invalid=mismatch
                            value=confirm
                            oninput=move |v: String| confirm.set(v)
                        />
                        {move || {
                            mismatch
                                .get()
                                .then(|| {
                                    view! {
                                        <p class="admin-profile-hint" data-error="true" role="alert">
                                            "两次输入的新密码不一致。"
                                        </p>
                                    }
                                })
                        }}
                    </div>
                </div>
            </div>
            <div class="admin-profile-foot">
                <button
                    class=button_class(ButtonVariant::Secondary, ButtonSize::Md)
                    type="submit"
                    disabled=move || !can_submit.get()
                >
                    {move || if busy.get() { "修改中…" } else { "修改密码" }}
                </button>
            </div>
        </form>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_level_grows_with_length_and_symbols() {
        assert_eq!(password_level(""), 0);
        assert_eq!(password_level("abc"), 1);
        assert_eq!(password_level("abcdefgh"), 2);
        assert_eq!(password_level("abcdefghijkl"), 3);
        assert_eq!(password_level("abcdefghijk!"), 4);
        // 中文按字符计。
        assert_eq!(password_level("一二三四五六七八"), 2);
    }

    #[test]
    fn password_hint_counts_missing_characters() {
        assert_eq!(password_hint(""), "至少 8 位。");
        assert_eq!(password_hint("abcde"), "还差 3 位。");
        assert_eq!(password_hint("abcdefghijk!"), "强度不错。");
    }
}
