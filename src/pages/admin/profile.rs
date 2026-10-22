//! 后台「个人资料」页面（`/admin/profile`）。
//!
//! 三个分区纵向排列：个人信息（头像、只读用户名、显示名称、邮箱，改动后才能保存）、
//! 修改密码（当前密码、新密码与强度条、确认）、退出登录。username 是唯一登录凭据，
//! 只读展示。启用 ADMIN_* 环境变量时顶部提示重启后以环境变量为准。保存成功后用
//! 响应里的 `PublicUser` 刷新 `UserContext`，侧栏即时更新。密码强度条只在浏览器端
//! 提示，不改变服务端校验。样式见 styles/pages/admin-profile.css。

use crate::components::admin_layout::AdminPageHead;
use dioxus::prelude::*;

use crate::components::assets::{AssetPickerModal, AssetSelection};
use crate::components::forms::FormInput;
use crate::components::skeletons::profile_skeleton::ProfileSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconName, LoadError,
};
use crate::models::user::PublicUser;
#[cfg(target_arch = "wasm32")]
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

/// 后台个人资料页面。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn Profile() -> Element {
    let mut user = use_signal(|| None::<PublicUser>);
    let mut env_admin = use_signal(|| false);
    let mut error = use_signal(|| false);
    let mut reload = use_signal(|| 0_u32);
    let actions: Option<crate::components::admin_layout::AdminActions> = try_consume_context();

    use_effect(move || {
        let _ = reload();
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            match crate::api::profile::get_profile().await {
                Ok(resp) => {
                    env_admin.set(resp.admin_env_active);
                    user.set(Some(resp.user));
                    error.set(false);
                }
                Err(_) => error.set(true),
            }
        });
    });

    rsx! {
        div { class: "admin-profile",
            AdminPageHead { title: "个人资料", "当前登录账号的展示资料与登录密码。" }
            if env_admin() {
                p { class: "admin-profile-note", role: "note",
                    Icon { name: IconName::TriangleAlert, size: 16 }
                    "已启用 ADMIN_* 环境变量：每次服务重启后，邮箱与密码会以环境变量为准。"
                }
            }
            if error() {
                LoadError { title: "个人资料加载失败", on_retry: move |_| reload += 1 }
            } else if let Some(current) = user() {
                InfoSection {
                    key: "{current.id}",
                    user: current,
                    on_saved: move |u: PublicUser| user.set(Some(u)),
                }
                PasswordSection {}
                section { class: "admin-profile-logout", aria_labelledby: "profile-out-h",
                    div {
                        h2 { id: "profile-out-h", "退出登录" }
                        p { "只退出当前设备。" }
                    }
                    button {
                        class: button_class(ButtonVariant::Outline, ButtonSize::Md),
                        "data-danger": "true",
                        r#type: "button",
                        onclick: move |_| {
                            if let Some(actions) = actions {
                                actions.logout.call(());
                            }
                        },
                        Icon { name: IconName::LogOut, size: 15 }
                        "退出登录"
                    }
                }
            } else {
                ProfileSkeleton {}
            }
        }
    }
}

