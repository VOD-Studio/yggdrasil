//! Pagination, tabs and navigable tag chips.
use super::icons::{Icon, IconName};
use dioxus::prelude::*;
use dioxus::router::components::Link;
/// 前台列表分页：上一页 / 下一页描边按钮与居中的「当前 / 总」页码，首尾页不渲染越界按钮。
///
/// Props：
/// - `current_page`：当前页码（从 1 开始）
/// - `total`：数据总条数
/// - `per_page`：每页条数，用于计算总页数
/// - `prev_route`：上一页的目标路由
/// - `next_route`：下一页的目标路由
#[component]
pub fn Pagination<R: Routable + Clone + PartialEq + 'static>(
    current_page: i32,
    total: i64,
    per_page: i32,
    prev_route: R,
    next_route: R,
) -> Element {
    let has_prev = current_page > 1;
    let total_pages = ((total + per_page as i64 - 1) / per_page as i64).max(1) as i32;
    let has_next = current_page < total_pages;

    rsx! {
        nav { class: "frontend-pagination", aria_label: "分页导航",
            if has_prev {
                Link { class: "btn btn-sm btn-outline", to: prev_route,
                    Icon { name: IconName::ChevronLeft, size: 15 }
                    "上一页"
                }
            }
            span {
                class: "col-start-2 row-start-1 text-xs text-fg-3 whitespace-nowrap tabular-nums",
                aria_label: "第 {current_page} 页，共 {total_pages} 页",
                aria_current: "page",
                "{current_page} / {total_pages}"
            }
            if has_next {
                Link {
                    class: "btn btn-sm btn-outline col-start-3 row-start-1 justify-self-end",
                    to: next_route,
                    "下一页"
                    Icon { name: IconName::ChevronRight, size: 15 }
                }
            }
        }
    }
}

/// 表格底栏分页：左侧「第 21–40 篇，共 57 篇」，右侧上一页、页码与下一页图标按钮。
/// 只有一页时只显示总数。
///
/// Props：
/// - `page`：当前页（从 1 开始）；`total`：总条数；`per_page`：每页条数
/// - `unit`：计数单位，如「篇」「条」
/// - `on_change`：翻到的页码
#[component]
pub fn TablePager(
    page: i32,
    total: i64,
    per_page: i32,
    unit: &'static str,
    on_change: EventHandler<i32>,
) -> Element {
    let pages = ((total + per_page as i64 - 1) / per_page as i64).max(1) as i32;
    let page = page.clamp(1, pages);
    let first = (page as i64 - 1) * per_page as i64 + 1;
    let last = (page as i64 * per_page as i64).min(total);
    let range = if total == 0 {
        format!("共 0 {unit}")
    } else if pages == 1 {
        format!("共 {total} {unit}")
    } else {
        format!("第 {first}–{last} {unit}，共 {total} {unit}")
    };
    rsx! {
        nav { class: "table-pager", aria_label: "分页导航",
            span { class: "table-pager-range", "{range}" }
            if pages > 1 {
                div { class: "table-pager-controls",
                    super::controls::IconButton {
                        icon: IconName::ChevronLeft,
                        label: "上一页",
                        size: super::controls::ButtonSize::Sm,
                        disabled: page <= 1,
                        onclick: move |_| on_change.call(page - 1),
                    }
                    span { class: "table-pager-page", aria_current: "page", "{page} / {pages}" }
                    super::controls::IconButton {
                        icon: IconName::ChevronRight,
                        label: "下一页",
                        size: super::controls::ButtonSize::Sm,
                        tooltip_align: "end",
                        disabled: page >= pages,
                        onclick: move |_| on_change.call(page + 1),
                    }
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
/// - `counts`：与 `items` 一一对应的数量，显示在标签后（可选，缺省不显示）
/// - `class`：外层容器类，默认留出下方 1.5rem 间距
#[component]
pub fn FilterTabs(
    items: Vec<(&'static str, &'static str)>,
    active_value: String,
    on_change: EventHandler<String>,
    #[props(default)] aria_label: Option<&'static str>,
    #[props(default)] counts: Vec<i64>,
    #[props(default = "mb-6")] class: &'static str,
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
                    // 显式写回不透明：样式字符串更新时，未测量时的 opacity 不会被移除。
                    thumb_style.set(Some(format!(
                        "transform: translateX({}px); width: {}px; opacity: 1;",
                        el.offset_left(),
                        el.offset_width()
                    )));
                }
            }
        });
    };
    // 选中项或数量变化（宽度随之变化）时重新测量滑块。
    let count_key = counts
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    use_effect(use_reactive(
        (&active_value, &count_key),
        move |(active, _)| update_thumb(active),
    ));

    let measured = thumb_style().is_some();
    let thumb = thumb_style().unwrap_or_else(|| "opacity: 0;".to_string());
    let items_for_keys = items.clone();
    let active_for_keys = active_value.clone();

    rsx! {
        div { class,
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
                for (index, (value, label)) in items.iter().enumerate() {
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
                        if let Some(n) = counts.get(index) {
                            span { class: "segmented-count", "{n}" }
                        }
                    }
                }
            }
        }
    }
}

/// 面包屑中的一级。最后一级是当前页（不可点击，标 `aria-current`）；中间级 `to` 为
/// `None` 时是不可点击的分组名（如后台的「内容」）。
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

    /// 不可点击的中间级（分组名）。
    pub fn text(label: impl Into<String>) -> Self {
        Self::current(label)
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
                    None if i < last => rsx! {
                        span { key: "{i}", "{crumb.label}" }
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
/// 各处跳转到 [`Route::TagDetail`] 的可点击标签共用此组件，仅以 `variant` 区分视觉。
///
/// Props：
/// - `label`：标签名
/// - `to`：跳转目标路由（泛型，调用方传入具体 `Route` 变体，原子层不绑定 app 路由类型）
/// - `variant`：`"solid"`（软底）/ `"outline"`（卡片胶囊）/ `"text"`（紧凑文章流）
/// - `count`：可选的文章计数
/// - `stop_propagation`：是否阻止点击冒泡（卡片内覆盖层链接场景需要）
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
