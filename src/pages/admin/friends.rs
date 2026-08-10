//! 管理后台「友链」页面。
//!
//! 管理员在此维护前台 `/friends` 展示的友链：新建 / 编辑（名称、URL、头像、描述、
//! 排序、启用状态）/ 删除（物理删除，无回收站）。数据经 Dioxus server functions
//! （`src/api/friends.rs`）加载；写操作成功后由服务端失效 moka 缓存 + `/friends`
//! SSR 缓存，前台下次访问立即生效。
//!
//! 仅 WASM 前端交互（照 mcp.rs 的 `#[cfg(target_arch = "wasm32")]` 门控模式）。
//! 跨子组件状态（刷新触发、操作提示、编辑目标）经一个共享 context 传递。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::friends::{
    create_friend_link, delete_friend_link, list_all_friend_links, update_friend_link,
};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::{FormInput, FormLabel, FormSelect, INPUT_CLASS, INPUT_INLINE_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
#[cfg(target_arch = "wasm32")]
use crate::components::skeletons::friends_admin_skeleton::FriendsAdminSkeleton;
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{
    ADMIN_CARD_CLASS, BADGE_BASE, BTN_OUTLINE, BTN_PRIMARY, BTN_TEXT_ACCENT, BTN_TEXT_RED,
};
#[cfg(target_arch = "wasm32")]
use crate::models::friend_link::FriendLink;
#[cfg(target_arch = "wasm32")]
use crate::pages::admin::asset_picker::AssetPickerModal;

/// 跨子组件共享的页面状态：刷新代际、操作提示、编辑目标。
///
/// `editing` 为 `Some(link)` 时表单进入编辑模式（回填该友链），`None` 为新建模式。
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, PartialEq)]
struct FriendsPageState {
    /// 递增以触发友链列表重新加载（创建/更新/删除后 +1）。
    reload_gen: Signal<u32>,
    /// 全局操作提示：(消息, 是否错误)。
    toast: Signal<Option<(String, bool)>>,
    /// 编辑目标：Some = 编辑模式，None = 新建模式。
    editing: Signal<Option<FriendLink>>,
}

/// 管理后台友链管理页面。
#[component]
pub fn FriendsAdmin() -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let state = FriendsPageState {
            reload_gen: use_signal(|| 0),
            toast: use_signal(|| None),
            editing: use_signal(|| None),
        };
        use_context_provider(|| state);

        rsx! {
            div { class: "w-full max-w-7xl mx-auto space-y-8",
                PageHeader {}
                Toast {}
                EditorCard {}
                LinkList {}
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // server 构建下页面无前端交互；路由实际只在 WASM 渲染。
        rsx! {
            p { class: "text-paper-secondary", "此页面仅在浏览器中可用。" }
        }
    }
}

/// 页头标题区。
#[component]
fn PageHeader() -> Element {
    rsx! {
        div { class: "flex flex-col md:flex-row md:items-end justify-between gap-6 pb-8 border-b border-[var(--color-paper-border)]/50",
            div {
                h1 { class: "text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                    "友链管理"
                }
                p { class: "text-base text-[var(--color-paper-secondary)] mt-2",
                    "维护前台 /friends 页展示的友链。"
                }
            }
        }
    }
}

/// 全局操作提示条（读取共享 context 的 toast）。
#[cfg(target_arch = "wasm32")]
#[component]
fn Toast() -> Element {
    let state: FriendsPageState = use_context();
    let toast = state.toast;
    if let Some((msg, is_err)) = toast() {
        let cls = if is_err {
            "bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300"
        } else {
            "bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300"
        };
        rsx! {
            div { class: "text-sm rounded-lg px-3 py-2 {cls}", "{msg}" }
        }
    } else {
        rsx! {}
    }
}

