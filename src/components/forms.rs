//! 表单控件组件
//!
//! 提供登录、注册、评论等页面共享的输入框、下拉、时间、数字、开关与提示条。
//! 外观定义在 styles/components/ui.css（`.field`、`.listbox`、`.switch`、`.alert`）。
//!
//! 约定（Leptos）：随状态变化的 props 一律是 `Signal<T>` / `MaybeProp<T>`，可直接传
//! 常量、信号或 `move ||` 闭包；受控值写作 `value=信号` + `oninput`/`onchange` 回调
//! （回调里由调用方写回信号）。`id` 等可选字符串是 `MaybeProp<String>`，`id="x"` 即可。

use crate::components::ui::{Icon, IconName};
use leptos::ev::{FocusEvent, KeyboardEvent, MouseEvent};
use leptos::prelude::*;

/// 输入框基础类：浅底、发丝线环，聚焦陶土双环，`aria-invalid` 时红环。
pub const INPUT_CLASS: &str = "field w-full";

/// 多行输入框基础类：随内容长高（`.field-textarea`）。
pub const TEXTAREA_CLASS: &str = "field field-textarea";

/// FormSelect / TimePicker / NumberInput 实例序号计数器（跨泛型单例化全局唯一）。
static FORM_SELECT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// FormSelect 紧凑触发器：工具栏内联小下拉（自动刷新、导出格式等）。
pub const FORM_SELECT_COMPACT_CLASS: &str = "field field-sm w-auto";

/// 选项多于该数量时，下拉面板顶部出现筛选框。
const FILTER_THRESHOLD: usize = 8;

/// 分配实例序号，用于拼接未显式传 `id` 时的元素 id（`form-select-N` 等）。
///
/// 序号在挂载后（Effect）才分配：全局计数器在服务端是进程级的，直接写进 SSR HTML
/// 会与水合后的客户端计数不一致，导致 `get_element_by_id` 找不到元素。因此服务端
/// 与水合首帧都是 `None`，对应的 id 属性随之缺省；弹层都在用户交互后才出现，届时序号已就绪。
fn use_instance_prefix() -> Signal<Option<usize>> {
    let prefix = RwSignal::new(None::<usize>);
    Effect::new(move |_| {
        prefix.set(Some(
            FORM_SELECT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        ));
    });
    prefix.into()
}

/// 面板应向上展开的条件：视口下方空间不足，且上方空间比下方更宽余。
///
/// 纯函数便于单测；`panel_height` 由调用方按选项数估算（见 `measure_flip`）。
#[allow(dead_code)] // server 构建下仅被 dead 的组件体引用（wasm 与单测为真实调用方）
fn should_flip(
    trigger_top: f64,
    trigger_bottom: f64,
    viewport_height: f64,
    panel_height: f64,
) -> bool {
    /// 面板与触发器的间隙（6px）加视口边缘留白。
    const MARGIN: f64 = 14.0;
    let below = viewport_height - trigger_bottom;
    let above = trigger_top;
    below < panel_height + MARGIN && above > below
}

/// 键盘导航的循环索引：在 `len` 个选项中从 `cur` 移动 `delta`（±1），越界回绕。
fn wrap_index(cur: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (cur as i32 + delta).rem_euclid(len as i32) as usize
}

/// 在可选项（已筛选、未禁用）之间移动高亮；当前项不在其中时落到首项或末项。
fn step_active(navigable: &[usize], cur: usize, delta: i32) -> Option<usize> {
    if navigable.is_empty() {
        return None;
    }
    match navigable.iter().position(|&i| i == cur) {
        Some(pos) => Some(navigable[wrap_index(pos, delta, navigable.len())]),
        None if delta < 0 => navigable.last().copied(),
        None => navigable.first().copied(),
    }
}

/// 筛选：忽略大小写的子串匹配，空查询匹配全部。
fn matches_query(label: &str, query: &str) -> bool {
    let query = query.trim();
    query.is_empty() || label.to_lowercase().contains(&query.to_lowercase())
}

/// 测量触发器视口位置，决定面板展开方向（仅 wasm；SSR 无 DOM）。
#[cfg(target_arch = "wasm32")]
pub(crate) fn measure_flip(trigger_id: &str, option_count: usize) -> bool {
    /// 选项行高：20px 行盒 + 上下各 8px 内边距。
    const ROW_HEIGHT: f64 = 36.0;
    /// 面板内边距 4px ×2。
    const PANEL_CHROME: f64 = 8.0;
    /// 面板高度上限：18rem。
    const PANEL_MAX: f64 = 288.0;

    let Some(window) = web_sys::window() else {
        return false;
    };
    let Some(document) = window.document() else {
        return false;
    };
    let Some(el) = document.get_element_by_id(trigger_id) else {
        return false;
    };
    let rect = el.get_bounding_client_rect();
    let viewport = window
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0);
    let panel_height = ((option_count as f64) * ROW_HEIGHT + PANEL_CHROME).min(PANEL_MAX);
    should_flip(rect.top(), rect.bottom(), viewport, panel_height)
}

/// 把指定元素滚入所在面板的可视区（打开/键盘导航时跟随；仅 wasm）。
#[cfg(target_arch = "wasm32")]
fn scroll_option_into_view(element_id: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id(element_id) else {
        return;
    };
    let opts = web_sys::ScrollIntoViewOptions::new();
    opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
    el.scroll_into_view_with_scroll_into_view_options(&opts);
}

/// 把指定元素滚到所在列的中间（时间列打开时；仅 wasm）。
#[cfg(target_arch = "wasm32")]
fn scroll_into_center(element_id: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id(element_id) else {
        return;
    };
    let opts = web_sys::ScrollIntoViewOptions::new();
    opts.set_block(web_sys::ScrollLogicalPosition::Center);
    el.scroll_into_view_with_scroll_into_view_options(&opts);
}

/// 按 id 聚焦元素（仅 wasm）。
#[cfg(target_arch = "wasm32")]
fn focus_by_id(element_id: &str) {
    use wasm_bindgen::JsCast;
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(element_id))
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.focus();
    }
}

