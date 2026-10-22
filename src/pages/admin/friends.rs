//! 管理后台「友链」页面（`/admin/friends`）。
//!
//! 左侧为友链列表：拖动把手或在把手上按 ↑ / ↓ 调整顺序，顺序按 10、20、30… 写回
//! 排序值；右侧为添加 / 编辑表单（名称、链接、头像、简介、排序值、是否在前台展示），
//! 编辑时可删除（确认对话框，物理删除）。写操作成功后服务端失效友链缓存与 `/friends`
//! SSR 缓存。数据只在 WASM 前端读取。样式见 styles/pages/admin-friends.css。

use dioxus::prelude::*;

use crate::components::assets::{AssetPickerModal, AssetSelection};
use crate::components::empty_state::EmptyState;
use crate::components::forms::ToggleSwitch;
use crate::components::skeletons::friends_admin_skeleton::FriendsAdminListSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, ConfirmDialog, Icon, IconName, LoadError,
};
use crate::models::friend_link::FriendLink;
use crate::utils::error::failure_message;

/// 排序值步长：给手动插队留出空位。
const SORT_STEP: i32 = 10;

/// 链接去掉协议与末尾斜杠，列表里只显示主机与路径。
fn host_of(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

/// 名称首字（头像缺省时显示）。
fn initial_of(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into())
}

/// 把 `from` 移到 `to` 后，按步长重排排序值；返回排序值有变化的友链。
fn reorder(list: &mut Vec<FriendLink>, from: usize, to: usize) -> Vec<FriendLink> {
    if from == to || from >= list.len() || to >= list.len() {
        return Vec::new();
    }
    let item = list.remove(from);
    list.insert(to, item);
    let mut changed = Vec::new();
    for (i, link) in list.iter_mut().enumerate() {
        let order = (i as i32 + 1) * SORT_STEP;
        if link.sort_order != order {
            link.sort_order = order;
            changed.push(link.clone());
        }
    }
    changed
}

/// 管理后台友链页面。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn FriendsAdmin() -> Element {
    let toast = use_toast();
    let mut links = use_signal(|| None::<Vec<FriendLink>>);
    let mut error = use_signal(|| false);
    let mut reload = use_signal(|| 0_u32);
    let mut editing = use_signal(|| None::<FriendLink>);
    // 拖动中的行与当前悬停的目标行。
    let mut dragging = use_signal(|| None::<usize>);
    let mut drop_at = use_signal(|| None::<usize>);
    let mut saving_order = use_signal(|| false);

    use_effect(move || {
        let _ = reload();
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            match crate::api::friends::list_all_friend_links().await {
                Ok(list) => {
                    // 编辑中的友链若已被删除，退回新建。
                    let current = editing.peek().as_ref().map(|l| l.id);
                    if current.is_some_and(|id| !list.iter().any(|l| l.id == id)) {
                        editing.set(None);
                    }
                    links.set(Some(list));
                    error.set(false);
                }
                Err(_) => error.set(true),
            }
        });
    });

    let mut persist_order = move |from: usize, to: usize| {
        let Some(mut list) = links.peek().clone() else {
            return;
        };
        let changed = reorder(&mut list, from, to);
        if changed.is_empty() {
            return;
        }
        links.set(Some(list));
        saving_order.set(true);
        spawn(async move {
            let mut failed = None;
            for link in changed {
                if let Err(e) = crate::api::friends::update_friend_link(
                    link.id,
                    link.name,
                    link.url,
                    link.avatar_url,
                    link.description,
                    link.sort_order,
                    link.is_active,
                )
                .await
                {
                    failed = Some(e);
                    break;
                }
            }
            match failed {
                None => toast.success("顺序已保存"),
                Some(e) => {
                    toast.error(failure_message("顺序保存失败", &e));
                    reload += 1;
                }
            }
            saving_order.set(false);
        });
    };

    let list = links();
    let editing_id = editing().map(|l| l.id);
    let next_order = list
        .as_ref()
        .and_then(|l| l.iter().map(|f| f.sort_order).max())
        .map_or(SORT_STEP, |max| max + SORT_STEP);

    rsx! {
        div { class: "admin-friends",
            div { class: "admin-page-head",
                div {
                    h1 { "友链" }
                    p { "维护前台 /friends 展示的友链。拖动把手，或在把手上按 ↑ / ↓ 调整顺序。" }
                }
                div { class: "admin-page-actions",
                    a {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                        href: "/friends",
                        target: "_blank",
                        rel: "noopener",
                        "查看前台页面"
                        Icon { name: IconName::ArrowUpRight, size: 14 }
                    }
                }
            }
            div { class: "admin-friends-grid",
                div { class: "admin-friends-list-col",
                    if error() {
                        LoadError { title: "友链加载失败", on_retry: move |_| reload += 1 }
                    } else if let Some(items) = list.clone() {
                        if items.is_empty() {
                            div { class: "admin-panel",
                                EmptyState {
                                    icon: IconName::Link,
                                    title: "还没有友链",
                                    description: "在右侧填写对方站点的名称和链接，添加第一条。",
                                }
                            }
                        } else {
                            ol {
                                class: "admin-panel admin-friends-list",
                                "aria-busy": saving_order().then_some("true"),
                                for (index, link) in items.iter().cloned().enumerate() {
                                    FriendRow {
                                        key: "{link.id}",
                                        index,
                                        count: items.len(),
                                        editing: editing_id == Some(link.id),
                                        dragging: dragging() == Some(index),
                                        drop_target: drop_at() == Some(index) && dragging().is_some_and(|d| d != index),
                                        on_edit: move |link: FriendLink| editing.set(Some(link)),
                                        on_move: move |(from, to): (usize, usize)| persist_order(from, to),
                                        on_drag: move |state: Option<usize>| {
                                            dragging.set(state);
                                            if state.is_none() {
                                                drop_at.set(None);
                                            }
                                        },
                                        on_over: move |i: usize| drop_at.set(Some(i)),
                                        on_drop: move |to: usize| {
                                            if let Some(from) = dragging() {
                                                persist_order(from, to);
                                            }
                                            dragging.set(None);
                                            drop_at.set(None);
                                        },
                                        link,
                                    }
                                }
                            }
                        }
                    } else {
                        FriendsAdminListSkeleton {}
                    }
                }
                for key in std::iter::once(editing_id.unwrap_or(0)) {
                    FriendForm {
                        key: "{key}",
                        link: editing(),
                        next_order,
                        on_saved: move |_| {
                            editing.set(None);
                            reload += 1;
                        },
                        on_new: move |_| editing.set(None),
                    }
                }
            }
        }
    }
}