/// 新建 / 编辑二合一表单卡片。
///
/// 表单草稿用独立 signal（合法独立状态）；进入编辑模式时经 `use_effect`
/// 一次性回填草稿（同 write.rs 的种子回填合法例外），取消/保存成功时清空。
#[cfg(target_arch = "wasm32")]
#[component]
fn EditorCard() -> Element {
    let mut state: FriendsPageState = use_context();
    let mut name = use_signal(String::new);
    let mut url = use_signal(String::new);
    let mut avatar = use_signal(String::new);
    let mut desc = use_signal(String::new);
    let mut sort_str = use_signal(|| "0".to_string());
    let mut is_active = use_signal(|| true);
    let mut busy = use_signal(|| false);
    let mut picker_visible = use_signal(|| false);
    let avatar_uploading = use_signal(|| false);
    let mut avatar_failed = use_signal(|| false);

    let mut editing = state.editing;
    let reload_gen = state.reload_gen;
    let mut toast = state.toast;

    // 编辑目标变化时回填草稿（一次性种子回填；新建/取消时 editing 为 None 不动草稿，
    // 由「取消」按钮与保存成功路径显式清空）。
    use_effect(move || {
        if let Some(link) = editing() {
            name.set(link.name.clone());
            url.set(link.url.clone());
            avatar.set(link.avatar_url.clone().unwrap_or_default());
            avatar_failed.set(false);
            desc.set(link.description.clone());
            sort_str.set(link.sort_order.to_string());
            is_active.set(link.is_active);
        }
    });

    let mut clear_drafts = move || {
        name.set(String::new());
        url.set(String::new());
        avatar.set(String::new());
        avatar_failed.set(false);
        desc.set(String::new());
        sort_str.set("0".to_string());
        is_active.set(true);
    };

    let editing_target = editing();
    let editing_mode = editing_target.is_some();

    rsx! {
        div { class: "{ADMIN_CARD_CLASS} p-8 flex flex-col gap-6",
            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]",
                if editing_mode {
                    "编辑友链"
                } else {
                    "添加友链"
                }
            }

            div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                // 名称
                div { class: "flex flex-col gap-2",
                    FormLabel { label: "名称" }
                    FormInput {
                        r#type: "text",
                        placeholder: "站点名称",
                        value: name(),
                        oninput: move |v: String| name.set(v),
                    }
                }
                // URL
                div { class: "flex flex-col gap-2",
                    FormLabel { label: "URL" }
                    FormInput {
                        r#type: "url",
                        placeholder: "https://example.com",
                        value: url(),
                        oninput: move |v: String| url.set(v),
                    }
                }
                // 头像（可留空）：圆形预览磁贴 + 外链输入框 + 素材库按钮。
                // 保留外链手输（既有用法），新增「素材库」一键选择本站上传的图片。
                div { class: "flex flex-col gap-2",
                    FormLabel { label: "头像（可留空）" }
                    div { class: "flex items-center gap-3",
                        // 圆形预览磁贴：有 URL 且未加载失败时渲染 img，
                        // 否则显示名称首字符兜底（与 LinkRow / 前台 FriendCard 磁贴一致）。
                        div { class: "relative h-10 w-10 shrink-0 rounded-full bg-[var(--color-paper-code-bg)] flex items-center justify-center overflow-hidden",
                            if avatar().trim().is_empty() || avatar_failed() {
                                span { class: "text-sm font-semibold text-[var(--color-paper-primary)] select-none",
                                    { name().chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_else(|| "?".to_string()) }
                                }
                            } else {
                                img {
                                    class: "w-full h-full object-cover",
                                    src: "{avatar()}",
                                    alt: "头像预览",
                                    // 外链可能失效：失败后切回首字符磁贴，不阻塞保存。
                                    onerror: move |_| avatar_failed.set(true),
                                }
                            }
                        }
                        FormInput {
                            r#type: "url",
                            placeholder: "粘贴外链或从素材库选择",
                            value: avatar(),
                            class: INPUT_INLINE_CLASS,
                            oninput: move |v: String| {
                                avatar.set(v);
                                avatar_failed.set(false);
                            },
                        }
                        button {
                            class: "shrink-0 {BTN_OUTLINE}",
                            onclick: move |_| picker_visible.set(true),
                            "素材库"
                        }
                    }
                }
                // 排序
                div { class: "flex flex-col gap-2",
                    FormLabel { label: "排序（越小越靠前）" }
                    FormInput {
                        r#type: "number",
                        placeholder: "0",
                        value: sort_str(),
                        oninput: move |v: String| sort_str.set(v),
                    }
                }
                // 描述
                div { class: "flex flex-col gap-2 md:col-span-2",
                    FormLabel { label: "描述" }
                    textarea {
                        class: "{INPUT_CLASS}",
                        rows: "2",
                        placeholder: "一句话介绍对方站点",
                        value: desc(),
                        oninput: move |e| desc.set(e.value()),
                    }
                }
                // 启用状态（仅编辑模式展示）
                if editing_mode {
                    div { class: "flex flex-col gap-2",
                        FormLabel { label: "状态" }
                        FormSelect {
                            value: is_active(),
                            options: vec![(true, "启用"), (false, "停用")],
                            onchange: move |a: bool| is_active.set(a),
                        }
                    }
                }
            }

            div { class: "flex items-center gap-3",
                button {
                    class: "{BTN_PRIMARY}",
                    disabled: "{busy() || avatar_uploading() || name().trim().is_empty() || url().trim().is_empty()}",
                    onclick: move |_| {
                        if busy() {
                            return;
                        }
                        let n = name().trim().to_string();
                        let u = url().trim().to_string();
                        if n.is_empty() || u.is_empty() {
                            return;
                        }
                        let av = {
                            let a = avatar().trim().to_string();
                            if a.is_empty() { None } else { Some(a) }
                        };
                        let d = desc().trim().to_string();
                        let sort = match sort_str().trim().parse::<i32>() {
                            Ok(s) => s,
                            Err(_) => {
                                toast.set(Some(("排序值必须是整数".to_string(), true)));
                                return;
                            }
                        };
                        let active = is_active();
                        let target = editing();
                        let was_edit = target.is_some();
                        busy.set(true);
                        spawn(async move {
                            let result = match target {
                                Some(link) => {
                                    update_friend_link(link.id, n, u, av, d, sort, active).await
                                }
                                None => create_friend_link(n, u, av, d, sort).await,
                            };
                            match result {
                                Ok(_) => {
                                    toast
                                        .set(
                                            Some((
                                                if was_edit {
                                                    "已保存".to_string()
                                                } else {
                                                    "已添加".to_string()
                                                },
                                                false,
                                            )),
                                        );
                                    editing.set(None);
                                    clear_drafts();
                                    let g = reload_gen();
                                    state.reload_gen.set(g + 1);
                                }
                                Err(e) => {
                                    toast.set(Some((format!("保存失败：{e}"), true)));
                                }
                            }
                            busy.set(false);
                        });
                    },
                    if busy() {
                        "保存中…"
                    } else if editing_mode {
                        "保存修改"
                    } else {
                        "添加友链"
                    }
                }
                if editing_mode {
                    button {
                        class: "{BTN_OUTLINE}",
                        onclick: move |_| {
                            editing.set(None);
                            clear_drafts();
                        },
                        "取消"
                    }
                }
            }
            // 素材选择 modal：选中回填头像 URL（复用 write.rs 封面选择的同一组件）。
            AssetPickerModal {
                visible: picker_visible,
                cover_uploading: avatar_uploading,
                title: "选择头像",
                on_select: move |url: String| {
                    avatar.set(url);
                    avatar_failed.set(false);
                },
            }
        }
    }
}

