//! Pagination, tabs and navigable tag chips.
use super::icons::{Icon, IconName};
use crate::components::forms::FormInput;
use dioxus::prelude::*;
use dioxus::router::components::Link;
/// 分页导航组件。
///
/// 统一了后台与前台的分页 UI，通过 `variant` 切换配色与展示细节：
/// - `"admin"`：小号描边按钮，显示页码计数
///   （`{当前} / {总} 页 (共 {total} {unit})`），首尾页渲染禁用态。
/// - `"frontend"`：小号描边按钮与居中页码，首尾页不渲染越界按钮。
///
/// Props：
/// - `variant`：`"admin"` 或 `"frontend"`
/// - `current_page`：当前页码（从 1 开始）
/// - `total`：数据总条数
/// - `per_page`：每页条数，用于计算总页数
/// - `prev_route`：点击上一页跳转的目标路由（路由式翻页用；回调式翻页可不传，默认 `None`）
/// - `next_route`：点击下一页跳转的目标路由（路由式翻页用；回调式翻页可不传，默认 `None`）
/// - `unit`：计数单位（"篇" / "条"），仅 admin 显示计数时使用
/// - `on_prev` / `on_next`：可选回调。传入时渲染 `<button onclick>` 走客户端
///   signal 翻页（与路由式 `prev_route`/`next_route` 互斥，回调优先）；不传则保持
///   原路由式 `<Link>`。后台客户端分页列表（如「全部文章」）用回调翻页，
///   前台/其他分页页面仍走路由，零影响。
/// - `on_jump`：可选跳页回调。传入且总页数 > 1 时，admin 变体的当前页码渲染为
///   可编辑输入框——聚焦编辑、回车跳转（自动夹取到 `[1, total_pages]`，非法输入
///   还原为当前页）、失焦回显。仅回调式翻页可用——路由式翻页无法由任意页码反推
///   `Route`，故路由式调用方不传此 prop，页码保持纯文本。
/// - `compact`：admin 变体是否去掉默认外边距，供弹窗等固定底栏嵌入使用。
#[component]
pub fn Pagination<R: Routable + Clone + PartialEq + 'static>(
    variant: &'static str,
    current_page: i32,
    total: i64,
    per_page: i32,
    #[props(default)] prev_route: Option<R>,
    #[props(default)] next_route: Option<R>,
    unit: &'static str,
    #[props(default)] on_prev: Option<EventHandler<()>>,
    #[props(default)] on_next: Option<EventHandler<()>>,
    #[props(default)] on_jump: Option<EventHandler<i32>>,
    #[props(default)] compact: bool,
) -> Element {
    let has_prev = current_page > 1;
    let total_pages = ((total + per_page as i64 - 1) / per_page as i64).max(1) as i32;
    let has_next = current_page < total_pages;

    let is_admin = variant == "admin";
    let nav_class = if is_admin {
        if compact {
            "admin-pagination"
        } else {
            "admin-pagination mt-6"
        }
    } else {
        "frontend-pagination"
    };
    let link_class = "btn btn-sm btn-outline";
    let link_extra_next = if is_admin {
        ""
    } else {
        "col-start-3 row-start-1 justify-self-end"
    };

    // admin 首尾页渲染禁用态；frontend 首尾页直接不渲染。
    // on_prev/on_next 存在时走回调式 button（客户端 signal 翻页），否则走路由式 Link。
    let prev_inner = rsx! {
        Icon { name: IconName::ChevronLeft, size: 15 }
        "上一页"
    };
    let next_inner = rsx! {
        "下一页"
        Icon { name: IconName::ChevronRight, size: 15 }
    };

    // 跳页状态：当前页码本身渲染为可编辑输入框（仅回调式翻页且多页时）。
    // `jump_editing` = 输入框聚焦态——聚焦期间显示草稿，失焦回显 current_page，
    // 与父组件翻页天然同步，无需 effect 桥接；`jump_draft` = 用户输入草稿。
    // 两者皆为独立 UI 状态（非派生镜像），合法 use_signal。
    let mut jump_editing: Signal<bool> = use_signal(|| false);
    let mut jump_draft: Signal<String> = use_signal(String::new);
    rsx! {
        nav { class: nav_class, aria_label: "分页导航",
            if has_prev {
                if let Some(on_prev) = on_prev {
                    button {
                        class: "{link_class}",
                        onclick: move |_| on_prev.call(()),
                        {prev_inner}
                    }
                } else if let Some(pr) = prev_route.clone() {
                    Link { class: "{link_class}", to: pr, {prev_inner} }
                }
            } else if is_admin {
                span { class: "{link_class}", aria_disabled: "true",
                    Icon { name: IconName::ChevronLeft, size: 15 }
                    "上一页"
                }
            }

            // admin 显示页码计数；回调式翻页且多页时，当前页码可直接编辑跳页。
            if is_admin {
                span { class: "flex items-center gap-1.5 text-sm text-fg-2 whitespace-nowrap tabular-nums",
                    if total_pages > 1 && on_jump.is_some() {
                        FormInput {
                            r#type: "text",
                            placeholder: "",
                            value: if jump_editing() { jump_draft() } else { current_page.to_string() },
                            class: Some("field field-sm w-12 text-center tabular-nums"),
                            inputmode: Some("numeric"),
                            title: Some("输入页码，回车跳转"),
                            oninput: move |v: String| jump_draft.set(v),
                            onfocus: move |_| {
                                jump_draft.set(current_page.to_string());
                                jump_editing.set(true);
                            },
                            onblur: move |_| jump_editing.set(false),
                            onkeydown: move |e: KeyboardEvent| {
                                if e.key() == Key::Enter {
                                    if let Some(on_jump) = on_jump {
                                        // 合法页码：夹取到 [1, total_pages] 后回调并回显；
                                        // 非法输入（空串/非数字）：还原为当前页。
                                        if let Ok(p) = jump_draft().trim().parse::<i32>() {
                                            let p = p.clamp(1, total_pages);
                                            on_jump.call(p);
                                            jump_draft.set(p.to_string());
                                        } else {
                                            jump_draft.set(current_page.to_string());
                                        }
                                    }
                                }
                            },
                        }
                    } else {
                        "{current_page}"
                    }
                    " / {total_pages} 页 (共 {total} {unit})"
                }
            } else {
                span {
                    class: "col-start-2 row-start-1 text-xs text-fg-3 whitespace-nowrap tabular-nums",
                    aria_label: "第 {current_page} 页，共 {total_pages} 页",
                    aria_current: "page",
                    "{current_page} / {total_pages}"
                }
            }

            if has_next {
                if let Some(on_next) = on_next {
                    button {
                        class: "{link_class} {link_extra_next}",
                        onclick: move |_| on_next.call(()),
                        {next_inner}
                    }
                } else if let Some(nr) = next_route.clone() {
                    Link {
                        class: "{link_class} {link_extra_next}",
                        to: nr,
                        {next_inner}
                    }
                }
            } else if is_admin {
                span { class: "{link_class}", aria_disabled: "true",
                    "下一页"
                    Icon { name: IconName::ChevronRight, size: 15 }
                }
            }
        }
    }
}

