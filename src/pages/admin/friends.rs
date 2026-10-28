//! 管理后台「友链」页面（`/admin/friends`）。
//!
//! 左侧为友链列表：拖动把手或在把手上按 ↑ / ↓ 调整顺序，顺序按 10、20、30… 写回
//! 排序值；右侧为添加 / 编辑表单（名称、链接、头像、简介、排序值、是否在前台展示），
//! 编辑时可删除（确认对话框，物理删除）。写操作成功后服务端失效友链缓存与 `/friends`
//! SSR 缓存。数据只在 WASM 前端读取（Effect + 世代号）。样式见 styles/pages/admin-friends.css。

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{lazy_route, LazyRoute};

use crate::components::admin_layout::AdminPageHead;
use crate::components::assets::{AssetPickerModal, AssetSelection};
use crate::components::empty_state::EmptyState;
use crate::components::forms::{FormInput, FormLabel, FormTextarea, ToggleSwitch};
use crate::components::skeletons::friends_admin_skeleton::FriendsAdminListSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, ConfirmDialog, Icon, IconName, LoadError,
};
use crate::models::friend_link::FriendLink;
use crate::utils::error::failure_message;

/// 排序值步长：给手动插队留出空位。
const SORT_STEP: i32 = 10;

/// 拖动状态：正在拖的行下标（宏属性里不便直接写泛型）。
type DragState = Option<usize>;

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

/// 友链管理路由（无 SSR 数据，列表在客户端读取）。
pub struct FriendsAdminRoute;

#[lazy_route]
impl LazyRoute for FriendsAdminRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <FriendsAdmin/> }.into_any()
    }
}

/// 列表行的重建键：内容或位置变化时重建该行，拖动等瞬时状态不在其中。
fn row_key(index: usize, link: &FriendLink) -> String {
    format!(
        "{index}|{}|{}|{}|{}|{}|{}|{}",
        link.id,
        link.sort_order,
        link.is_active,
        link.name,
        link.url,
        link.avatar_url.as_deref().unwrap_or_default(),
        link.description,
    )
}

