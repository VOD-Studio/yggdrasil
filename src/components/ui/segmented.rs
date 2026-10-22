//! 分段控件：胶囊轨道 + 弹簧滑块。
use super::icons::{Icon, IconName};
use dioxus::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static SEGMENTED_ID: AtomicUsize = AtomicUsize::new(0);

/// 分段控件的一项：值、文字，可选图标与数量。
#[derive(Clone, PartialEq)]
pub struct SegmentedItem<T> {
    pub value: T,
    pub label: String,
    pub icon: Option<IconName>,
    pub count: Option<i64>,
}

impl<T> SegmentedItem<T> {
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            icon: None,
            count: None,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn count(mut self, count: i64) -> Self {
        self.count = Some(count);
        self
    }
}

/// 语义：Tabs = tablist/tab + aria-selected（切换视图、筛选）；
/// Radio = radiogroup/radio + aria-checked（设置取值）。
#[derive(Clone, Copy, PartialEq, Default)]
pub enum SegmentedKind {
    #[default]
    Tabs,
    Radio,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum SegmentedSize {
    #[default]
    Md,
    Sm,
}

/// 循环移动到相邻项（←/→ 用）；没有当前项时从第 0 项起算。
fn neighbor_index(len: usize, current: Option<usize>, delta: i32) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let cur = current.unwrap_or(0) as i32;
    Some((cur + delta).rem_euclid(len as i32) as usize)
}