static TAB_GROUP_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 在选项卡之间循环移动（←/→、Home/End 用）。
fn neighbor_tab<'a>(items: &[(&'a str, &'a str)], active: &str, delta: i32) -> Option<&'a str> {
    let len = items.len() as i32;
    if len == 0 {
        return None;
    }
    let cur = items.iter().position(|(v, _)| *v == active).unwrap_or(0) as i32;
    Some(items[(cur + delta).rem_euclid(len) as usize].0)
}

/// 筛选选项卡（分段控件）。
///
/// 胶囊轨道里放一组选项，选中项是浮起的白色滑块，切换时以 snappy 弹簧滑到新位置。
/// 语义为 `tablist`：←/→ 在选项间移动并立即切换，Home/End 跳到首尾，焦点随之移动
/// （roving tabindex，Tab 只停在当前项）。滑块测量完成前，选中项直接画浮起底色，
/// 首屏不闪。
///
/// Props：
/// - `items`：选项卡列表，每一项为 `(value, label)`
/// - `active_value`：当前选中的值
/// - `on_change`：选项卡切换时的回调
/// - `aria_label`：选项卡组的无障碍名称（可选）
#[component]
pub fn FilterTabs(
    items: Vec<(&'static str, &'static str)>,
    active_value: String,
    on_change: EventHandler<String>,
    #[props(default)] aria_label: Option<&'static str>,
) -> Element {
    #[allow(unused_mut)]
    let mut thumb_style = use_signal(|| None::<String>);
    let id_prefix = use_hook(|| TAB_GROUP_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
    let update_thumb = move |active: String| {
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            {
                use wasm_bindgen::JsCast;
                // 等待 DOM 节点更新
                crate::utils::time::sleep_ms(30).await;
                if let Some(el) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id(&format!("tab-{id_prefix}-{active}")))
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                {
                    thumb_style.set(Some(format!(
                        "transform: translateX({}px); width: {}px;",
                        el.offset_left(),
                        el.offset_width()
                    )));
                }
            }
        });
    };
    use_effect({
        let active_value = active_value.clone();
        move || {
            update_thumb(active_value.clone());
        }
    });

    let measured = thumb_style().is_some();
    let thumb = thumb_style().unwrap_or_else(|| "opacity: 0;".to_string());
    let items_for_keys = items.clone();
    let active_for_keys = active_value.clone();

    rsx! {
        div { class: "mb-6",
            div {
                class: "segmented",
                role: "tablist",
                aria_label,
                "data-ready": measured.then_some("true"),
                onkeydown: move |e| {
                    let next = match e.key() {
                        Key::ArrowRight => neighbor_tab(&items_for_keys, &active_for_keys, 1),
                        Key::ArrowLeft => neighbor_tab(&items_for_keys, &active_for_keys, -1),
                        Key::Home => items_for_keys.first().map(|(v, _)| *v),
                        Key::End => items_for_keys.last().map(|(v, _)| *v),
                        _ => None,
                    };
                    if let Some(next) = next {
                        e.prevent_default();
                        on_change.call(next.to_string());
                        update_thumb(next.to_string());
                        #[cfg(target_arch = "wasm32")]
                        {
                            use wasm_bindgen::JsCast;
                            if let Some(el) = web_sys::window()
                                .and_then(|w| w.document())
                                .and_then(|d| d.get_element_by_id(&format!("tab-{id_prefix}-{next}")))
                                .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                            {
                                let _ = el.focus();
                            }
                        }
                    }
                },
                span { class: "segmented-thumb", aria_hidden: "true", style: "{thumb}" }
                for (value, label) in items {
                    button {
                        id: "tab-{id_prefix}-{value}",
                        key: "{value}",
                        r#type: "button",
                        class: "segmented-item",
                        role: "tab",
                        aria_selected: "{active_value == *value}",
                        tabindex: if active_value == *value { "0" } else { "-1" },
                        onclick: {
                            let v = value.to_string();
                            move |_| {
                                on_change.call(v.clone());
                                update_thumb(v.clone());
                            }
                        },
                        "{label}"
                    }
                }
            }
        }
    }
}