/// 管理后台友链页面。
#[component]
fn FriendsAdmin() -> impl IntoView {
    let toast = use_toast();
    let links = RwSignal::new(None::<Vec<FriendLink>>);
    let error = RwSignal::new(false);
    let reload = RwSignal::new(0_u32);
    let generation = StoredValue::new(0_u64);
    let editing = RwSignal::new(None::<FriendLink>);
    // 拖动中的行与当前悬停的目标行。
    let dragging = RwSignal::new(None::<usize>);
    let drop_at = RwSignal::new(None::<usize>);
    let saving_order = RwSignal::new(false);

    Effect::new(move |_| {
        reload.track();
        let mut id = 0;
        generation.update_value(|g| {
            *g += 1;
            id = *g;
        });
        spawn_local(async move {
            let result = crate::api::friends::list_all_friend_links().await;
            if generation.try_get_value() != Some(id) {
                return;
            }
            match result {
                Ok(list) => {
                    // 编辑中的友链若已被删除，退回新建。
                    let current = editing.with_untracked(|l| l.as_ref().map(|l| l.id));
                    if current.is_some_and(|id| !list.iter().any(|l| l.id == id)) {
                        let _ = editing.try_set(None);
                    }
                    let _ = links.try_set(Some(list));
                    let _ = error.try_set(false);
                }
                Err(_) => {
                    let _ = error.try_set(true);
                }
            }
        });
    });

    let persist_order = Callback::new(move |(from, to): (usize, usize)| {
        let Some(mut list) = links.get_untracked() else {
            return;
        };
        let changed = reorder(&mut list, from, to);
        if changed.is_empty() {
            return;
        }
        links.set(Some(list));
        saving_order.set(true);
        spawn_local(async move {
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
                    let _ = reload.try_update(|n| *n += 1);
                }
            }
            let _ = saving_order.try_set(false);
        });
    });

    let editing_id = Memo::new(move |_| editing.with(|l| l.as_ref().map(|l| l.id)));
    let next_order = Memo::new(move |_| {
        links.with(|list| {
            list.as_ref()
                .and_then(|l| l.iter().map(|f| f.sort_order).max())
                .map_or(SORT_STEP, |max| max + SORT_STEP)
        })
    });
    let count = Memo::new(move |_| links.with(|l| l.as_ref().map_or(0, Vec::len)));
    let rows = Memo::new(move |_| links.get().unwrap_or_default());

    view! {
        <div class="admin-friends">
            <AdminPageHead
                title="友链"
                actions=|| {
                    view! {
                        <a
                            class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                            href="/friends"
                            target="_blank"
                            rel="noopener"
                        >
                            "查看前台页面"
                            <Icon name=IconName::ArrowUpRight size=14/>
                        </a>
                    }
                }
            >
                "维护前台 /friends 展示的友链。拖动把手，或在把手上按 ↑ / ↓ 调整顺序。"
            </AdminPageHead>
            <div class="admin-friends-grid">
                <div class="admin-friends-list-col">
                    {move || {
                        if error.get() {
                            view! {
                                <LoadError
                                    title="友链加载失败"
                                    on_retry=move |()| reload.update(|n| *n += 1)
                                />
                            }
                                .into_any()
                        } else if links.with(Option::is_none) {
                            view! { <FriendsAdminListSkeleton/> }.into_any()
                        } else if count.get() == 0 {
                            view! {
                                <div class="admin-panel">
                                    <EmptyState
                                        icon=IconName::Link
                                        title="还没有友链"
                                        description="在右侧填写对方站点的名称和链接，添加第一条。"
                                    />
                                </div>
                            }
                                .into_any()
                        } else {
                            view! {
                                <ol
                                    class="admin-panel admin-friends-list"
                                    aria-busy=move || saving_order.get().then_some("true")
                                >
                                    <For
                                        each={move || {
                                            rows.get().into_iter().enumerate().collect::<Vec<_>>()
                                        }}
                                        key=|(index, link)| row_key(*index, link)
                                        children=move |(index, link)| {
                                            let link_id = link.id;
                                            view! {
                                                <FriendRow
                                                    link=link
                                                    index=index
                                                    count=count
                                                    editing=Signal::derive(move || {
                                                        editing_id.get() == Some(link_id)
                                                    })
                                                    dragging=Signal::derive(move || dragging.get() == Some(index))
                                                    drop_target=Signal::derive(move || {
                                                        drop_at.get() == Some(index)
                                                            && dragging.get().is_some_and(|d| d != index)
                                                    })
                                                    on_edit=move |link: FriendLink| editing.set(Some(link))
                                                    on_move=persist_order
                                                    on_drag=move |state: DragState| {
                                                        dragging.set(state);
                                                        if state.is_none() {
                                                            drop_at.set(None);
                                                        }
                                                    }
                                                    on_over=move |i: usize| drop_at.set(Some(i))
                                                    on_drop=move |to: usize| {
                                                        if let Some(from) = dragging.get_untracked() {
                                                            persist_order.run((from, to));
                                                        }
                                                        dragging.set(None);
                                                        drop_at.set(None);
                                                    }
                                                />
                                            }
                                        }
                                    />
                                </ol>
                            }
                                .into_any()
                        }
                    }}
                </div>
                // 切换编辑对象时按 id 重建表单以回填。
                {move || {
                    let _key = editing_id.get().unwrap_or(0);
                    view! {
                        <FriendForm
                            link=editing.get_untracked()
                            next_order=next_order.get_untracked()
                            on_saved=move |()| {
                                editing.set(None);
                                reload.update(|n| *n += 1);
                            }
                            on_new=move |()| editing.set(None)
                        />
                    }
                }}
            </div>
        </div>
    }
}