/// 分段控件。
///
/// 胶囊轨道里放一组选项，当前项下是浮起的滑块，切换时以 snappy 弹簧滑到新位置。
/// ←/→ 在选项间循环并立即切换，Home/End 跳到首尾，焦点随之移动（roving tabindex，
/// Tab 只停在当前项）。滑块测量完成前（含服务端渲染与 hydration 首帧），当前项直接
/// 画浮起底色，首屏不闪。
///
/// Props：
/// - `items`：选项列表，见 [`SegmentedItem`]
/// - `value`：当前值；没有匹配项时没有当前项
/// - `on_change`：切换时的回调
/// - `aria_label`：控件组的无障碍名称
/// - `kind`：语义，Tabs（默认）或 Radio，见 [`SegmentedKind`]
/// - `size`：Md（默认）或 Sm
/// - `icon_only`：只显示图标，文字作为 aria-label 与 title
/// - `class`：外层容器附加类（间距、页面变体）
///
/// 带图标且未设 `icon_only` 时，容器窄于 16rem 自动只留图标（文字仍供读屏）。
/// 容器对宽度有尺寸包含，放进 flex 行时调用方必须给它宽度。
#[component]
pub fn SegmentedControl<T: Clone + PartialEq + 'static>(
    items: Vec<SegmentedItem<T>>,
    value: T,
    on_change: EventHandler<T>,
    aria_label: &'static str,
    #[props(default)] kind: SegmentedKind,
    #[props(default)] size: SegmentedSize,
    #[props(default)] icon_only: bool,
    #[props(default)] class: &'static str,
) -> Element {
    let group = use_hook(|| SEGMENTED_ID.fetch_add(1, Ordering::SeqCst));
    #[allow(unused_mut)]
    let mut thumb_style = use_signal(|| None::<String>);
    #[cfg(target_arch = "wasm32")]
    let track = use_signal(|| None::<web_sys::Element>);

    let active_index = items.iter().position(|item| item.value == value);
    // 选中项或数量变化（宽度随之变化）时重新测量滑块。
    let count_key = items
        .iter()
        .map(|item| item.count.map(|n| n.to_string()).unwrap_or_default())
        .collect::<Vec<_>>()
        .join(",");

    // 测量当前项的位置与尺寸；轨道被隐藏（宽度为 0）或没有当前项时回到未测量态。
    #[cfg(target_arch = "wasm32")]
    let measure = move || {
        use wasm_bindgen::JsCast;
        let style = track.peek().as_ref().and_then(|el| {
            let active = el
                .query_selector("[data-active]")
                .ok()
                .flatten()?
                .dyn_into::<web_sys::HtmlElement>()
                .ok()?;
            (active.offset_width() > 0).then(|| {
                // 显式写回不透明：样式字符串更新时，未测量时的 opacity 不会被移除。
                format!(
                    "transform: translate({}px, {}px); width: {}px; height: {}px; opacity: 1;",
                    active.offset_left(),
                    active.offset_top(),
                    active.offset_width(),
                    active.offset_height()
                )
            })
        });
        if *thumb_style.peek() != style {
            let mut thumb_style = thumb_style;
            thumb_style.set(style);
        }
    };

    // 轨道尺寸变化（容器查询切换文字显隐、从隐藏变为显示）时重新测量。
    #[cfg(target_arch = "wasm32")]
    let observer = {
        use std::cell::RefCell;
        use std::rc::Rc;
        type Observer = Rc<
            RefCell<
                Option<(
                    web_sys::ResizeObserver,
                    wasm_bindgen::prelude::Closure<dyn FnMut()>,
                )>,
            >,
        >;
        let observer: Observer = use_hook(|| Rc::new(RefCell::new(None)));
        let for_drop = observer.clone();
        use_drop(move || {
            if let Some((observer, _closure)) = for_drop.borrow_mut().take() {
                observer.disconnect();
            }
        });
        observer
    };

    use_effect(use_reactive((&active_index, &count_key), move |_| {
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            {
                crate::utils::time::sleep_ms(30).await;
                measure();
            }
        });
    }));

    let measured = thumb_style().is_some();
    let thumb = thumb_style().unwrap_or_else(|| "opacity: 0;".to_string());
    let is_tabs = kind == SegmentedKind::Tabs;
    let adaptive = !icon_only && items.iter().any(|item| item.icon.is_some());
    let wrap_class = format!(
        "segmented-wrap{}{}{} {class}",
        if size == SegmentedSize::Sm {
            " segmented-sm"
        } else {
            ""
        },
        if icon_only {
            " segmented-icon-only"
        } else {
            ""
        },
        if adaptive { " segmented-adaptive" } else { "" },
    );
    let items_for_keys = items.clone();
    let value_for_keys = value.clone();

    rsx! {
        div { class: "{wrap_class}",
            div {
                class: "segmented",
                role: if is_tabs { "tablist" } else { "radiogroup" },
                aria_label,
                "data-ready": measured.then_some("true"),
                onmounted: move |_e| {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use dioxus::web::WebEventExt;
                        use wasm_bindgen::JsCast;
                        let Some(el) = _e.try_as_web_event() else { return };
                        let mut track = track;
                        track.set(Some(el.clone()));
                        if observer.borrow().is_none() {
                            let closure = wasm_bindgen::prelude::Closure::wrap(
                                Box::new(measure) as Box<dyn FnMut()>,
                            );
                            if let Ok(ro) = web_sys::ResizeObserver::new(closure.as_ref().unchecked_ref()) {
                                ro.observe(&el);
                                *observer.borrow_mut() = Some((ro, closure));
                            }
                        }
                        measure();
                    }
                },
                onkeydown: move |e| {
                    let len = items_for_keys.len();
                    let current = items_for_keys.iter().position(|item| item.value == value_for_keys);
                    let next = match e.key() {
                        Key::ArrowRight => neighbor_index(len, current, 1),
                        Key::ArrowLeft => neighbor_index(len, current, -1),
                        Key::Home => (len > 0).then_some(0),
                        Key::End => len.checked_sub(1),
                        _ => None,
                    };
                    if let Some(next) = next {
                        e.prevent_default();
                        on_change.call(items_for_keys[next].value.clone());
                        #[cfg(target_arch = "wasm32")]
                        {
                            use wasm_bindgen::JsCast;
                            if let Some(el) = web_sys::window()
                                .and_then(|w| w.document())
                                .and_then(|d| d.get_element_by_id(&format!("seg-{group}-{next}")))
                                .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                            {
                                let _ = el.focus();
                            }
                        }
                    }
                },
                span { class: "segmented-thumb", aria_hidden: "true", style: "{thumb}" }
                for (index, item) in items.iter().enumerate() {
                    {
                        let active = active_index == Some(index);
                        let state = active.to_string();
                        rsx! {
                            button {
                                id: "seg-{group}-{index}",
                                key: "{index}",
                                r#type: "button",
                                class: "segmented-item",
                                role: if is_tabs { "tab" } else { "radio" },
                                aria_selected: is_tabs.then(|| state.clone()),
                                aria_checked: (!is_tabs).then(|| state.clone()),
                                "data-active": active.then_some("true"),
                                tabindex: if active { "0" } else { "-1" },
                                aria_label: icon_only.then(|| item.label.clone()),
                                title: icon_only.then(|| item.label.clone()),
                                onclick: {
                                    let v = item.value.clone();
                                    move |_| on_change.call(v.clone())
                                },
                                if let Some(icon) = item.icon {
                                    Icon { name: icon, size: 14 }
                                }
                                if !icon_only {
                                    span { class: "segmented-label", "{item.label}" }
                                }
                                if let Some(n) = item.count {
                                    span { class: "segmented-count", "{n}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::neighbor_index;

    #[test]
    fn neighbor_index_wraps_in_both_directions() {
        assert_eq!(neighbor_index(3, Some(0), 1), Some(1));
        assert_eq!(neighbor_index(3, Some(2), 1), Some(0));
        assert_eq!(neighbor_index(3, Some(0), -1), Some(2));
        // 没有当前项时从第 0 项起算
        assert_eq!(neighbor_index(3, None, 1), Some(1));
        assert_eq!(neighbor_index(0, None, 1), None);
    }
}