/// 下拉选择框组件（自定义弹层，全主题化）。
///
/// 原生 `<select>` 的弹出列表由 OS/浏览器渲染，无法跟随主题（暗色下是白底
/// 系统菜单），故用 `button[aria-haspopup=listbox]` + 绝对定位面板重写：
/// - 面板从触发器下方展开（snappy 弹簧，0.98 → 1），视口下方不够时向上展开；
/// - 焦点留在触发器，键盘经 `aria-activedescendant` 高亮（↑↓ 循环、
///   Enter/Space 选中、Esc 关闭、Home/End 跳首尾、Tab 关闭并自然流转焦点）；
/// - 选项多于 8 个时面板顶部出现筛选框，打开即聚焦，键盘规则相同，Esc 把焦点还给触发器；
/// - `disabled_options` 中的项保留但变淡，并在下方写明原因，键盘与点击都跳过；
/// - 透明遮罩拦截外部点击关闭；选项 `mousedown` 阻止默认行为，点击不夺走焦点；
/// - 泛型值绑定：`onchange` 直接回传选中项的 `T`，无需字符串反查。
///
/// 触发器与选项的内部 `id`（`form-select-N-opt-I` 等）在挂载后由客户端计数分配，
/// 原因见 `use_instance_prefix`；需要与 label 关联时显式传 `id`。
///
/// Props：
/// - `id`：触发器 id，用于与 label 关联（缺省挂载后用内部计数器生成）
/// - `value`：当前选中项（受控，可传信号）
/// - `options`：可选项 `(值, 标签)` 列表（`Vec` 或返回它的闭包 / 信号）
/// - `disabled_options`：不可用的项及原因 `(值, 原因)`
/// - `onchange`：选中变化回调，回传新选中项的值
/// - `trigger_class`：触发器外观（缺省全宽 `field`；工具栏传 [`FORM_SELECT_COMPACT_CLASS`]）
/// - `aria_label`：触发器无障碍标签；可见文本不足以表意时传入
///
/// `value` / `onchange` 的类型参数要能从 `options` 或 `onchange` 闭包的参数类型推断；
/// 传裸信号且闭包参数无标注时，给闭包写类型（`move |v: Foo| ..`）。
#[component]
pub fn FormSelect<T, L>(
    #[prop(optional, into)] id: MaybeProp<String>,
    #[prop(into)] value: Signal<T>,
    #[prop(into)] options: Signal<Vec<(T, L)>>,
    #[prop(into)] onchange: Callback<T>,
    #[prop(optional, into)] disabled_options: Signal<Vec<(T, &'static str)>>,
    #[prop(optional)] trigger_class: Option<&'static str>,
    #[prop(optional)] aria_label: Option<&'static str>,
) -> impl IntoView
where
    T: Clone + PartialEq + Send + Sync + 'static,
    L: AsRef<str> + Send + Sync + 'static,
{
    let trigger_cls = format!("select-trigger {}", trigger_class.unwrap_or("field"));
    let prefix = use_instance_prefix();
    let trigger_id = Signal::derive(move || {
        id.get()
            .or_else(|| prefix.get().map(|n| format!("form-select-{n}")))
    });
    let filter_id = move || prefix.get().map(|n| format!("form-select-{n}-filter"));
    let option_id = move |i: usize| {
        prefix
            .get()
            .map(|n| format!("form-select-{n}-opt-{i}"))
            .unwrap_or_default()
    };

    // 受控选中序号；value 不在 options 时兜底 0（与浏览器默认选中第一项一致）。
    let selected = Memo::new(move |_| {
        let current = value.get();
        options.with(|o| o.iter().position(|(v, _)| *v == current).unwrap_or(0))
    });
    let len = Memo::new(move |_| options.with(Vec::len));
    let filterable = Memo::new(move |_| len.get() > FILTER_THRESHOLD);

    let open = RwSignal::new(false);
    let active = RwSignal::new(selected.get_untracked());
    let query = RwSignal::new(String::new());
    let flip_up = RwSignal::new(false);

    // 每项的禁用原因；与 options 一一对应。
    let reasons = Memo::new(move |_| {
        options.with(|o| {
            disabled_options.with(|d| {
                o.iter()
                    .map(|(v, _)| d.iter().find(|(x, _)| x == v).map(|(_, reason)| *reason))
                    .collect::<Vec<Option<&'static str>>>()
            })
        })
    });
    let visible = Memo::new(move |_| {
        let q = query.get();
        options.with(|o| {
            (0..o.len())
                .filter(|&i| matches_query(o[i].1.as_ref(), &q))
                .collect::<Vec<usize>>()
        })
    });
    let navigable = Memo::new(move |_| {
        reasons.with(|r| {
            visible.with(|v| {
                v.iter()
                    .copied()
                    .filter(|&i| r[i].is_none())
                    .collect::<Vec<usize>>()
            })
        })
    });

    // 打开或键盘导航时，把高亮项滚入面板可视区；带筛选框时打开即聚焦筛选框。
    Effect::new(move |_| {
        if open.get() {
            let _idx = active.get();
            #[cfg(target_arch = "wasm32")]
            {
                scroll_option_into_view(&option_id(_idx));
                if filterable.get_untracked() {
                    if let Some(filter) = filter_id() {
                        focus_by_id(&filter);
                    }
                }
            }
        }
    });

    let focus_trigger = move || {
        #[cfg(target_arch = "wasm32")]
        if let Some(trigger) = trigger_id.get_untracked() {
            focus_by_id(&trigger);
        }
    };

    let open_panel = move || {
        #[cfg(target_arch = "wasm32")]
        if let Some(trigger) = trigger_id.get_untracked() {
            flip_up.set(measure_flip(
                &trigger,
                len.get_untracked().min(FILTER_THRESHOLD + 1),
            ));
        }
        query.set(String::new());
        active.set(selected.get_untracked());
        open.set(true);
    };

    // 选中第 i 项：关闭面板、回调、焦点还给触发器。
    let pick = move |i: usize| {
        open.set(false);
        if let Some(v) = options.with_untracked(|o| o.get(i).map(|(v, _)| v.clone())) {
            onchange.run(v);
        }
        focus_trigger();
    };

    // 面板打开后的键盘处理；触发器与筛选框共用。
    let handle_open_key = move |e: &KeyboardEvent| {
        let key = e.key();
        match key.as_str() {
            "ArrowDown" | "ArrowUp" => {
                e.prevent_default();
                let delta = if key == "ArrowDown" { 1 } else { -1 };
                if let Some(next) =
                    navigable.with_untracked(|n| step_active(n, active.get_untracked(), delta))
                {
                    active.set(next);
                }
            }
            "Home" => {
                e.prevent_default();
                if let Some(first) = navigable.with_untracked(|n| n.first().copied()) {
                    active.set(first);
                }
            }
            "End" => {
                e.prevent_default();
                if let Some(last) = navigable.with_untracked(|n| n.last().copied()) {
                    active.set(last);
                }
            }
            "Enter" | " " if key == "Enter" || !filterable.get_untracked() => {
                e.prevent_default();
                let current = active.get_untracked();
                if navigable.with_untracked(|n| n.contains(&current)) {
                    pick(current);
                }
            }
            "Escape" => {
                e.prevent_default();
                open.set(false);
                focus_trigger();
            }
            "Tab" => open.set(false),
            _ => {}
        }
    };

    view! {
        <div class="relative">
            <button
                id=move || trigger_id.get()
                type="button"
                class=trigger_cls
                aria-haspopup="listbox"
                aria-expanded=move || open.get().to_string()
                aria-activedescendant=move || {
                    (open.get() && !filterable.get()).then(|| option_id(active.get()))
                }
                aria-label=aria_label
                on:click=move |_| {
                    // 打开态下触发器被透明遮罩盖住，点击落在遮罩上即关闭；
                    // 这里只需处理「未开 → 开」。
                    if !open.get_untracked() {
                        open_panel();
                    }
                }
                on:keydown=move |e: KeyboardEvent| {
                    if open.get_untracked() {
                        handle_open_key(&e);
                        return;
                    }
                    if matches!(e.key().as_str(), "ArrowDown" | "ArrowUp" | "Enter" | " ") {
                        e.prevent_default();
                        open_panel();
                    }
                }
            >
                <span class="select-value">
                    {move || {
                        options.with(|o| {
                            o.get(selected.get())
                                .map(|(_, label)| label.as_ref().to_string())
                                .unwrap_or_default()
                        })
                    }}
                </span>
                <Icon name=IconName::ChevronDown size=15 class="select-chevron"/>
            </button>

            {move || {
                open.get()
                    .then(|| {
                        view! {
                            // 透明遮罩：拦截外部点击（点遮罩即关），z-40 < 面板 z-50。
                            <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>
                            <ul
                                class="listbox"
                                data-placement=move || if flip_up.get() { "top" } else { "bottom" }
                                role="listbox"
                                aria-labelledby=move || trigger_id.get()
                            >
                                {move || {
                                    filterable
                                        .get()
                                        .then(|| {
                                            view! {
                                                <li class="listbox-filter" role="presentation">
                                                    <input
                                                        id=filter_id
                                                        class="field field-sm w-full"
                                                        type="text"
                                                        placeholder="筛选"
                                                        autocomplete="off"
                                                        prop:value=move || query.get()
                                                        aria-label="筛选选项"
                                                        aria-activedescendant=move || option_id(active.get())
                                                        on:input=move |e| {
                                                            let text = event_target_value(&e);
                                                            let first = options
                                                                .with_untracked(|o| {
                                                                    reasons
                                                                        .with_untracked(|r| {
                                                                            o.iter()
                                                                                .zip(r)
                                                                                .position(|((_, label), reason)| {
                                                                                    reason.is_none() && matches_query(label.as_ref(), &text)
                                                                                })
                                                                        })
                                                                });
                                                            if let Some(first) = first {
                                                                active.set(first);
                                                            }
                                                            query.set(text);
                                                        }
                                                        on:keydown=move |e: KeyboardEvent| handle_open_key(&e)
                                                    />
                                                </li>
                                            }
                                        })
                                }}
                                {move || {
                                    visible
                                        .with(Vec::is_empty)
                                        .then(|| {
                                            view! {
                                                <li class="listbox-empty" role="presentation">
                                                    "没有匹配的选项"
                                                </li>
                                            }
                                        })
                                }}
                                {move || {
                                    visible
                                        .get()
                                        .into_iter()
                                        .map(|i| {
                                            let label = options
                                                .with(|o| {
                                                    o.get(i)
                                                        .map(|(_, l)| l.as_ref().to_string())
                                                        .unwrap_or_default()
                                                });
                                            let reason = reasons.with(|r| r.get(i).copied().flatten());
                                            view! {
                                                <SelectOption
                                                    id=option_id(i)
                                                    label=label
                                                    reason=reason
                                                    selected=Signal::derive(move || selected.get() == i)
                                                    active=Signal::derive(move || active.get() == i)
                                                    on_hover=move |_| {
                                                        if reason.is_none() {
                                                            active.set(i);
                                                        }
                                                    }
                                                    on_pick=move |_| pick(i)
                                                />
                                            }
                                        })
                                        .collect_view()
                                }}
                            </ul>
                        }
                    })
            }}
        </div>
    }
}