/// 列表的一行：把手、头像、名称与状态、主机、排序值与编辑。
#[component]
#[allow(clippy::too_many_arguments)]
fn FriendRow(
    link: FriendLink,
    index: usize,
    #[prop(into)] count: Signal<usize>,
    #[prop(into)] editing: Signal<bool>,
    #[prop(into)] dragging: Signal<bool>,
    #[prop(into)] drop_target: Signal<bool>,
    #[prop(into)] on_edit: Callback<FriendLink>,
    #[prop(into)] on_move: Callback<(usize, usize)>,
    #[prop(into)] on_drag: Callback<Option<usize>>,
    #[prop(into)] on_over: Callback<usize>,
    #[prop(into)] on_drop: Callback<usize>,
) -> impl IntoView {
    let host = host_of(&link.url);
    let initial = initial_of(&link.name);
    let avatar_failed = RwSignal::new(false);
    let edit_link = link.clone();
    let paused = !link.is_active;
    let avatar_url = link.avatar_url.clone().filter(|u| !u.trim().is_empty());
    let grip_label = format!("调整「{}」的顺序，当前第 {} 位", link.name, index + 1);
    view! {
        <li
            class="admin-friend"
            data-editing=move || editing.get().then_some("true")
            data-dragging=move || dragging.get().then_some("true")
            data-drop=move || drop_target.get().then_some("true")
            data-paused=paused.then_some("true")
            draggable="true"
            on:dragstart=move |_| on_drag.run(Some(index))
            on:dragend=move |_| on_drag.run(None)
            on:dragover=move |e: leptos::ev::DragEvent| {
                e.prevent_default();
                on_over.run(index);
            }
            on:drop=move |e: leptos::ev::DragEvent| {
                e.prevent_default();
                on_drop.run(index);
            }
        >
            <button
                class="admin-friend-grip"
                type="button"
                aria-label=grip_label
                title="拖动，或按 ↑ / ↓"
                on:keydown=move |e: leptos::ev::KeyboardEvent| {
                    let to = match e.key().as_str() {
                        "ArrowUp" if index > 0 => Some(index - 1),
                        "ArrowDown" if index + 1 < count.get_untracked() => Some(index + 1),
                        _ => None,
                    };
                    if let Some(to) = to {
                        e.prevent_default();
                        on_move.run((index, to));
                    }
                }
            >
                <Icon name=IconName::GripVertical size=15/>
            </button>
            <span class="admin-friend-avatar" aria-hidden="true">
                {move || match avatar_url.clone().filter(|_| !avatar_failed.get()) {
                    Some(url) => {
                        view! {
                            <img
                                src=url
                                alt=""
                                loading="lazy"
                                on:error=move |_| avatar_failed.set(true)
                            />
                        }
                            .into_any()
                    }
                    None => initial.clone().into_any(),
                }}
            </span>
            <div class="admin-friend-text">
                <div class="admin-friend-name">
                    <span>{link.name.clone()}</span>
                    {move || {
                        editing
                            .get()
                            .then(|| {
                                view! {
                                    <span class="admin-friend-tag" data-tone="clay">
                                        "编辑中"
                                    </span>
                                }
                            })
                    }}
                    {paused.then(|| view! { <span class="admin-friend-tag">"已停用"</span> })}
                </div>
                <a
                    class="admin-friend-host"
                    href=link.url.clone()
                    target="_blank"
                    rel="noopener"
                >
                    {host}
                </a>
            </div>
            <span class="admin-friend-order" title="排序值">
                {link.sort_order}
            </span>
            <button
                class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                type="button"
                on:click=move |_| on_edit.run(edit_link.clone())
            >
                "编辑"
            </button>
        </li>
    }
}

