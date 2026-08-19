//! 后台「个人信息」页面（`/admin/profile`）。
//!
//! 管理当前登录账号的展示资料（显示名称 / 头像 / 邮箱）与登录密码；
//! username 是唯一登录凭据，只读展示。
//!
//! 布局：单列居中卡片流（max-w-2xl）——身份卡（头像 + 展示名 + 角色徽章 +
//! 注册时间）→「基本资料」卡 →「安全」卡。与站点设置页的 scroll-spy 分区
//! 布局刻意不同：本页只有 2-3 个分区，纵向堆叠一次看完，无需左侧导航。
//!
//! 数据流（照 settings 分区的 wasm 门控模式，见 settings/mod.rs 模块文档）：
//! - 挂载后 `use_effect` 一次性拉取 `get_profile` 并回填草稿信号
//!   （一次性「种子」回填，草稿随后与已存值分叉，非派生状态）；
//! - 保存成功后用响应里的 `PublicUser` 同步刷新 `UserContext`，
//!   侧栏用户卡片即时更新，无需整页刷新；
//! - 头像选择复用 `AssetPickerModal` 单选模式（上传新图 / 素材库选择同一弹窗）。
//!
//! 注意：`AssetPickerModal` 是 `position: fixed` 弹层，必须挂在
//! `animate-page-enter` 动画树**之外**——CSS transform 会让 fixed 定位
//! 以变换祖先为参照系而非视口（MCP 页已踩过，见 mcp.rs 修复记录）。
//! 因此本页外层是无动画的布局 wrapper，动画类只加在内容列上。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use crate::api::profile::{change_password, get_profile, update_profile};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::{FormInput, FormLabel};
#[cfg(target_arch = "wasm32")]
use crate::components::skeletons::profile_skeleton::ProfileSkeleton;
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{ADMIN_CARD_CLASS, BTN_GHOST, LoadingButton, StatusBadge, UserAvatar};
#[cfg(target_arch = "wasm32")]
use crate::context::UserContext;
#[cfg(target_arch = "wasm32")]
use crate::models::user::PublicUser;
#[cfg(target_arch = "wasm32")]
use crate::pages::admin::asset_picker::{AssetPickerModal, AssetSelection};

/// Material Symbols `photo_camera`（头像 hover 遮罩；`public/icons/` 有存档）。
#[cfg(target_arch = "wasm32")]
const ICON_CAMERA: &str = "M479.5-267q72.5 0 121.5-49t49-121.5q0-72.5-49-121T479.5-607q-72.5 0-121 48.5t-48.5 121q0 72.5 48.5 121.5t121 49Zm0-60q-47.5 0-78.5-31.5t-31-79q0-47.5 31-78.5t78.5-31q47.5 0 79 31t31.5 78.5q0 47.5-31.5 79t-79 31.5ZM140-120q-24 0-42-18t-18-42v-513q0-23 18-41.5t42-18.5h147l73-87h240l73 87h147q23 0 41.5 18.5T880-693v513q0 24-18.5 42T820-120H140Zm0-60h680v-513H645l-73-87H388l-73 87H140v513Zm340-257Z";
/// Material Symbols `badge`（「基本资料」卡头图标；`public/icons/` 有存档）。
#[cfg(target_arch = "wasm32")]
const ICON_BADGE: &str = "M140-80q-24 0-42-18t-18-42v-480q0-24 18-42t42-18h250v-140q0-24 18-42t42-18h60q24 0 42 18t18 42v140h250q24 0 42 18t18 42v480q0 24-18 42t-42 18H140Zm0-60h680v-480H570v30q0 28-18 44t-42 16h-60q-24 0-42-16t-18-44v-30H140v480Zm92-107h239v-14q0-18-9-32t-23-19q-32-11-50-14.5t-35-3.5q-19 0-40.5 4.5T265-312q-15 5-24 19t-9 32v14Zm336-67h170v-50H568v50Zm-175.5-65.5Q408-395 408-418t-15.5-38.5Q377-472 354-472t-38.5 15.5Q300-441 300-418t15.5 38.5Q331-364 354-364t38.5-15.5ZM568-427h170v-50H568v50ZM450-590h60v-230h-60v230Zm30 210Z";
/// Material Symbols `lock`（「安全」卡头图标；`public/icons/` 有存档）。
#[cfg(target_arch = "wasm32")]
const ICON_LOCK: &str = "M220-80q-24.75 0-42.37-17.63Q160-115.25 160-140v-434q0-24.75 17.63-42.38Q195.25-634 220-634h70v-96q0-78.85 55.61-134.42Q401.21-920 480.11-920q78.89 0 134.39 55.58Q670-808.85 670-730v96h70q24.75 0 42.38 17.62Q800-598.75 800-574v434q0 24.75-17.62 42.37Q764.75-80 740-80H220Zm0-60h520v-434H220v434Zm314.5-162.03Q557-324.06 557-355q0-30-22.67-54.5t-54.5-24.5q-31.83 0-54.33 24.5t-22.5 55q0 30.5 22.67 52.5t54.5 22q31.83 0 54.33-22.03ZM350-634h260v-96q0-54.17-37.88-92.08-37.88-37.92-92-37.92T388-822.08q-38 37.91-38 92.08v96ZM220-140v-434 434Z";