/// 列表的一行：把手、头像、名称与状态、主机、排序值与编辑。
#[component]
#[allow(clippy::too_many_arguments)]
fn FriendRow(
    link: FriendLink,
    index: usize,
    count: usize,
    editing: bool,
    dragging: bool,
    drop_target: bool,
    on_edit: EventHandler<FriendLink>,
    on_move: EventHandler<(usize, usize)>,
    on_drag: EventHandler<Option<usize>>,
    on_over: EventHandler<usize>,
    on_drop: EventHandler<usize>,
) -> Element {
    let host = host_of(&link.url);
    let initial = initial_of(&link.name);
    let mut avatar_failed = use_signal(|| false);
    let edit_link = link.clone();
    rsx! {
        li {
            class: "admin-friend",
            "data-editing": editing.then_some("true"),
            "data-dragging": dragging.then_some("true"),
            "data-drop": drop_target.then_some("true"),
            "data-paused": (!link.is_active).then_some("true"),
            draggable: "true",
            ondragstart: move |_| on_drag.call(Some(index)),
            ondragend: move |_| on_drag.call(None),
            ondragover: move |e| {
                e.prevent_default();
                on_over.call(index);
            },
            ondrop: move |e| {
                e.prevent_default();
                on_drop.call(index);
            },
            button {
                class: "admin-friend-grip",
                r#type: "button",
                aria_label: "调整「{link.name}」的顺序，当前第 {index + 1} 位",
                title: "拖动，或按 ↑ / ↓",
                onkeydown: move |e| {
                    let to = match e.key() {
                        Key::ArrowUp if index > 0 => Some(index - 1),
                        Key::ArrowDown if index + 1 < count => Some(index + 1),
                        _ => None,
                    };
                    if let Some(to) = to {
                        e.prevent_default();
                        on_move.call((index, to));
                    }
                },
                Icon { name: IconName::GripVertical, size: 15 }
            }
            span { class: "admin-friend-avatar", aria_hidden: "true",
                match link.avatar_url.as_deref().filter(|u| !u.trim().is_empty() && !avatar_failed()) {
                    Some(url) => rsx! {
                        img { src: "{url}", alt: "", loading: "lazy", onerror: move |_| avatar_failed.set(true) }
                    },
                    None => rsx! { "{initial}" },
                }
            }
            div { class: "admin-friend-text",
                div { class: "admin-friend-name",
                    span { "{link.name}" }
                    if editing {
                        span { class: "admin-friend-tag", "data-tone": "clay", "编辑中" }
                    }
                    if !link.is_active {
                        span { class: "admin-friend-tag", "已停用" }
                    }
                }
                a { class: "admin-friend-host", href: "{link.url}", target: "_blank", rel: "noopener", "{host}" }
            }
            span { class: "admin-friend-order", title: "排序值", "{link.sort_order}" }
            button {
                class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                r#type: "button",
                onclick: move |_| on_edit.call(edit_link.clone()),
                "编辑"
            }
        }
    }
}