/// 添加 / 编辑表单。`link` 为 `None` 时为添加；调用方按编辑对象重建以回填。
#[component]
fn FriendForm(
    link: Option<FriendLink>,
    next_order: i32,
    #[prop(into)] on_saved: Callback<()>,
    #[prop(into)] on_new: Callback<()>,
) -> impl IntoView {
    let toast = use_toast();
    let editing = link.is_some();
    let id = link.as_ref().map(|l| l.id);
    let name = RwSignal::new(link.as_ref().map(|l| l.name.clone()).unwrap_or_default());
    let url = RwSignal::new(link.as_ref().map(|l| l.url.clone()).unwrap_or_default());
    let avatar = RwSignal::new(
        link.as_ref()
            .and_then(|l| l.avatar_url.clone())
            .unwrap_or_default(),
    );
    let desc = RwSignal::new(
        link.as_ref()
            .map(|l| l.description.clone())
            .unwrap_or_default(),
    );
    let order = RwSignal::new(
        link.as_ref()
            .map_or(next_order, |l| l.sort_order)
            .to_string(),
    );
    let active = RwSignal::new(link.as_ref().is_none_or(|l| l.is_active));
    let busy = RwSignal::new(false);
    let picker = RwSignal::new(false);
    let picker_uploading = RwSignal::new(false);
    let confirm_delete = RwSignal::new(false);
    let avatar_failed = RwSignal::new(false);

    let order_value = Memo::new(move |_| order.with(|o| o.trim().parse::<i32>().ok()));
    let can_submit = Memo::new(move |_| {
        !busy.get()
            && name.with(|n| !n.trim().is_empty())
            && url.with(|u| !u.trim().is_empty())
            && order_value.get().is_some()
    });

    let submit = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        let Some(sort) = order_value.get_untracked() else {
            return;
        };
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let (n, u, d, is_active) = (
            name.get_untracked().trim().to_string(),
            url.get_untracked().trim().to_string(),
            desc.get_untracked().trim().to_string(),
            active.get_untracked(),
        );
        let av = Some(avatar.get_untracked().trim().to_string()).filter(|s| !s.is_empty());
        spawn_local(async move {
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
                    on_saved.run(());
                }
                Err(e) => toast.error(failure_message("保存失败", &e)),
            }
            let _ = busy.try_set(false);
        });
    };
    let delete = Callback::new(move |()| {
        let Some(id) = id else {
            return;
        };
        busy.set(true);
        spawn_local(async move {
            match crate::api::friends::delete_friend_link(id).await {
                Ok(()) => {
                    toast.success("友链已删除");
                    on_saved.run(());
                }
                Err(e) => toast.error(failure_message("删除失败", &e)),
            }
            let _ = busy.try_set(false);
        });
    });

    // 带泛型的闭包参数类型放在 `view!` 外，避免宏把 `<` 当作标签。
    let on_pick = move |picks: Vec<AssetSelection>| {
        if let Some(first) = picks.into_iter().next() {
            avatar.set(first.url);
            avatar_failed.set(false);
        }
    };

    view! {
        <form class="admin-friend-form" aria-labelledby="friend-form-h" on:submit=submit>
            <div class="admin-friend-form-head">
                <h2 id="friend-form-h">{if editing { "编辑友链" } else { "添加友链" }}</h2>
                {editing
                    .then(|| {
                        view! {
                            <button
                                class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                type="button"
                                on:click=move |_| on_new.run(())
                            >
                                "改为新建"
                            </button>
                        }
                    })}
            </div>
            <div>
                <FormLabel label="站点名称" html_for="friend-name".to_string()/>
                <FormInput
                    id="friend-name"
                    placeholder="如：Rua Lab"
                    maxlength=100
                    value=name
                    oninput=move |v: String| name.set(v)
                />
            </div>
            <div>
                <FormLabel label="链接" html_for="friend-url".to_string()/>
                <FormInput
                    id="friend-url"
                    class="field admin-friend-mono"
                    r#type="url"
                    placeholder="https://"
                    value=url
                    oninput=move |v: String| url.set(v)
                />
            </div>
            <div>
                <FormLabel label="头像" html_for="friend-avatar".to_string()/>
                <div class="admin-friend-avatar-row">
                    <span class="admin-friend-avatar" aria-hidden="true">
                        {move || {
                            if avatar.with(|a| a.trim().is_empty()) || avatar_failed.get() {
                                name.with(|n| initial_of(n)).into_any()
                            } else {
                                view! {
                                    <img
                                        src=avatar.get()
                                        alt=""
                                        on:error=move |_| avatar_failed.set(true)
                                    />
                                }
                                    .into_any()
                            }
                        }}
                    </span>
                    <FormInput
                        id="friend-avatar"
                        placeholder="粘贴外链或从素材库选择"
                        value=avatar
                        oninput=move |v: String| {
                            avatar.set(v);
                            avatar_failed.set(false);
                        }
                    />
                    <button
                        class=button_class(ButtonVariant::Secondary, ButtonSize::Md)
                        type="button"
                        on:click=move |_| picker.set(true)
                    >
                        "素材库"
                    </button>
                </div>
            </div>
            <div>
                <FormLabel label="简介" html_for="friend-desc".to_string()/>
                <FormTextarea
                    id="friend-desc"
                    rows=3
                    maxlength=300
                    placeholder="一句话介绍对方站点"
                    value=desc
                    oninput=move |v: String| desc.set(v)
                />
            </div>
            <div class="admin-friend-form-row">
                <div class="admin-friend-order-field">
                    <FormLabel label="排序值" html_for="friend-order".to_string()/>
                    <FormInput
                        id="friend-order"
                        class="field admin-friend-mono"
                        inputmode="numeric"
                        invalid=Signal::derive(move || order_value.get().is_none())
                        value=order
                        oninput=move |v: String| order.set(v)
                    />
                </div>
                <div class="admin-friend-switch">
                    <ToggleSwitch
                        checked=active
                        aria_label="在前台展示".to_string()
                        ontoggle=move |()| active.update(|a| *a = !*a)
                    />
                    <span>"在前台展示"</span>
                </div>
            </div>
            {move || {
                order_value
                    .get()
                    .is_none()
                    .then(|| {
                        view! {
                            <p class="admin-friend-error" role="alert">
                                "排序值须为整数，越小越靠前。"
                            </p>
                        }
                    })
            }}
            <div class="admin-friend-actions">
                <button
                    class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                    type="submit"
                    disabled=move || !can_submit.get()
                >
                    {move || {
                        if busy.get() {
                            "保存中…"
                        } else if editing {
                            "保存修改"
                        } else {
                            "添加"
                        }
                    }}
                </button>
                {editing
                    .then(|| {
                        view! {
                            <button
                                class=button_class(ButtonVariant::Outline, ButtonSize::Md)
                                data-danger="true"
                                type="button"
                                disabled=move || busy.get()
                                on:click=move |_| confirm_delete.set(true)
                            >
                                "删除"
                            </button>
                        }
                    })}
            </div>
        </form>
        <AssetPickerModal
            visible=picker
            cover_uploading=picker_uploading
            title="选择头像"
            on_select=on_pick
        />
        <ConfirmDialog
            visible=confirm_delete
            title="删除这条友链？"
            description=Signal::derive(move || {
                format!(
                    "「{}」会从友链页移除，无法恢复。只想暂时隐藏可以关闭「在前台展示」。",
                    name.get(),
                )
            })
            confirm_label="删除"
            on_confirm=delete
        />
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
