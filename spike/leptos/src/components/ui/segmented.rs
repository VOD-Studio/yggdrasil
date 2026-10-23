//! 分段控件：胶囊轨道 + 弹簧滑块。
use super::icons::{Icon, IconName};
use leptos::prelude::*;

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
pub fn SegmentedControl<T: Clone + PartialEq + Send + Sync + 'static>(
    items: Vec<SegmentedItem<T>>,
    #[prop(into)] value: Signal<T>,
    #[prop(into)] on_change: Callback<T>,
    aria_label: &'static str,
    #[prop(optional)] kind: SegmentedKind,
    #[prop(optional)] size: SegmentedSize,
    #[prop(optional)] icon_only: bool,
    #[prop(optional)] class: &'static str,
) -> impl IntoView {
    let thumb_style = RwSignal::new(None::<String>);
    let track = NodeRef::<leptos::html::Div>::new();

    let values: Vec<T> = items.iter().map(|item| item.value.clone()).collect();
    let active_index = {
        let values = values.clone();
        Memo::new(move |_| value.with(|v| values.iter().position(|item| item == v)))
    };

    // 测量当前项的位置与尺寸；轨道被隐藏（宽度为 0）或没有当前项时回到未测量态。
    #[cfg(target_arch = "wasm32")]
    let measure = move || {
        use wasm_bindgen::JsCast;
        let style = track.get_untracked().and_then(|el| {
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
        if thumb_style.get_untracked() != style {
            thumb_style.set(style);
        }
    };

    // 轨道尺寸变化（容器查询切换文字显隐、从隐藏变为显示）时重新测量。
    // ResizeObserver 经 Reflect 动态构造，不额外依赖 web-sys feature。
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::Closure;
        use wasm_bindgen::{JsCast, JsValue};
        type ObserverSlot = Option<(JsValue, Closure<dyn FnMut()>)>;
        let observer: StoredValue<ObserverSlot, LocalStorage> = StoredValue::new_local(None);
        Effect::new(move |_| {
            let Some(el) = track.get() else { return };
            if observer.with_value(|o| o.is_some()) {
                return;
            }
            let closure = Closure::wrap(Box::new(measure) as Box<dyn FnMut()>);
            let Ok(ctor) = js_sys::Reflect::get(&js_sys::global(), &"ResizeObserver".into()) else {
                return;
            };
            let Ok(ctor) = ctor.dyn_into::<js_sys::Function>() else {
                return;
            };
            let Ok(ro) = js_sys::Reflect::construct(&ctor, &js_sys::Array::of1(closure.as_ref()))
            else {
                return;
            };
            if let Ok(observe) = js_sys::Reflect::get(&ro, &"observe".into()) {
                let _ = observe.unchecked_into::<js_sys::Function>().call1(&ro, &el);
            }
            observer.set_value(Some((ro, closure)));
            measure();
        });
        on_cleanup(move || {
            if let Some(Some((ro, _closure))) = observer.try_update_value(|o| o.take()) {
                if let Ok(disconnect) = js_sys::Reflect::get(&ro, &"disconnect".into()) {
                    let _ = disconnect.unchecked_into::<js_sys::Function>().call0(&ro);
                }
            }
        });
    }

    // 选中项或数量变化（宽度随之变化）时重新测量滑块。
    Effect::new(move |_| {
        active_index.track();
        #[cfg(target_arch = "wasm32")]
        leptos::task::spawn_local(async move {
            crate::utils::time::sleep_ms(30).await;
            measure();
        });
    });

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

    let on_keydown = {
        let values = values.clone();
        move |e: leptos::ev::KeyboardEvent| {
            let len = values.len();
            let current = active_index.get_untracked();
            let next = match e.key().as_str() {
                "ArrowRight" => neighbor_index(len, current, 1),
                "ArrowLeft" => neighbor_index(len, current, -1),
                "Home" => (len > 0).then_some(0),
                "End" => len.checked_sub(1),
                _ => None,
            };
            if let Some(next) = next {
                e.prevent_default();
                on_change.run(values[next].clone());
                // 焦点随选中项移动：按序号取轨道里的第 next 个选项。
                #[cfg(target_arch = "wasm32")]
                {
                    use wasm_bindgen::JsCast;
                    if let Some(el) = track
                        .get_untracked()
                        .and_then(|t| t.query_selector_all(".segmented-item").ok())
                        .and_then(|list| list.get(next as u32))
                        .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
                    {
                        let _ = el.focus();
                    }
                }
            }
        }
    };

    let buttons = items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let active = move || active_index.get() == Some(index);
            let label = item.label.clone();
            let v = item.value.clone();
            view! {
                <button
                    type="button"
                    class="segmented-item"
                    role=if is_tabs { "tab" } else { "radio" }
                    aria-selected=move || is_tabs.then(|| active().to_string())
                    aria-checked=move || (!is_tabs).then(|| active().to_string())
                    data-active=move || active().then_some("true")
                    tabindex=move || if active() { "0" } else { "-1" }
                    aria-label=icon_only.then(|| label.clone())
                    title=icon_only.then(|| label.clone())
                    on:click=move |_| on_change.run(v.clone())
                >
                    {item.icon.map(|icon| view! { <Icon name=icon size=14/> })}
                    {(!icon_only).then(|| view! { <span class="segmented-label">{label.clone()}</span> })}
                    {item.count.map(|n| view! { <span class="segmented-count">{n}</span> })}
                </button>
            }
        })
        .collect_view();

    view! {
        <div class=wrap_class>
            <div
                node_ref=track
                class="segmented"
                role=if is_tabs { "tablist" } else { "radiogroup" }
                aria-label=aria_label
                data-ready=move || thumb_style.with(|s| s.is_some()).then_some("true")
                on:keydown=on_keydown
            >
                <span
                    class="segmented-thumb"
                    aria-hidden="true"
                    style=move || thumb_style.get().unwrap_or_else(|| "opacity: 0;".to_string())
                ></span>
                {buttons}
            </div>
        </div>
    }
}