/// 面包屑中的一级。`to` 为 `None` 表示当前页（不可点击，标 `aria-current`）。
#[derive(Clone, PartialEq)]
pub struct Crumb<R: Clone + PartialEq> {
    pub label: String,
    pub to: Option<R>,
}

impl<R: Clone + PartialEq> Crumb<R> {
    pub fn link(label: impl Into<String>, to: R) -> Self {
        Self {
            label: label.into(),
            to: Some(to),
        }
    }

    pub fn current(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            to: None,
        }
    }
}

/// 面包屑：`关于 / 组件图鉴 / 复选框`，最后一级为当前页并在过长时截断。
#[component]
pub fn Breadcrumb<R: Routable + Clone + PartialEq + 'static>(items: Vec<Crumb<R>>) -> Element {
    let last = items.len().saturating_sub(1);
    rsx! {
        nav { class: "breadcrumb", aria_label: "面包屑",
            for (i, crumb) in items.into_iter().enumerate() {
                if i > 0 {
                    span { key: "sep-{i}", aria_hidden: "true", "/" }
                }
                match crumb.to {
                    Some(to) if i < last => rsx! {
                        Link { key: "{i}", to, "{crumb.label}" }
                    },
                    _ => rsx! {
                        span { key: "{i}", aria_current: "page", "{crumb.label}" }
                    },
                }
            }
        }
    }
}

/// 标签芯片组件：统一软底标签、卡片胶囊与文字链接的展示。
///
/// 收敛原本散落在标签云与文章卡片（`post_card.rs` 描边胶囊）
/// 的两套手写 `Link` 样式——两者都是跳转到 [`Route::TagDetail`] 的可点击标签，
/// 仅视觉变体不同，故合并为一个组件 + `variant` prop。
///
/// Props：
/// - `label`：标签名
/// - `to`：跳转目标路由（泛型，调用方传入具体 `Route` 变体，原子层不绑定 app 路由类型）
/// - `variant`：`"solid"`（软底）/ `"outline"`（卡片胶囊）/ `"text"`（紧凑文章流）
/// - `count`：可选的文章计数
/// - `stop_propagation`：是否阻止点击冒泡（卡片内覆盖层链接场景需要，见 `PostCard`）
#[component]
pub fn TagChip<R: Routable + Clone + PartialEq + 'static>(
    label: String,
    to: R,
    #[props(default = "outline")] variant: &'static str,
    #[props(default)] count: Option<i64>,
    #[props(default)] stop_propagation: bool,
) -> Element {
    let class = match variant {
        "solid" => "chip chip-clay",
        "text" => "inline-flex items-center py-1 text-fg-2 hover:text-fg underline decoration-hairline underline-offset-4 hover:decoration-clay transition-colors focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-clay",
        _ => "chip",
    };
    rsx! {
        Link {
            class: "{class}",
            to,
            onclick: move |evt: dioxus::events::MouseEvent| {
                if stop_propagation {
                    evt.stop_propagation();
                }
            },
            "{label}"
            if let Some(c) = count {
                span { class: "chip-count", "{c}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::neighbor_tab;

    #[test]
    fn neighbor_tab_wraps_in_both_directions() {
        let items = [("all", "全部"), ("pending", "待审核"), ("spam", "垃圾")];
        assert_eq!(neighbor_tab(&items, "all", 1), Some("pending"));
        assert_eq!(neighbor_tab(&items, "spam", 1), Some("all"));
        assert_eq!(neighbor_tab(&items, "all", -1), Some("spam"));
        // 当前值不在列表里时从第一项起算
        assert_eq!(neighbor_tab(&items, "missing", 1), Some("pending"));
        assert_eq!(neighbor_tab(&[], "all", 1), None);
    }
}