/// 下拉面板中的一项。禁用项（`reason` 为 Some）变淡、写明原因，不响应点击。
#[component]
fn SelectOption(
    id: String,
    label: String,
    reason: Option<&'static str>,
    #[prop(into)] selected: Signal<bool>,
    #[prop(into)] active: Signal<bool>,
    #[prop(into)] on_hover: Callback<()>,
    #[prop(into)] on_pick: Callback<()>,
) -> impl IntoView {
    view! {
        <li
            id=id
            class="listbox-option"
            role="option"
            aria-selected=move || selected.get().to_string()
            aria-disabled=reason.is_some().then_some("true")
            data-active=move || active.get().to_string()
            // 阻止 mousedown 默认行为：点击选项不夺走焦点。
            on:mousedown=move |e: MouseEvent| e.prevent_default()
            on:mouseenter=move |_| on_hover.run(())
            on:click=move |_| {
                if reason.is_none() {
                    on_pick.run(());
                }
            }
        >
            <span class="min-w-0 flex-1">
                <span class="block truncate">{label}</span>
                {reason.map(|reason| view! { <span class="listbox-option-hint">{reason}</span> })}
            </span>
            {move || {
                selected.get().then(|| view! { <Icon name=IconName::Check size=14 class="listbox-check"/> })
            }}
        </li>
    }
}