/// 友链列表卡片：纵向行列表（非表格，友链字段少）。
#[cfg(target_arch = "wasm32")]
#[component]
fn LinkList() -> Element {
    let mut state: FriendsPageState = use_context();
    let mut links = use_signal(|| Vec::<FriendLink>::new());
    let mut loaded_gen = use_signal(|| None::<u32>);
    let mut loading = use_signal(|| true);

    let reload_gen = state.reload_gen;
    let mut toast = state.toast;

    // reload_gen 变化时重新加载。
    use_effect(move || {
        let g = reload_gen();
        if loaded_gen() != Some(g) {
            loaded_gen.set(Some(g));
            spawn(async move {
                match list_all_friend_links().await {
                    Ok(list) => links.set(list),
                    Err(e) => toast.set(Some((format!("加载失败：{e}"), true))),
                }
                loading.set(false);
            });
        }
    });

    rsx! {
        div { class: "{ADMIN_CARD_CLASS} p-8 flex flex-col gap-4",
            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]", "友链列表" }

            if loading() && links().is_empty() {
                DelayedSkeleton { FriendsAdminSkeleton {} }
            } else if links().is_empty() {
                p { class: "text-[var(--color-paper-secondary)] text-sm py-4 text-center",
                    "还没有友链，先添加一位伙伴吧。"
                }
            } else {
                div { class: "flex flex-col divide-y divide-[var(--color-paper-border)]/50",
                    for link in links().iter() {
                        LinkRow { key: "{link.id}", link: link.clone(), state }
                    }
                }
            }
        }
    }
}

