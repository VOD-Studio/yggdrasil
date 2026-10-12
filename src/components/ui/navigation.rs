//! Pagination, tabs and navigable tag chips.
use super::styles::BTN_OUTLINE;
use crate::components::forms::FormInput;
use dioxus::prelude::*;
use dioxus::router::components::Link;
/// 分页导航组件。
///
/// 统一了后台与前台的分页 UI，通过 `variant` 切换配色与展示细节：
/// - `"admin"`：描边胶囊按钮（与 `BTN_OUTLINE` 同族），显示页码计数
///   （`{当前} / {总} 页 (共 {total} {unit})`），首尾页渲染禁用态。
/// - `"frontend"`：描边胶囊按钮与居中页码，首尾页不渲染越界按钮。
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

    // admin 与 frontend 的配色差异。admin 上一页/下一页用描边胶囊，与本页其它
    // 操作按钮（BTN_OUTLINE）同族，避免出现方角实心按钮破坏整体圆角语言。
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
    let (link_class, link_extra_next): (String, &'static str) = if is_admin {
        (
            format!("{BTN_OUTLINE} inline-flex items-center active:scale-[0.98]"),
            "",
        )
    } else {
        (
            format!("{BTN_OUTLINE} inline-flex items-center justify-center whitespace-nowrap focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-paper-primary"),
            "col-start-3 row-start-1 justify-self-end",
        )
    };
    let disabled_class =
        "inline-flex items-center px-4 py-2 text-sm font-medium text-paper-secondary border border-paper-border rounded-full cursor-not-allowed";

    // admin 首尾页渲染禁用态；frontend 首尾页直接不渲染。
    // on_prev/on_next 存在时走回调式 button（客户端 signal 翻页），否则走路由式 Link。
    let prev_inner = rsx! {
        span { class: "mr-1", "«" }
        "上一页"
    };
    let next_inner = rsx! {
        "下一页"
        span { class: "ml-1", "»" }
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
                span { class: "{disabled_class}",
                    span { class: "mr-1", "«" }
                    "上一页"
                }
            }

            // admin 显示页码计数；回调式翻页且多页时，当前页码可直接编辑跳页。
            if is_admin {
                span { class: "flex items-center gap-1.5 text-sm text-paper-secondary whitespace-nowrap tabular-nums",
                    if total_pages > 1 && on_jump.is_some() {
                        FormInput {
                            r#type: "text",
                            placeholder: "",
                            value: if jump_editing() { jump_draft() } else { current_page.to_string() },
                            class: Some(
                                "w-11 px-1 py-0.5 text-sm text-center bg-transparent text-paper-primary border border-paper-border rounded-full hover:border-paper-accent/60 focus:outline-none focus:border-paper-accent transition-colors",
                            ),
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
                    class: "col-start-2 row-start-1 text-xs text-paper-secondary whitespace-nowrap tabular-nums",
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
                span { class: "{disabled_class}",
                    "下一页"
                    span { class: "ml-1", "»" }
                }
            }
        }
    }
}

static TAB_GROUP_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 筛选选项卡组件。
///
/// 用于切换不同的视图或筛选条件（例如：全部、待审核、已通过等）。
/// 具备高级的平滑滑动底部指示器动画。
///
/// Props：
/// - `items`：选项卡列表，每一项为 `(value, label)`
/// - `active_value`：当前选中的值
/// - `on_change`：选项卡切换时的回调
#[component]
pub fn FilterTabs(
    items: Vec<(&'static str, &'static str)>,
    active_value: String,
    on_change: EventHandler<String>,
) -> Element {
    #[allow(unused_mut)]
    let mut indicator_style = use_signal(|| "left: 0px; width: 0px; opacity: 0;".to_string());
    let id_prefix = use_hook(|| TAB_GROUP_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst));

    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
    let update_indicator = move |active: String| {
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            {
                use wasm_bindgen::JsCast;

                // 等待 DOM 节点更新
                crate::utils::time::sleep_ms(50).await;

                if let Some(window) = web_sys::window() {
                    if let Some(doc) = window.document() {
                        let element_id = format!("tab-{}-{}", id_prefix, active);
                        if let Some(el) = doc.get_element_by_id(&element_id) {
                            if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                                let left = html_el.offset_left();
                                let width = html_el.offset_width();
                                indicator_style.set(format!(
                                    "left: {}px; width: {}px; opacity: 1;",
                                    left, width
                                ));
                            }
                        }
                    }
                }
            }
        });
    };

    use_effect({
        let active_value = active_value.clone();
        move || {
            update_indicator(active_value.clone());
        }
    });

    rsx! {
        div { class: "relative flex gap-4 border-b border-paper-border mb-6",
            for (value, label) in items {
                button {
                    id: "tab-{id_prefix}-{value}",
                    key: "{value}",
                    class: if active_value == *value { "cursor-pointer px-2 py-3 text-xs font-mono tracking-widest uppercase text-paper-primary transition-colors" } else { "cursor-pointer px-2 py-3 text-xs font-mono tracking-widest uppercase text-paper-secondary hover:text-paper-primary transition-colors" },
                    onclick: {
                        let v = value.to_string();
                        move |_| {
                            on_change.call(v.clone());
                            update_indicator(v.clone());
                        }
                    },
                    "{label}"
                }
            }
            // 绝对定位的滑动颜色条
            div {
                class: "absolute bottom-[-1px] h-[2px] bg-paper-primary transition-all duration-300 ease-[cubic-bezier(0.4,0,0.2,1)] pointer-events-none",
                style: "{indicator_style}",
            }
        }
    }
}

/// 标签芯片组件：统一归档索引、软底标签、卡片胶囊与文字链接的展示。
///
/// 收敛原本散落在标签云与文章卡片（`post_card.rs` 描边胶囊）
/// 的两套手写 `Link` 样式——两者都是跳转到 [`Route::TagDetail`] 的可点击标签，
/// 仅视觉变体不同，故合并为一个组件 + `variant` prop。
///
/// Props：
/// - `label`：标签名
/// - `to`：跳转目标路由（泛型，调用方传入具体 `Route` 变体，原子层不绑定 app 路由类型）
/// - `variant`：`"archive"`（归档标签索引）/ `"solid"`（软底）/ `"outline"`（卡片胶囊）/ `"text"`（紧凑文章流）
/// - `count`：可选的文章计数，归档变体使用独立计数徽标，其余使用 `<sup>`
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
        "archive" => "archive-tag-chip",
        "solid" => "inline-flex items-center px-3 py-1.5 text-base font-medium bg-paper-accent-soft text-paper-accent rounded-lg hover:bg-paper-accent hover:text-white transition-all duration-200",
        "text" => "inline-flex items-center py-1 text-paper-secondary hover:text-paper-primary underline decoration-paper-border underline-offset-4 hover:decoration-paper-accent transition-colors focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-paper-accent",
        _ => "inline-flex items-center px-3 py-1 rounded-full border border-paper-border hover:bg-paper-accent hover:border-paper-accent hover:text-white transition-all duration-200",
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
            if variant == "archive" {
                span { class: "archive-tag-mark", aria_hidden: "true", "#" }
                span { class: "archive-tag-name", "{label}" }
            } else {
                "{label}"
            }
            if let Some(c) = count {
                if variant == "archive" {
                    span { class: "archive-tag-count", aria_label: "{c} 篇文章", "{c}" }
                } else {
                    sup { class: "ml-1 text-sm text-paper-secondary", "{c}" }
                }
            }
        }
    }
}