/// 解析 "HH:MM" 为 (时, 分)；任一段缺失或越界整体回退 (0, 0)。
/// 纯防御性兜底：正常路径下 value 只来自服务端 normalize 或本组件输出，必然合法。
fn parse_hhmm(value: &str) -> (u8, u8) {
    let mut parts = value.split(':');
    let hour = parts
        .next()
        .and_then(|s| s.parse::<u8>().ok())
        .filter(|h| *h < 24);
    let minute = parts
        .next()
        .and_then(|s| s.parse::<u8>().ok())
        .filter(|m| *m < 60);
    match (hour, minute) {
        (Some(h), Some(m)) => (h, m),
        _ => (0, 0),
    }
}

/// 时间选择器（24 小时制 "HH:MM"，浏览器本地时间）。
///
/// 触发器显示当前时间与「本地时间」；点开是小时、分钟两列的浮层，点选即回调，
/// 底部给出对应的 UTC 时间（服务端以 UTC 存储，换算由调用方在保存时完成）。
/// Esc 或点外部关闭，焦点回到触发器；列内 ↑↓ 逐格调整。
///
/// Props：
/// - `id`：触发器 id，用于与 label 关联（缺省挂载后用内部计数器生成）
/// - `value`：当前值 "HH:MM"（受控；非法值回退显示 00:00，不 panic）
/// - `onchange`：选中变化回调，回传组合后的 "HH:MM"
#[component]
pub fn TimePicker(
    #[prop(optional, into)] id: MaybeProp<String>,
    #[prop(into)] value: Signal<String>,
    #[prop(into)] onchange: Callback<String>,
) -> impl IntoView {
    let prefix = use_instance_prefix();
    let trigger_id = Signal::derive(move || {
        id.get()
            .or_else(|| prefix.get().map(|n| format!("time-picker-{n}")))
    });
    let parsed = Memo::new(move |_| parse_hhmm(&value.get()));
    let hour = Memo::new(move |_| parsed.get().0);
    let minute = Memo::new(move |_| parsed.get().1);
    let text = move || format!("{:02}:{:02}", hour.get(), minute.get());
    let open = RwSignal::new(false);

    // 打开时把当前小时、分钟滚到列中间（避开上下渐隐）。
    Effect::new(move |_| {
        if open.get() {
            #[cfg(target_arch = "wasm32")]
            if let Some(n) = prefix.get_untracked() {
                scroll_into_center(&format!("time-picker-{n}-h"));
                scroll_into_center(&format!("time-picker-{n}-m"));
            }
        }
    });

    // UTC 换算依赖浏览器时区，仅在客户端（打开浮层后）显示。
    let utc = move || {
        cfg!(target_arch = "wasm32").then(|| crate::utils::time::local_hhmm_to_utc(&text()))
    };

    let close = move || {
        open.set(false);
        #[cfg(target_arch = "wasm32")]
        if let Some(trigger) = trigger_id.get_untracked() {
            focus_by_id(&trigger);
        }
    };

    let hour_id = move |h: u8| {
        (h == hour.get())
            .then(|| prefix.get().map(|n| format!("time-picker-{n}-h")))
            .flatten()
    };
    let minute_id = move |m: u8| {
        (m == minute.get())
            .then(|| prefix.get().map(|n| format!("time-picker-{n}-m")))
            .flatten()
    };

    view! {
        <div class="relative inline-block">
            <button
                id=move || trigger_id.get()
                type="button"
                class="select-trigger field w-40"
                aria-haspopup="dialog"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| open.set(true)
            >
                <span class="font-mono tabular-nums">{text}</span>
                <span class="text-xs text-fg-3">"本地时间"</span>
            </button>
            {move || {
                open.get()
                    .then(|| {
                        view! {
                            <div class="fixed inset-0 z-40" on:click=move |_| open.set(false)></div>
                            <div
                                class="listbox time-panel"
                                data-placement="bottom"
                                role="dialog"
                                aria-label="选择时间"
                                on:keydown=move |e: KeyboardEvent| {
                                    if e.key() == "Escape" {
                                        e.prevent_default();
                                        close();
                                    }
                                }
                            >
                                <div class="time-columns">
                                    <div
                                        class="time-column"
                                        role="listbox"
                                        aria-label="小时"
                                        on:keydown=move |e: KeyboardEvent| {
                                            let delta = match e.key().as_str() {
                                                "ArrowDown" => 1,
                                                "ArrowUp" => -1,
                                                _ => return,
                                            };
                                            e.prevent_default();
                                            let h = wrap_index(hour.get_untracked() as usize, delta, 24);
                                            onchange.run(format!("{h:02}:{:02}", minute.get_untracked()));
                                        }
                                    >
                                        {(0..24u8)
                                            .map(|h| {
                                                view! {
                                                    <button
                                                        id=move || hour_id(h)
                                                        class="time-cell"
                                                        type="button"
                                                        role="option"
                                                        aria-selected=move || (h == hour.get()).to_string()
                                                        tabindex=move || if h == hour.get() { "0" } else { "-1" }
                                                        on:click=move |_| {
                                                            onchange.run(format!("{h:02}:{:02}", minute.get_untracked()))
                                                        }
                                                    >
                                                        {format!("{h:02}")}
                                                    </button>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                    <div
                                        class="time-column"
                                        role="listbox"
                                        aria-label="分钟"
                                        on:keydown=move |e: KeyboardEvent| {
                                            let delta = match e.key().as_str() {
                                                "ArrowDown" => 1,
                                                "ArrowUp" => -1,
                                                _ => return,
                                            };
                                            e.prevent_default();
                                            let m = wrap_index(minute.get_untracked() as usize, delta, 60);
                                            onchange.run(format!("{:02}:{m:02}", hour.get_untracked()));
                                        }
                                    >
                                        {(0..60u8)
                                            .map(|m| {
                                                view! {
                                                    <button
                                                        id=move || minute_id(m)
                                                        class="time-cell"
                                                        type="button"
                                                        role="option"
                                                        aria-selected=move || (m == minute.get()).to_string()
                                                        tabindex=move || if m == minute.get() { "0" } else { "-1" }
                                                        on:click=move |_| {
                                                            onchange.run(format!("{:02}:{m:02}", hour.get_untracked()))
                                                        }
                                                    >
                                                        {format!("{m:02}")}
                                                    </button>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                </div>
                                <div class="time-footer">
                                    <span>{move || utc().map(|utc| format!("UTC {utc}"))}</span>
                                    <button
                                        class="btn btn-xs btn-primary"
                                        type="button"
                                        on:click=move |_| close()
                                    >
                                        "确定"
                                    </button>
                                </div>
                            </div>
                        }
                    })
            }}
        </div>
    }
}

/// 表单输入框组件。
///
/// Props：
/// - `id`：input 元素 id，用于与 label 关联
/// - `r#type`：input 类型（缺省 `"text"`；如 `"email"`、`"password"`、`"number"`）
/// - `placeholder`：占位提示文本（可选，不填则不输出属性）
/// - `value`：当前值（受控，传信号或 `move ||` 闭包）
/// - `disabled`：是否禁用（可选，缺省 `false`）
/// - `invalid`：是否校验未通过（红环，输出 `aria-invalid`）
/// - `required`：是否必填（缺省 `false`）
/// - `readonly`：是否只读（缺省 `false`）
/// - `autofocus`：挂载时是否自动聚焦（缺省 `false`）
/// - `maxlength`：最大字符数（可选）
/// - `autocomplete`：浏览器自动填充提示（可选，如 `"new-password"`）
/// - `min` / `step`：number 类输入的下限与步进（可选）
/// - `aria_describedby`：关联说明元素 id（可选）
/// - `oninput`：输入事件回调，返回新的字符串值（可选；只读框不需要）
/// - `onkeydown` / `onfocus` / `onblur`：可选的键盘与焦点事件回调
/// - `class`：自定义 class（可选，缺省用 [`INPUT_CLASS`] 全宽表单款）。
///   内联/固定宽度场景传自定义类串，覆盖默认样式。
/// - `mono`：是否使用等宽字体（代码片段、表名、JSON 等，缺省 `false`）
/// - `inputmode` / `title` / `aria_label`：可选的原生属性
#[component]
pub fn FormInput(
    #[prop(optional, into)] id: MaybeProp<String>,
    #[prop(default = "text")] r#type: &'static str,
    #[prop(optional)] placeholder: Option<&'static str>,
    #[prop(into)] value: Signal<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
    #[prop(optional, into)] invalid: Signal<bool>,
    #[prop(optional)] required: bool,
    #[prop(optional)] readonly: bool,
    #[prop(optional)] autofocus: bool,
    #[prop(optional)] maxlength: Option<i32>,
    #[prop(optional)] autocomplete: Option<&'static str>,
    #[prop(optional)] min: Option<&'static str>,
    #[prop(optional)] step: Option<&'static str>,
    #[prop(optional)] aria_describedby: Option<&'static str>,
    #[prop(optional, into)] oninput: Option<Callback<String>>,
    #[prop(optional, into)] onkeydown: Option<Callback<KeyboardEvent>>,
    #[prop(optional, into)] onfocus: Option<Callback<FocusEvent>>,
    #[prop(optional, into)] onblur: Option<Callback<FocusEvent>>,
    #[prop(optional)] class: Option<&'static str>,
    #[prop(optional)] mono: bool,
    #[prop(optional)] inputmode: Option<&'static str>,
    #[prop(optional)] title: Option<&'static str>,
    #[prop(optional)] aria_label: Option<&'static str>,
) -> impl IntoView {
    let base = class.unwrap_or(INPUT_CLASS);
    let mono_class = if mono { " font-mono" } else { "" };
    let class = move || {
        let disabled_class = if disabled.get() {
            " opacity-60 cursor-not-allowed"
        } else {
            ""
        };
        format!("{base}{mono_class}{disabled_class}")
    };
    view! {
        <input
            id=move || id.get()
            class=class
            type=r#type
            placeholder=placeholder
            value=value.get_untracked()
            prop:value=move || value.get()
            disabled=move || disabled.get()
            required=required
            readonly=readonly
            autofocus=autofocus
            maxlength=maxlength.map(|n| n.to_string())
            autocomplete=autocomplete
            min=min
            step=step
            aria-describedby=aria_describedby
            aria-invalid=move || invalid.get().then_some("true")
            inputmode=inputmode
            title=title
            aria-label=aria_label
            on:input=move |e| {
                if let Some(handler) = oninput {
                    handler.run(event_target_value(&e));
                }
            }
            on:keydown=move |e| {
                if let Some(handler) = onkeydown {
                    handler.run(e);
                }
            }
            on:focus=move |e| {
                if let Some(handler) = onfocus {
                    handler.run(e);
                }
            }
            on:blur=move |e| {
                if let Some(handler) = onblur {
                    handler.run(e);
                }
            }
        />
    }
}

/// 多行输入框：外观同 FormInput，`.field-textarea` 随内容长高，
/// `rows` 是不支持 field-sizing 时的初始行数。
#[component]
pub fn FormTextarea(
    #[prop(optional, into)] id: MaybeProp<String>,
    #[prop(into)] value: Signal<String>,
    #[prop(optional)] placeholder: Option<&'static str>,
    #[prop(default = 3)] rows: u32,
    #[prop(optional)] maxlength: Option<i32>,
    #[prop(optional, into)] invalid: Signal<bool>,
    #[prop(into)] oninput: Callback<String>,
    #[prop(optional)] class: Option<&'static str>,
) -> impl IntoView {
    view! {
        <textarea
            id=move || id.get()
            class=class.unwrap_or(TEXTAREA_CLASS)
            rows=rows
            maxlength=maxlength.map(|n| n.to_string())
            placeholder=placeholder
            prop:value=move || value.get()
            aria-invalid=move || invalid.get().then_some("true")
            on:input=move |e| oninput.run(event_target_value(&e))
        >
            {value.get_untracked()}
        </textarea>
    }
}

/// 数字输入框：单位写在框内右侧，两侧 −/+ 步进；越界时边框与说明一起变红。
///
/// 输入过程中保留草稿文本，只有在 `[min, max]` 内的整数才回调 `onchange`；
/// 越界或非数字时显示红环与范围说明，不改动父组件的值。
///
/// Props：
/// - `id`：input id，用于与 label 关联（缺省挂载后用内部计数器生成）
/// - `value`：当前值（受控，可传信号）
/// - `min` / `max`：允许范围（含端点）
/// - `step`：步进，缺省 1
/// - `unit`：单位，如「天」「次/分钟」
/// - `hint`：范围内时显示的说明（可选）
/// - `aria_label`：没有可见标签时的无障碍名称（可选）
/// - `onchange`：合法新值回调
#[component]
pub fn NumberInput(
    #[prop(optional, into)] id: MaybeProp<String>,
    #[prop(into)] value: Signal<i64>,
    min: i64,
    max: i64,
    #[prop(default = 1)] step: i64,
    #[prop(optional)] unit: Option<&'static str>,
    #[prop(optional, into)] hint: MaybeProp<String>,
    #[prop(optional)] aria_label: Option<&'static str>,
    #[prop(into)] onchange: Callback<i64>,
) -> impl IntoView {
    let prefix = use_instance_prefix();
    let input_id = Signal::derive(move || {
        id.get()
            .or_else(|| prefix.get().map(|n| format!("number-input-{n}")))
    });
    let help_id = Signal::derive(move || input_id.get().map(|id| format!("{id}-help")));
    let draft = RwSignal::new(value.get_untracked().to_string());
    // 父组件改值（外部重置、保存回填）时同步草稿。
    Effect::new(move |previous: Option<i64>| {
        let current = value.get();
        if previous.is_some_and(|p| p != current) {
            draft.set(current.to_string());
        }
        current
    });

    let parsed = Memo::new(move |_| parse_in_range(&draft.get(), min, max));
    let invalid = move || parsed.get().is_none();
    let current = move || parsed.get().unwrap_or_else(|| value.get());
    let help = move || {
        if invalid() {
            Some(format!("超出范围：请输入 {min} 到 {max} 之间的整数。"))
        } else {
            hint.get()
        }
    };
    let at_min = move || current() <= min;
    let at_max = move || current() >= max;
    let set = move |next: i64| {
        let next = next.clamp(min, max);
        draft.set(next.to_string());
        onchange.run(next);
    };

    view! {
        <div>
            <div class="field number-field">
                <button
                    class="btn btn-sm btn-icon btn-ghost"
                    type="button"
                    aria-label="减少"
                    disabled=at_min
                    on:click=move |_| set(current() - step)
                >
                    <Icon name=IconName::Minus size=14/>
                </button>
                <input
                    id=move || input_id.get()
                    type="text"
                    inputmode="numeric"
                    value=draft.get_untracked()
                    prop:value=move || draft.get()
                    aria-label=aria_label
                    aria-invalid=move || invalid().then_some("true")
                    aria-describedby=move || help().and_then(|_| help_id.get())
                    on:input=move |e| {
                        let text: String = event_target_value(&e)
                            .chars()
                            .filter(|c| c.is_ascii_digit() || *c == '-')
                            .collect();
                        if let Some(v) = parse_in_range(&text, min, max) {
                            onchange.run(v);
                        }
                        draft.set(text);
                    }
                    on:keydown=move |e: KeyboardEvent| {
                        let delta = match e.key().as_str() {
                            "ArrowUp" => step,
                            "ArrowDown" => -step,
                            _ => return,
                        };
                        e.prevent_default();
                        set(current() + delta);
                    }
                />
                {unit.map(|unit| view! { <span class="number-unit">{unit}</span> })}
                <button
                    class="btn btn-sm btn-icon btn-ghost"
                    type="button"
                    aria-label="增加"
                    disabled=at_max
                    on:click=move |_| set(current() + step)
                >
                    <Icon name=IconName::Plus size=14/>
                </button>
            </div>
            {move || {
                help()
                    .map(|help| {
                        view! {
                            <p
                                id=move || help_id.get()
                                class="field-hint"
                                data-tone=move || if invalid() { "danger" } else { "" }
                            >
                                {help}
                            </p>
                        }
                    })
            }}
        </div>
    }
}

/// 解析草稿文本，在 `[min, max]` 内才返回。
fn parse_in_range(text: &str, min: i64, max: i64) -> Option<i64> {
    text.trim()
        .parse::<i64>()
        .ok()
        .filter(|v| (min..=max).contains(v))
}

/// 表单标签组件。
///
/// Props：
/// - `label`：标签文本
/// - `html_for`：关联的 input id
#[component]
pub fn FormLabel(
    #[prop(into)] label: Signal<String>,
    #[prop(optional, into)] html_for: MaybeProp<String>,
) -> impl IntoView {
    view! {
        <label class="field-label" for=move || html_for.get()>
            {move || label.get()}
        </label>
    }
}

/// 提示条，用于显示成功、错误等状态消息。
///
/// Props：
/// - `message`：提示文本
/// - `variant`：`"error"`、`"success"`，其他值为中性说明
#[component]
pub fn AlertBox(
    #[prop(into)] message: Signal<String>,
    #[prop(into)] variant: Signal<&'static str>,
) -> impl IntoView {
    let tone = move || match variant.get() {
        "error" => "danger",
        "success" => "success",
        _ => "",
    };
    let role = move || match variant.get() {
        "error" => "alert",
        _ => "status",
    };
    let icon = move || match variant.get() {
        "error" => IconName::CircleAlert,
        "success" => IconName::CircleCheck,
        _ => IconName::Info,
    };
    view! {
        <div class="alert mb-4" data-tone=tone role=role>
            {move || view! { <Icon name=icon() size=16/> }}
            <span>{move || message.get()}</span>
        </div>
    }
}

/// 开关（toggle switch）组件。
///
/// 设置项的布尔切换：轨道 36×22，开启陶土色；圆点以 bouncy 弹簧滑动。
/// accessibility：`role="switch"` + `aria-checked`，键盘焦点陶土双环。
///
/// Props：
/// - `checked`：当前开关状态（可传信号）
/// - `ontoggle`：点击切换回调（父组件在回调内翻转 signal 并触发副作用）
/// - `aria_label`：没有可见标签时的无障碍名称
/// - `disabled`：是否禁用（缺省 `false`）
#[component]
pub fn ToggleSwitch(
    #[prop(into)] checked: Signal<bool>,
    #[prop(into)] ontoggle: Callback<()>,
    #[prop(optional, into)] aria_label: MaybeProp<String>,
    #[prop(optional, into)] disabled: Signal<bool>,
) -> impl IntoView {
    view! {
        <button
            type="button"
            role="switch"
            class="switch"
            aria-label=move || aria_label.get()
            aria-checked=move || checked.get().to_string()
            disabled=move || disabled.get()
            on:click=move |_| ontoggle.run(())
        >
            <span class="switch-thumb"></span>
        </button>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptos::reactive::owner::Owner;

    #[test]
    fn parse_hhmm_valid_values() {
        assert_eq!(parse_hhmm("00:00"), (0, 0));
        assert_eq!(parse_hhmm("04:30"), (4, 30));
        assert_eq!(parse_hhmm("23:59"), (23, 59));
    }

    #[test]
    fn parse_hhmm_invalid_falls_back_to_zero() {
        assert_eq!(parse_hhmm(""), (0, 0)); // 空串
        assert_eq!(parse_hhmm("12"), (0, 0)); // 缺分钟段
        assert_eq!(parse_hhmm("24:00"), (0, 0)); // 小时越界
        assert_eq!(parse_hhmm("12:60"), (0, 0)); // 分钟越界
        assert_eq!(parse_hhmm("ab:cd"), (0, 0)); // 非数字
    }

    #[test]
    fn wrap_index_cycles_both_directions() {
        assert_eq!(wrap_index(0, 1, 3), 1);
        assert_eq!(wrap_index(2, 1, 3), 0); // 末尾前进回绕到首
        assert_eq!(wrap_index(0, -1, 3), 2); // 首位后退回绕到尾
        assert_eq!(wrap_index(1, -1, 3), 0);
    }

    #[test]
    fn wrap_index_empty_is_zero() {
        assert_eq!(wrap_index(5, 1, 0), 0); // 空列表不越界
    }

    #[test]
    fn should_flip_only_when_below_insufficient_and_above_wider() {
        // 下方充足：不翻（below = 800-140 = 660 > 200+14）
        assert!(!should_flip(100.0, 140.0, 800.0, 200.0));
        // 下方不足且上方更宽：上翻（below = 160 < 214，above = 600 > 160）
        assert!(should_flip(600.0, 640.0, 800.0, 200.0));
        // 下方不足但上方更窄：保持向下（above = 30 < below = 190）
        assert!(!should_flip(30.0, 70.0, 260.0, 200.0));
        // 恰好放得下（below = 214 == 200+14，非严格小于）：不翻
        assert!(!should_flip(300.0, 340.0, 554.0, 200.0));
    }

    #[test]
    fn step_active_skips_filtered_and_disabled_options() {
        // 可选项为 1、3、4（0、2 被筛掉或禁用）
        let navigable = [1, 3, 4];
        assert_eq!(step_active(&navigable, 1, 1), Some(3));
        assert_eq!(step_active(&navigable, 4, 1), Some(1)); // 末尾回绕
        assert_eq!(step_active(&navigable, 1, -1), Some(4)); // 首位回绕
                                                             // 当前项被筛掉：向下落到首项，向上落到末项
        assert_eq!(step_active(&navigable, 2, 1), Some(1));
        assert_eq!(step_active(&navigable, 2, -1), Some(4));
        assert_eq!(step_active(&[], 0, 1), None);
    }

    #[test]
    fn filter_matches_case_insensitive_substrings() {
        assert!(matches_query("PostgreSQL", "sql"));
        assert!(matches_query("最新上传", "上传"));
        assert!(matches_query("任意", "  "));
        assert!(!matches_query("按文件名", "体积"));
    }

    #[test]
    fn number_input_accepts_only_values_in_range() {
        assert_eq!(parse_in_range("30", 1, 90), Some(30));
        assert_eq!(parse_in_range(" 90 ", 1, 90), Some(90));
        assert_eq!(parse_in_range("0", 1, 90), None);
        assert_eq!(parse_in_range("120", 1, 90), None);
        assert_eq!(parse_in_range("", 1, 90), None);
        assert_eq!(parse_in_range("-", 1, 90), None);
    }

    // ---- SSR 渲染（Leptos）：覆盖各组件在常见传参写法下的静态结构 ----

    fn render<V: IntoView + 'static>(build: impl FnOnce() -> V) -> String {
        Owner::new().with(|| build().to_html())
    }

    #[test]
    fn form_input_renders_attributes_and_state_classes() {
        let html = render(|| {
            view! {
                <FormInput
                    id="login-name"
                    r#type="email"
                    placeholder="邮箱"
                    value="a@b.c"
                    invalid=true
                    disabled=true
                    mono=true
                    maxlength=64
                    autocomplete="off"
                    oninput=move |_: String| {}
                />
            }
        });
        assert!(html.contains("id=\"login-name\""), "{html}");
        assert!(html.contains("type=\"email\""), "{html}");
        assert!(html.contains("placeholder=\"邮箱\""), "{html}");
        assert!(html.contains("value=\"a@b.c\""), "{html}");
        assert!(html.contains("aria-invalid=\"true\""), "{html}");
        assert!(html.contains("maxlength=\"64\""), "{html}");
        assert!(
            html.contains("field w-full font-mono opacity-60 cursor-not-allowed"),
            "{html}"
        );
        assert!(html.contains("disabled"), "{html}");
    }

    #[test]
    fn form_input_omits_unset_optional_attributes() {
        let html = render(|| {
            let value = RwSignal::new(String::new());
            view! { <FormInput value=value/> }
        });
        assert!(!html.contains("id="), "{html}");
        assert!(!html.contains("aria-invalid"), "{html}");
        assert!(!html.contains("inputmode"), "{html}");
        assert!(!html.contains("title"), "{html}");
        assert!(html.contains("class=\"field w-full\""), "{html}");
    }

    #[test]
    fn form_textarea_renders_initial_text_and_rows() {
        let html = render(|| {
            view! {
                <FormTextarea id="bio" value="第一行" rows=5 invalid=true oninput=move |_: String| {}/>
            }
        });
        assert!(html.contains("<textarea"), "{html}");
        assert!(html.contains("rows=\"5\""), "{html}");
        assert!(html.contains("field field-textarea"), "{html}");
        assert!(html.contains("aria-invalid=\"true\""), "{html}");
        assert!(html.contains("第一行</textarea>"), "{html}");
    }

    #[test]
    fn alert_box_maps_variant_to_tone_role_and_icon() {
        let error = render(|| view! { <AlertBox message="出错了" variant="error"/> });
        assert!(error.contains("data-tone=\"danger\""), "{error}");
        assert!(error.contains("role=\"alert\""), "{error}");
        assert!(error.contains("lucide-circle-alert"), "{error}");
        assert!(error.contains("出错了"), "{error}");

        let success = render(|| view! { <AlertBox message="成功" variant="success"/> });
        assert!(success.contains("data-tone=\"success\""), "{success}");
        assert!(success.contains("role=\"status\""), "{success}");

        let info = render(|| view! { <AlertBox message="提示" variant="info"/> });
        assert!(info.contains("data-tone=\"\""), "{info}");
        assert!(info.contains("lucide-info"), "{info}");
    }

    #[test]
    fn toggle_switch_exposes_switch_semantics() {
        let html = render(|| {
            let checked = RwSignal::new(true);
            view! { <ToggleSwitch checked=checked aria_label="启用缓存" ontoggle=move |_| {}/> }
        });
        assert!(html.contains("role=\"switch\""), "{html}");
        assert!(html.contains("aria-checked=\"true\""), "{html}");
        assert!(html.contains("aria-label=\"启用缓存\""), "{html}");
        assert!(html.contains("switch-thumb"), "{html}");
    }

    #[test]
    fn form_label_associates_with_input() {
        let html = render(|| view! { <FormLabel label="用户名" html_for="login-name"/> });
        assert!(html.contains("class=\"field-label\""), "{html}");
        assert!(html.contains("for=\"login-name\""), "{html}");
        assert!(html.contains("用户名"), "{html}");
    }

    #[test]
    fn form_select_closed_renders_only_trigger_with_selected_label() {
        let html = render(|| {
            let sort = RwSignal::new(2_u8);
            view! {
                <FormSelect
                    id="sort"
                    value=sort
                    options=vec![(1_u8, "最新上传"), (2, "文件最大")]
                    trigger_class=FORM_SELECT_COMPACT_CLASS
                    aria_label="排序"
                    onchange=move |v: u8| sort.set(v)
                />
            }
        });
        assert!(html.contains("id=\"sort\""), "{html}");
        assert!(html.contains("aria-haspopup=\"listbox\""), "{html}");
        assert!(html.contains("aria-expanded=\"false\""), "{html}");
        assert!(html.contains("aria-label=\"排序\""), "{html}");
        assert!(
            html.contains("select-trigger field field-sm w-auto"),
            "{html}"
        );
        assert!(html.contains("文件最大"), "{html}");
        assert!(!html.contains("role=\"listbox\""), "{html}");
        assert!(!html.contains("最新上传"), "{html}");
    }

    #[test]
    fn form_select_falls_back_to_first_option_for_unknown_value() {
        let html = render(|| {
            view! {
                <FormSelect
                    value=Signal::derive(|| "zzz".to_string())
                    options=vec![("a".to_string(), "甲".to_string()), ("b".to_string(), "乙".to_string())]
                    onchange=move |_: String| {}
                />
            }
        });
        assert!(html.contains("甲"), "{html}");
    }

    /// 页面里常见的 FormSelect 传参写法都要能编译并推断出类型参数。
    #[test]
    fn form_select_accepts_common_call_shapes() {
        const REFRESH: [(u32, &str); 2] = [(0, "关闭"), (5000, "5 秒")];
        let html = render(|| {
            let refresh = RwSignal::new(5000_u32);
            let tag = RwSignal::new("rust".to_string());
            let published = RwSignal::new(None::<bool>);
            let tags = RwSignal::new(vec![
                ("all".to_string(), "全部".to_string()),
                ("rust".to_string(), "Rust".to_string()),
            ]);
            view! {
                // 信号值 + 常量表 + 无标注闭包参数
                <FormSelect value=refresh options=REFRESH.to_vec() onchange=move |v| refresh.set(v)/>
                // 闭包值 + 响应式选项 + 标注参数
                <FormSelect
                    value=move || tag.get()
                    options=move || tags.get()
                    onchange=move |v: String| tag.set(v)
                />
                // Option 值 + 内联字面量选项
                <FormSelect
                    value=published
                    options=vec![(None, "公开状态"), (Some(true), "已公开"), (Some(false), "私密")]
                    onchange={move |v: Option<bool>| published.set(v)}
                />
                // 整数字面量值 + 禁用项
                <FormSelect
                    value=3_u32
                    options=vec![(1, "1"), (3, "3")]
                    disabled_options=vec![(1, "需要先重建索引")]
                    onchange=move |_: u32| {}
                />
            }
        });
        assert!(html.contains("5 秒"), "{html}");
        assert!(html.contains("Rust"), "{html}");
        assert!(html.contains("公开状态"), "{html}");
    }

    #[test]
    fn time_picker_closed_shows_normalized_time() {
        let html = render(|| {
            view! {
                <TimePicker id="backup-time" value="4:05" onchange=move |_: String| {}/>
            }
        });
        // "4:05" 可解析为 (4, 5)，补零后显示。
        assert!(html.contains("04:05"), "{html}");
        assert!(html.contains("本地时间"), "{html}");
        assert!(html.contains("aria-haspopup=\"dialog\""), "{html}");
        assert!(!html.contains("time-panel"), "{html}");
    }

    #[test]
    fn number_input_renders_unit_and_range_error() {
        let ok = render(|| {
            view! {
                <NumberInput id="days" value=30_i64 min=1 max=90 unit="天" hint="保留天数" onchange=move |_: i64| {}/>
            }
        });
        assert!(ok.contains("number-unit"), "{ok}");
        assert!(ok.contains("天"), "{ok}");
        assert!(ok.contains("保留天数"), "{ok}");
        assert!(ok.contains("aria-describedby=\"days-help\""), "{ok}");
        assert!(ok.contains("data-tone=\"\""), "{ok}");

        let bad = render(|| {
            view! {
                <NumberInput id="days" value=120_i64 min=1 max=90 onchange=move |_: i64| {}/>
            }
        });
        assert!(bad.contains("data-tone=\"danger\""), "{bad}");
        assert!(bad.contains("aria-invalid=\"true\""), "{bad}");
        assert!(
            bad.contains("超出范围：请输入 1 到 90 之间的整数。"),
            "{bad}"
        );
    }
}