/// 卡头图标气泡（与 settings 分区卡头同一结构：圆形软底气泡 + 24px 图标）。
#[cfg(target_arch = "wasm32")]
fn icon_bubble(path: &'static str) -> Element {
    rsx! {
        span { class: "inline-flex items-center justify-center w-10 h-10 rounded-full bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)] flex-shrink-0",
            svg {
                xmlns: "http://www.w3.org/2000/svg",
                height: "24px",
                view_box: "0 -960 960 960",
                width: "24px",
                fill: "currentColor",
                path { d: "{path}" }
            }
        }
    }
}

/// 后台个人信息页面组件。
#[component]
pub fn Profile() -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let mut ctx: UserContext = use_context();

        let mut loading = use_signal(|| true);
        let mut saved: Signal<Option<PublicUser>> = use_signal(|| None);
        let mut admin_env_active = use_signal(|| false);

        // 基本资料草稿（一次性种子回填后为用户可编辑的独立状态）。
        let mut email_draft = use_signal(String::new);
        let mut display_name_draft = use_signal(String::new);
        let mut avatar_draft: Signal<Option<String>> = use_signal(|| None);
        let mut saving = use_signal(|| false);
        let mut just_saved = use_signal(|| false);

        // 安全卡草稿。
        let mut current_pw = use_signal(String::new);
        let mut new_pw = use_signal(String::new);
        let mut confirm_pw = use_signal(String::new);
        let mut changing = use_signal(|| false);

        let mut picker_visible = use_signal(|| false);
        let avatar_uploading = use_signal(|| false);

        // toast：与 settings/mod.rs 同一实现（展示信号 + 3 秒自动消失）。
        let mut toast_state: Signal<Option<(String, bool)>> = use_signal(|| None);
        let mut display_msg: Signal<String> = use_signal(String::new);
        let mut display_err: Signal<bool> = use_signal(|| false);

        // 初始拉取并种子回填（合法的一次性 effect，见模块文档）。
        use_effect(move || {
            spawn(async move {
                match get_profile().await {
                    Ok(resp) => {
                        email_draft.set(resp.user.email.clone());
                        display_name_draft.set(resp.user.display_name.clone().unwrap_or_default());
                        avatar_draft.set(resp.user.avatar_url.clone());
                        admin_env_active.set(resp.admin_env_active);
                        saved.set(Some(resp.user));
                    }
                    Err(e) => {
                        toast_state.set(Some((format!("加载失败：{e}"), true)));
                    }
                }
                loading.set(false);
            });
        });

        use_effect(move || {
            if let Some((msg, is_err)) = toast_state() {
                display_msg.set(msg.clone());
                display_err.set(is_err);
                let key = msg.clone();
                spawn(async move {
                    crate::utils::time::sleep_ms(3000).await;
                    // 仅当 toast 未被新消息覆盖时清除（避免误清后续 toast）。
                    if toast_state().map(|(m, _)| m == key).unwrap_or(false) {
                        toast_state.set(None);
                    }
                });
            }
        });

        // 脏检查：任一草稿与已存值不同（多字段比较，用 memo 收敛）。
        let dirty = use_memo(move || {
            let Some(s) = saved() else {
                return false;
            };
            email_draft().trim() != s.email
                || display_name_draft().trim() != s.display_name.unwrap_or_default()
                || avatar_draft() != s.avatar_url
        });

        rsx! {
            // 外层布局 wrapper（无动画类）：fixed 弹层挂在动画树外，见模块文档。
            div { class: "w-full max-w-2xl mx-auto",
                div { class: "animate-page-enter flex flex-col gap-8",
                    // 页头（与站点设置页同一排版）
                    div { class: "pb-6 border-b border-[var(--color-paper-border)]",
                        h1 { class: "text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                            "个人信息"
                        }
                        p { class: "text-base text-[var(--color-paper-secondary)] mt-2",
                            "管理当前登录账号的展示资料与登录密码"
                        }
                    }

                    // 操作提示条（3 秒自动消失）
                    div { class: if toast_state().is_some() { "ygg-toast is-open" } else { "ygg-toast" },
                        div { class: if display_err() { "ygg-toast-inner text-sm rounded-lg px-3 py-2 bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300" } else { "ygg-toast-inner text-sm rounded-lg px-3 py-2 bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300" },
                            "{display_msg()}"
                        }
                    }

                    if loading() {
                        div { class: "animate-pulse",
                            ProfileSkeleton {}
                        }
                    } else if let Some(user) = saved() {
                        {
                            // 身份卡展示值：草稿优先（未保存的修改即时预览），
                            // 纯派生，内联计算即可。
                            let label = if display_name_draft().trim().is_empty() {
                                user.username.clone()
                            } else {
                                display_name_draft().trim().to_string()
                            };
                            let created = user.created_at.format("%Y-%m-%d").to_string();
                            rsx! {
                                // 身份卡：头像 + 展示名 + 角色徽章 + 注册时间
                                div { class: "{ADMIN_CARD_CLASS} p-6 md:p-8",
                                    div { class: "flex flex-col sm:flex-row sm:items-center gap-6",
                                        // 头像（点击打开素材选择弹窗；hover/focus 显示相机遮罩）
                                        div { class: "relative group flex-shrink-0 self-center sm:self-start",
                                            button {
                                                class: "relative block w-24 h-24 rounded-full overflow-hidden border border-[var(--color-paper-border)] shadow-sm cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-paper-accent)]/40",
                                                aria_label: "更换头像",
                                                onclick: move |_| picker_visible.set(true),
                                                UserAvatar {
                                                    name: label.clone(),
                                                    avatar_url: avatar_draft(),
                                                    class: "w-24 h-24 rounded-full text-3xl",
                                                }
                                                span { class: "absolute inset-0 flex items-center justify-center bg-black/45 text-white opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity",
                                                    svg {
                                                        xmlns: "http://www.w3.org/2000/svg",
                                                        height: "24px",
                                                        view_box: "0 -960 960 960",
                                                        width: "24px",
                                                        fill: "currentColor",
                                                        path { d: "{ICON_CAMERA}" }
                                                    }
                                                }
                                            }
                                        }
                                        div { class: "flex-1 min-w-0 flex flex-col gap-1.5 text-center sm:text-left",
                                            div { class: "flex items-center justify-center sm:justify-start gap-2.5 flex-wrap",
                                                h2 { class: "text-2xl font-bold text-[var(--color-paper-primary)] truncate",
                                                    "{label}"
                                                }
                                                StatusBadge {
                                                    color_class: "bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300",
                                                    label: "管理员".to_string(),
                                                }
                                            }
                                            p { class: "text-sm text-[var(--color-paper-secondary)] truncate",
                                                "@{user.username}"
                                            }
                                            p { class: "text-xs text-[var(--color-paper-tertiary)]",
                                                "注册于 {created}"
                                            }
                                            div { class: "flex items-center justify-center sm:justify-start gap-3 mt-1",
                                                span { class: "text-xs text-[var(--color-paper-secondary)]",
                                                    "点击头像可更换"
                                                }
                                                if avatar_draft().is_some() {
                                                    button {
                                                        class: "{BTN_GHOST}",
                                                        onclick: move |_| {
                                                            avatar_draft.set(None);
                                                            just_saved.set(false);
                                                        },
                                                        "移除头像"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // 基本资料卡
                                div { class: "{ADMIN_CARD_CLASS} p-6 md:p-8 flex flex-col gap-6",
                                    div { class: "flex items-center gap-3",
                                        {icon_bubble(ICON_BADGE)}
                                        div {
                                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]",
                                                "基本资料"
                                            }
                                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                                "展示在后台侧栏等位置；邮箱用于登录与找回会话。"
                                            }
                                        }
                                    }

                                    if admin_env_active() {
                                        div { class: "rounded-xl border border-amber-300/60 bg-amber-500/10 px-4 py-3 text-sm text-amber-700 dark:text-amber-300",
                                            "检测到 ADMIN_* 环境变量已启用：每次服务重启后，邮箱与密码将以环境变量为准。"
                                        }
                                    }

                                    div { class: "flex flex-col gap-2 max-w-xl",
                                        FormLabel {
                                            label: "用户名".to_string(),
                                            html_for: Some("profile-username".to_string()),
                                        }
                                        FormInput {
                                            id: Some("profile-username".to_string()),
                                            r#type: "text",
                                            placeholder: "",
                                            value: user.username.clone(),
                                            disabled: true,
                                            oninput: move |_| {},
                                        }
                                        p { class: "text-xs text-[var(--color-paper-secondary)]",
                                            "用户名是唯一登录凭据，不可修改。"
                                        }
                                    }

                                    div { class: "flex flex-col gap-2 max-w-xl",
                                        FormLabel {
                                            label: "显示名称".to_string(),
                                            html_for: Some("profile-display-name".to_string()),
                                        }
                                        FormInput {
                                            id: Some("profile-display-name".to_string()),
                                            r#type: "text",
                                            placeholder: "留空则显示用户名",
                                            value: display_name_draft(),
                                            oninput: move |v: String| {
                                                display_name_draft.set(v);
                                                just_saved.set(false);
                                            },
                                        }
                                    }

                                    div { class: "flex flex-col gap-2 max-w-xl",
                                        FormLabel {
                                            label: "邮箱".to_string(),
                                            html_for: Some("profile-email".to_string()),
                                        }
                                        FormInput {
                                            id: Some("profile-email".to_string()),
                                            r#type: "email",
                                            placeholder: "name@example.com",
                                            value: email_draft(),
                                            oninput: move |v: String| {
                                                email_draft.set(v);
                                                just_saved.set(false);
                                            },
                                        }
                                    }

                                    div { class: "flex items-center justify-between gap-4 pt-1",
                                        if just_saved() {
                                            span { class: "inline-flex items-center gap-1.5 text-xs text-[var(--color-paper-accent)]",
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2.5",
                                                    path {
                                                        stroke_linecap: "round",
                                                        stroke_linejoin: "round",
                                                        d: "M5 13l4 4L19 7",
                                                    }
                                                }
                                                "已保存"
                                            }
                                        } else if dirty() {
                                            span { class: "text-xs text-[var(--color-paper-secondary)]",
                                                "有未保存的更改"
                                            }
                                        } else {
                                            span { class: "text-xs text-transparent select-none", "·" }
                                        }
                                        LoadingButton {
                                            label: "保存资料".to_string(),
                                            loading: saving(),
                                            disabled: !dirty() || just_saved() || avatar_uploading(),
                                            onclick: move |_| {
                                                let email = email_draft().trim().to_string();
                                                let display_name = display_name_draft().trim().to_string();
                                                let avatar = avatar_draft();
                                                saving.set(true);
                                                spawn(async move {
                                                    match update_profile(
                                                            email,
                                                            Some(display_name),
                                                            avatar,
                                                        )
                                                        .await
                                                    {
                                                        Ok(resp) if resp.success => {
                                                            if let Some(u) = resp.user {
                                                                // 用服务端归一化后的值回填草稿，并同步
                                                                // 全局上下文（侧栏用户卡片即时更新）。
                                                                email_draft.set(u.email.clone());
                                                                display_name_draft
                                                                    .set(u.display_name.clone().unwrap_or_default());
                                                                avatar_draft.set(u.avatar_url.clone());
                                                                ctx.user.set(Some(Arc::new(u.clone())));
                                                                saved.set(Some(u));
                                                            }
                                                            just_saved.set(true);
                                                            toast_state.set(Some((resp.message, false)));
                                                        }
                                                        Ok(resp) => {
                                                            toast_state.set(Some((resp.message, true)));
                                                        }
                                                        Err(e) => {
                                                            toast_state
                                                                .set(Some((format!("保存失败：{e}"), true)));
                                                        }
                                                    }
                                                    saving.set(false);
                                                });
                                            },
                                        }
                                    }
                                }

                                // 安全卡
                                div { class: "{ADMIN_CARD_CLASS} p-6 md:p-8 flex flex-col gap-6",
                                    div { class: "flex items-center gap-3",
                                        {icon_bubble(ICON_LOCK)}
                                        div {
                                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]",
                                                "安全"
                                            }
                                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                                "修改登录密码；需要验证当前密码。"
                                            }
                                        }
                                    }

                                    div { class: "flex flex-col gap-2 max-w-xl",
                                        FormLabel {
                                            label: "当前密码".to_string(),
                                            html_for: Some("profile-current-pw".to_string()),
                                        }
                                        FormInput {
                                            id: Some("profile-current-pw".to_string()),
                                            r#type: "password",
                                            placeholder: "输入当前密码",
                                            value: current_pw(),
                                            oninput: move |v: String| current_pw.set(v),
                                        }
                                    }

                                    div { class: "grid sm:grid-cols-2 gap-4 max-w-xl",
                                        div { class: "flex flex-col gap-2",
                                            FormLabel {
                                                label: "新密码".to_string(),
                                                html_for: Some("profile-new-pw".to_string()),
                                            }
                                            FormInput {
                                                id: Some("profile-new-pw".to_string()),
                                                r#type: "password",
                                                placeholder: "至少 8 位",
                                                value: new_pw(),
                                                oninput: move |v: String| new_pw.set(v),
                                            }
                                        }
                                        div { class: "flex flex-col gap-2",
                                            FormLabel {
                                                label: "确认新密码".to_string(),
                                                html_for: Some("profile-confirm-pw".to_string()),
                                            }
                                            FormInput {
                                                id: Some("profile-confirm-pw".to_string()),
                                                r#type: "password",
                                                placeholder: "再次输入新密码",
                                                value: confirm_pw(),
                                                oninput: move |v: String| confirm_pw.set(v),
                                            }
                                        }
                                    }

                                    if !confirm_pw().is_empty() && new_pw() != confirm_pw() {
                                        p { class: "text-xs text-red-500 dark:text-red-400",
                                            "两次输入的新密码不一致。"
                                        }
                                    }

                                    p { class: "text-xs text-[var(--color-paper-secondary)] max-w-xl",
                                        if admin_env_active() {
                                            "修改成功后，其他设备的登录会话将自动退出，本端保持登录；已启用 ADMIN_* 环境变量时，重启后密码以环境变量为准。"
                                        } else {
                                            "修改成功后，其他设备的登录会话将自动退出，本端保持登录。"
                                        }
                                    }

                                    div { class: "flex justify-end pt-1",
                                        LoadingButton {
                                            label: "修改密码".to_string(),
                                            loading: changing(),
                                            disabled: current_pw().is_empty()
                                                || new_pw().len() < 8
                                                || new_pw() != confirm_pw(),
                                            onclick: move |_| {
                                                let current = current_pw();
                                                let new = new_pw();
                                                changing.set(true);
                                                spawn(async move {
                                                    match change_password(current, new).await {
                                                        Ok(resp) if resp.success => {
                                                            current_pw.set(String::new());
                                                            new_pw.set(String::new());
                                                            confirm_pw.set(String::new());
                                                            toast_state.set(Some((resp.message, false)));
                                                        }
                                                        Ok(resp) => {
                                                            toast_state.set(Some((resp.message, true)));
                                                        }
                                                        Err(e) => {
                                                            toast_state
                                                                .set(Some((format!("修改失败：{e}"), true)));
                                                        }
                                                    }
                                                    changing.set(false);
                                                });
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "{ADMIN_CARD_CLASS} p-8 text-center text-sm text-[var(--color-paper-secondary)]",
                            "加载失败，请刷新页面重试。"
                        }
                    }
                }

                // 素材选择弹窗（单选）：fixed 定位，必须挂在动画树外（见模块文档）。
                AssetPickerModal {
                    visible: picker_visible,
                    cover_uploading: avatar_uploading,
                    title: "选择头像",
                    on_select: move |picks: Vec<AssetSelection>| {
                        // 单选模式：载荷恰含一个元素。
                        if let Some(first) = picks.into_iter().next() {
                            avatar_draft.set(Some(first.url));
                            just_saved.set(false);
                        }
                    },
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        rsx! {
            div { class: "p-8 text-[var(--color-paper-secondary)]", "个人信息（前端渲染）" }
        }
    }
}