/// 添加 / 编辑表单。`link` 为 `None` 时为添加；按 `key` 重建以回填。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn FriendForm(
    link: Option<FriendLink>,
    next_order: i32,
    on_saved: EventHandler<()>,
    on_new: EventHandler<()>,
) -> Element {
    let toast = use_toast();
    let editing = link.is_some();
    let id = link.as_ref().map(|l| l.id);
    let mut name = use_signal(|| link.as_ref().map(|l| l.name.clone()).unwrap_or_default());
    let mut url = use_signal(|| link.as_ref().map(|l| l.url.clone()).unwrap_or_default());
    let mut avatar = use_signal(|| {
        link.as_ref()
            .and_then(|l| l.avatar_url.clone())
            .unwrap_or_default()
    });
    let mut desc = use_signal(|| {
        link.as_ref()
            .map(|l| l.description.clone())
            .unwrap_or_default()
    });
    let mut order = use_signal(|| {
        link.as_ref()
            .map_or(next_order, |l| l.sort_order)
            .to_string()
    });
    let mut active = use_signal(|| link.as_ref().is_none_or(|l| l.is_active));
    let mut busy = use_signal(|| false);
    let mut picker = use_signal(|| false);
    let picker_uploading = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);
    let mut avatar_failed = use_signal(|| false);

    let order_value = order().trim().parse::<i32>().ok();
    let can_submit =
        !busy() && !name().trim().is_empty() && !url().trim().is_empty() && order_value.is_some();

    let submit = move |e: FormEvent| {
        e.prevent_default();
        let Some(sort) = order().trim().parse::<i32>().ok() else {
            return;
        };
        if busy() {
            return;
        }
        busy.set(true);
        let (n, u, d, is_active) = (
            name().trim().to_string(),
            url().trim().to_string(),
            desc().trim().to_string(),
            active(),
        );
        let av = Some(avatar().trim().to_string()).filter(|s| !s.is_empty());
        spawn(async move {
            let result = match id {
                Some(id) => {
                    crate::api::friends::update_friend_link(id, n, u, av, d, sort, is_active)
                        .await
                        .map(|_| ())
                }
                None => match crate::api::friends::create_friend_link(
                    n.clone(),
                    u.clone(),
                    av.clone(),
                    d.clone(),
                    sort,
                )
                .await
                {
                    // 新建接口默认启用；选择不展示时再写一次状态。
                    Ok(created) if !is_active => crate::api::friends::update_friend_link(
                        created.id, n, u, av, d, sort, false,
                    )
                    .await
                    .map(|_| ()),
                    Ok(_) => Ok(()),
                    Err(e) => Err(e),
                },
            };
            match result {
                Ok(()) => {
                    toast.success(if id.is_some() {
                        "友链已保存"
                    } else {
                        "友链已添加"
                    });
                    on_saved.call(());
                }
                Err(e) => toast.error(failure_message("保存失败", &e)),
            }
            busy.set(false);
        });
    };
    let delete = move |_| {
        let Some(id) = id else {
            return;
        };
        busy.set(true);
        spawn(async move {
            match crate::api::friends::delete_friend_link(id).await {
                Ok(()) => {
                    toast.success("友链已删除");
                    on_saved.call(());
                }
                Err(e) => toast.error(failure_message("删除失败", &e)),
            }
            busy.set(false);
        });
    };

    rsx! {
        form { class: "admin-friend-form", aria_labelledby: "friend-form-h", onsubmit: submit,
            div { class: "admin-friend-form-head",
                h2 { id: "friend-form-h", if editing { "编辑友链" } else { "添加友链" } }
                if editing {
                    button {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                        r#type: "button",
                        onclick: move |_| on_new.call(()),
                        "改为新建"
                    }
                }
            }
            div {
                label { class: "admin-friend-label", r#for: "friend-name", "站点名称" }
                input {
                    id: "friend-name",
                    class: "field",
                    placeholder: "如：Rua Lab",
                    maxlength: 100,
                    value: "{name}",
                    oninput: move |e| name.set(e.value()),
                }
            }
            div {
                label { class: "admin-friend-label", r#for: "friend-url", "链接" }
                input {
                    id: "friend-url",
                    class: "field admin-friend-mono",
                    r#type: "url",
                    placeholder: "https://",
                    value: "{url}",
                    oninput: move |e| url.set(e.value()),
                }
            }
            div {
                label { class: "admin-friend-label", r#for: "friend-avatar", "头像" }
                div { class: "admin-friend-avatar-row",
                    span { class: "admin-friend-avatar", aria_hidden: "true",
                        if avatar().trim().is_empty() || avatar_failed() {
                            "{initial_of(&name())}"
                        } else {
                            img { src: "{avatar}", alt: "", onerror: move |_| avatar_failed.set(true) }
                        }
                    }
                    input {
                        id: "friend-avatar",
                        class: "field",
                        placeholder: "粘贴外链或从素材库选择",
                        value: "{avatar}",
                        oninput: move |e| {
                            avatar.set(e.value());
                            avatar_failed.set(false);
                        },
                    }
                    button {
                        class: button_class(ButtonVariant::Secondary, ButtonSize::Md),
                        r#type: "button",
                        onclick: move |_| picker.set(true),
                        "素材库"
                    }
                }
            }
            div {
                label { class: "admin-friend-label", r#for: "friend-desc", "简介" }
                textarea {
                    id: "friend-desc",
                    class: "field field-textarea",
                    rows: "3",
                    maxlength: 300,
                    placeholder: "一句话介绍对方站点",
                    value: "{desc}",
                    oninput: move |e| desc.set(e.value()),
                }
            }
            div { class: "admin-friend-form-row",
                div { class: "admin-friend-order-field",
                    label { class: "admin-friend-label", r#for: "friend-order", "排序值" }
                    input {
                        id: "friend-order",
                        class: "field admin-friend-mono",
                        inputmode: "numeric",
                        "aria-invalid": order_value.is_none().then_some("true"),
                        value: "{order}",
                        oninput: move |e| order.set(e.value()),
                    }
                }
                div { class: "admin-friend-switch",
                    ToggleSwitch {
                        checked: active(),
                        aria_label: "在前台展示".to_string(),
                        ontoggle: move |_| active.toggle(),
                    }
                    span { "在前台展示" }
                }
            }
            if order_value.is_none() {
                p { class: "admin-friend-error", role: "alert", "排序值须为整数，越小越靠前。" }
            }
            div { class: "admin-friend-actions",
                button {
                    class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                    r#type: "submit",
                    disabled: !can_submit,
                    if busy() {
                        "保存中…"
                    } else if editing {
                        "保存修改"
                    } else {
                        "添加"
                    }
                }
                if editing {
                    button {
                        class: button_class(ButtonVariant::Outline, ButtonSize::Md),
                        "data-danger": "true",
                        r#type: "button",
                        disabled: busy(),
                        onclick: move |_| confirm_delete.set(true),
                        "删除"
                    }
                }
            }
        }
        AssetPickerModal {
            visible: picker,
            cover_uploading: picker_uploading,
            title: "选择头像",
            on_select: move |picks: Vec<AssetSelection>| {
                if let Some(first) = picks.into_iter().next() {
                    avatar.set(first.url);
                    avatar_failed.set(false);
                }
            },
        }
        ConfirmDialog {
            visible: confirm_delete,
            title: "删除这条友链？",
            description: format!("「{}」会从友链页移除，无法恢复。只想暂时隐藏可以关闭「在前台展示」。", name()),
            confirm_label: "删除",
            on_confirm: delete,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(id: i32, order: i32) -> FriendLink {
        FriendLink {
            id,
            name: format!("站点 {id}"),
            url: format!("https://{id}.example.com"),
            avatar_url: None,
            description: String::new(),
            sort_order: order,
            is_active: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn reorder_moves_item_and_rewrites_only_changed_orders() {
        let mut list = vec![link(1, 10), link(2, 20), link(3, 30)];
        let changed = reorder(&mut list, 2, 0);
        assert_eq!(list.iter().map(|l| l.id).collect::<Vec<_>>(), [3, 1, 2]);
        assert_eq!(
            list.iter().map(|l| l.sort_order).collect::<Vec<_>>(),
            [10, 20, 30]
        );
        assert_eq!(changed.iter().map(|l| l.id).collect::<Vec<_>>(), [3, 1, 2]);
        assert!(reorder(&mut list, 1, 1).is_empty());
    }

    #[test]
    fn host_and_initial_are_display_friendly() {
        assert_eq!(host_of("https://blog.example.dev/"), "blog.example.dev");
        assert_eq!(initial_of(" rua lab"), "R");
        assert_eq!(initial_of(""), "?");
    }
}