/// 单行友链：头像磁贴 + 名称/URL/描述 + 排序 + 状态徽章 + 编辑/删除操作。
#[cfg(target_arch = "wasm32")]
#[component]
fn LinkRow(link: FriendLink, state: FriendsPageState) -> Element {
    let initial: String = link
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".to_string());
    let status_label = if link.is_active { "启用" } else { "停用" };
    let status_class = if link.is_active {
        "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300"
    } else {
        "bg-paper-code-block text-paper-tertiary"
    };

    let id_delete = link.id;
    let link_for_edit = link.clone();
    let mut editing = state.editing;
    let mut toast = state.toast;
    let reload_gen = state.reload_gen;

    rsx! {
        div { class: "flex items-center gap-4 py-4",
            div { class: "relative h-8 w-8 shrink-0 rounded-2xl bg-[var(--color-paper-code-bg)] flex items-center justify-center overflow-hidden",
                span { class: "text-sm font-semibold text-[var(--color-paper-primary)] select-none",
                    "{initial}"
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-baseline gap-2",
                    span { class: "font-medium text-[var(--color-paper-primary)] truncate",
                        "{link.name}"
                    }
                    span { class: "{BADGE_BASE} {status_class}", "{status_label}" }
                }
                div { class: "flex items-baseline gap-3 mt-0.5",
                    if !link.url.is_empty() {
                        span { class: "text-xs text-[var(--color-paper-tertiary)] font-mono truncate",
                            "{link.url}"
                        }
                    }
                    if !link.description.is_empty() {
                        span { class: "text-xs text-[var(--color-paper-secondary)] truncate",
                            "{link.description}"
                        }
                    }
                }
            }
            span { class: "text-xs text-[var(--color-paper-tertiary)] whitespace-nowrap",
                "排序 {link.sort_order}"
            }
            div { class: "flex items-center gap-3 whitespace-nowrap",
                button {
                    class: "{BTN_TEXT_ACCENT}",
                    onclick: move |_| editing.set(Some(link_for_edit.clone())),
                    "编辑"
                }
                button {
                    class: "{BTN_TEXT_RED}",
                    onclick: move |_| {
                        let confirmed = web_sys::window()
                            .and_then(|w| {
                                w.confirm_with_message("确定要删除这条友链吗？").ok()
                            })
                            .unwrap_or(false);
                        if !confirmed {
                            return;
                        }
                        let id = id_delete;
                        spawn(async move {
                            match delete_friend_link(id).await {
                                Ok(()) => {
                                    toast.set(Some(("已删除".to_string(), false)));
                                    let g = reload_gen();
                                    state.reload_gen.set(g + 1);
                                }
                                Err(e) => {
                                    toast.set(Some((format!("删除失败：{e}"), true)));
                                }
                            }
                        });
                    },
                    "删除"
                }
            }
        }
    }
}