/// 个人信息：头像、用户名、显示名称、邮箱。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn InfoSection(user: PublicUser, on_saved: EventHandler<PublicUser>) -> Element {
    let toast = use_toast();
    let base_display = user.display_name.clone().unwrap_or_default();
    let base_email = user.email.clone();
    let base_avatar = user.avatar_url.clone();
    let mut display = use_signal(|| base_display.clone());
    let mut email = use_signal(|| base_email.clone());
    let mut avatar = use_signal(|| base_avatar.clone());
    let mut saving = use_signal(|| false);
    let mut picker = use_signal(|| false);
    let mut uploading = use_signal(|| false);
    let mut avatar_failed = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    let mut ctx: crate::context::UserContext = use_context();

    let dirty =
        display().trim() != base_display || email().trim() != base_email || avatar() != base_avatar;
    let label = if display().trim().is_empty() {
        user.username.clone()
    } else {
        display().trim().to_string()
    };
    let initial = label
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();

    let save = move |e: FormEvent| {
        e.prevent_default();
        if saving() {
            return;
        }
        saving.set(true);
        let (mail, name, av) = (
            email().trim().to_string(),
            display().trim().to_string(),
            avatar(),
        );
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            match crate::api::profile::update_profile(
                mail,
                Some(name).filter(|n| !n.is_empty()),
                av,
            )
            .await
            {
                Ok(resp) if resp.success => {
                    if let Some(u) = resp.user {
                        ctx.user.set(Some(std::sync::Arc::new(u.clone())));
                        on_saved.call(u);
                    }
                    toast.success("资料已保存");
                }
                Ok(resp) => toast.error(resp.message),
                Err(e) => toast.error(failure_message("保存失败", &e)),
            }
            saving.set(false);
        });
    };
    let upload = move |evt: FormEvent| {
        #[cfg(target_arch = "wasm32")]
        {
            use dioxus::web::WebFileExt;
            let Some(file) = evt
                .files()
                .into_iter()
                .next()
                .and_then(|f| f.get_web_file())
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
            spawn(async move {
                match crate::bridges::tiptap::upload_image_file(file).await {
                    Ok(url) => {
                        avatar.set(Some(url));
                        avatar_failed.set(false);
                    }
                    Err(msg) => toast.error(format!("上传失败：{msg}")),
                }
                uploading.set(false);
            });
        }
    };

    rsx! {
        form { class: "admin-panel admin-profile-section", aria_labelledby: "profile-info-h", onsubmit: save,
            div { class: "admin-profile-section-head",
                h2 { id: "profile-info-h", "个人信息" }
                p { "显示名称会出现在后台侧栏和文章署名；邮箱用于登录。" }
            }
            div { class: "admin-profile-rows",
                div { class: "admin-profile-row",
                    span { class: "admin-profile-label", "头像" }
                    div { class: "admin-profile-avatar-row",
                        span { class: "admin-profile-avatar", aria_hidden: "true",
                            match avatar().filter(|_| !avatar_failed()) {
                                Some(url) => rsx! {
                                    img { src: "{url}", alt: "", onerror: move |_| avatar_failed.set(true) }
                                },
                                None => rsx! { "{initial}" },
                            }
                        }
                        button {
                            class: button_class(ButtonVariant::Secondary, ButtonSize::Sm),
                            r#type: "button",
                            onclick: move |_| picker.set(true),
                            "从素材库选择"
                        }
                        label {
                            class: "{button_class(ButtonVariant::Secondary, ButtonSize::Sm)} admin-profile-upload",
                            "aria-disabled": uploading().then_some("true"),
                            if uploading() { "上传中…" } else { "上传" }
                            input {
                                r#type: "file",
                                class: "sr-only",
                                accept: "image/jpeg,image/png,image/gif,image/webp",
                                disabled: uploading(),
                                onchange: upload,
                            }
                        }
                        if avatar().is_some() {
                            button {
                                class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                                r#type: "button",
                                onclick: move |_| avatar.set(None),
                                "移除"
                            }
                        }
                    }
                }
                div { class: "admin-profile-row",
                    span { class: "admin-profile-label", "用户名" }
                    div {
                        div { class: "admin-profile-readonly", "{user.username}" }
                        p { class: "admin-profile-hint", "用户名是唯一的登录凭据，不能修改。" }
                    }
                }
                div { class: "admin-profile-row",
                    label { class: "admin-profile-label", r#for: "profile-display", "显示名称" }
                    FormInput {
                        id: "profile-display",
                        placeholder: "留空则显示用户名",
                        maxlength: 50,
                        value: format!("{display}"),
                        oninput: move |v: String| display.set(v),
                    }
                }
                div { class: "admin-profile-row",
                    label { class: "admin-profile-label", r#for: "profile-email", "邮箱" }
                    FormInput {
                        id: "profile-email",
                        class: "field admin-profile-mono",
                        r#type: "email",
                        placeholder: "name@example.com",
                        value: format!("{email}"),
                        oninput: move |v: String| email.set(v),
                    }
                }
            }
            div { class: "admin-profile-foot",
                button {
                    class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                    r#type: "submit",
                    disabled: !dirty || saving(),
                    if saving() { "保存中…" } else { "保存" }
                }
            }
        }
        AssetPickerModal {
            visible: picker,
            cover_uploading: uploading,
            title: "选择头像",
            on_select: move |picks: Vec<AssetSelection>| {
                if let Some(first) = picks.into_iter().next() {
                    avatar.set(Some(first.url));
                    avatar_failed.set(false);
                }
            },
        }
    }
}

/// 修改密码：当前密码、新密码（强度条）、确认新密码。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn PasswordSection() -> Element {
    let toast = use_toast();
    let mut current = use_signal(String::new);
    let mut next = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut busy = use_signal(|| false);

    let level = password_level(&next());
    let mismatch = !confirm().is_empty() && confirm() != next();
    let can_submit = !busy()
        && !current().is_empty()
        && next().chars().count() >= MIN_PASSWORD
        && confirm() == next();

    let submit = move |e: FormEvent| {
        e.prevent_default();
        if !can_submit {
            return;
        }
        busy.set(true);
        let (old, new) = (current(), next());
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            match crate::api::profile::change_password(old, new).await {
                Ok(resp) if resp.success => {
                    current.set(String::new());
                    next.set(String::new());
                    confirm.set(String::new());
                    toast.success("密码已修改，其他设备上的登录已退出");
                }
                Ok(resp) => toast.error(resp.message),
                Err(e) => toast.error(failure_message("修改失败", &e)),
            }
            busy.set(false);
        });
    };

    rsx! {
        form { class: "admin-panel admin-profile-section", aria_labelledby: "profile-pw-h", onsubmit: submit,
            div { class: "admin-profile-section-head",
                h2 { id: "profile-pw-h", "修改密码" }
                p { "修改成功后，其他设备上的登录会自动退出，当前设备保持登录。" }
            }
            div { class: "admin-profile-rows",
                div { class: "admin-profile-row",
                    label { class: "admin-profile-label", r#for: "profile-pw0", "当前密码" }
                    FormInput {
                        id: "profile-pw0",
                        r#type: "password",
                        autocomplete: "current-password",
                        value: format!("{current}"),
                        oninput: move |v: String| current.set(v),
                    }
                }
                div { class: "admin-profile-row",
                    label { class: "admin-profile-label", r#for: "profile-pw1", "新密码" }
                    div {
                        FormInput {
                            id: "profile-pw1",
                            r#type: "password",
                            autocomplete: "new-password",
                            aria_describedby: "profile-pw1-hint",
                            value: format!("{next}"),
                            oninput: move |v: String| next.set(v),
                        }
                        div { class: "admin-profile-meter", "data-level": "{level}", aria_hidden: "true",
                            for i in 1..=4u8 {
                                span { key: "{i}", "data-on": (i <= level).then_some("true") }
                            }
                        }
                        p { id: "profile-pw1-hint", class: "admin-profile-hint", "{password_hint(&next())}" }
                    }
                }
                div { class: "admin-profile-row",
                    label { class: "admin-profile-label", r#for: "profile-pw2", "确认新密码" }
                    div {
                        FormInput {
                            id: "profile-pw2",
                            r#type: "password",
                            autocomplete: "new-password",
                            invalid: mismatch,
                            value: format!("{confirm}"),
                            oninput: move |v: String| confirm.set(v),
                        }
                        if mismatch {
                            p { class: "admin-profile-hint", "data-error": "true", role: "alert",
                                "两次输入的新密码不一致。"
                            }
                        }
                    }
                }
            }
            div { class: "admin-profile-foot",
                button {
                    class: button_class(ButtonVariant::Secondary, ButtonSize::Md),
                    r#type: "submit",
                    disabled: !can_submit,
                    if busy() { "修改中…" } else { "修改密码" }
                }
            }
        }
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
