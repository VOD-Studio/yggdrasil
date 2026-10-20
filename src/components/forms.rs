//! 表单控件组件
//!
//! 提供登录、注册、评论等页面共享的输入框、下拉、时间、数字、开关与提示条。
//! 外观定义在 styles/components/ui.css（`.field`、`.listbox`、`.switch`、`.alert`）。

use crate::components::ui::{Icon, IconName};
use dioxus::prelude::*;

/// 输入框基础类：浅底、发丝线环，聚焦陶土双环，`aria-invalid` 时红环。
pub const INPUT_CLASS: &str = "field w-full";

/// 内联输入框：与 [`INPUT_CLASS`] 同外观，用 `flex-1 min-w-0` 填充剩余宽度，
/// 用于与按钮并排的场景（搜索栏、URL 输入栏等）。
pub const INPUT_INLINE_CLASS: &str = "field flex-1 min-w-0";

/// 多行输入框：随内容长高（`field-sizing: content`）。
#[allow(dead_code)] // 文本域随页面重写逐步改用
pub const TEXTAREA_CLASS: &str = "field field-textarea w-full";

/// FormSelect 实例 id 计数器（跨泛型单例化全局唯一）。
static FORM_SELECT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// FormSelect 紧凑触发器：工具栏内联小下拉（自动刷新、导出格式等）。
pub const FORM_SELECT_COMPACT_CLASS: &str = "field field-sm w-auto";

/// 选项多于该数量时，下拉面板顶部出现筛选框。
const FILTER_THRESHOLD: usize = 8;

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
/// - 透明遮罩拦截外部点击关闭；选项 `onmousedown` 阻止默认行为，点击不夺走焦点；
/// - 泛型值绑定：`onchange` 直接回传选中项的 `T`，无需字符串反查。
///
/// Props：
/// - `id`：触发器 id，用于与 label 关联（缺省用内部计数器生成）
/// - `value`：当前选中项（受控）
/// - `options`：可选项 `(值, 标签)` 列表
/// - `disabled_options`：不可用的项及原因 `(值, 原因)`
/// - `onchange`：选中变化回调，回传新选中项的值
/// - `trigger_class`：触发器外观（缺省全宽 `field`；工具栏传 [`FORM_SELECT_COMPACT_CLASS`]）
/// - `aria_label`：触发器无障碍标签；可见文本不足以表意时传入
#[component]
pub fn FormSelect<T: Clone + PartialEq + 'static>(
    id: Option<String>,
    value: T,
    options: Vec<(T, &'static str)>,
    onchange: EventHandler<T>,
    #[props(default)] disabled_options: Vec<(T, &'static str)>,
    #[props(default)] trigger_class: Option<&'static str>,
    #[props(default)] aria_label: Option<&'static str>,
) -> Element {
    let trigger_cls = trigger_class.unwrap_or("field");
    let id_prefix = use_hook(|| FORM_SELECT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst));

    // 受控选中序号；value 不在 options 时兜底 0（与浏览器默认选中第一项一致）。
    let selected = options.iter().position(|(v, _)| *v == value).unwrap_or(0);
    let selected_label = options.get(selected).map(|(_, l)| *l).unwrap_or_default();
    let len = options.len();
    let filterable = len > FILTER_THRESHOLD;

    let mut open = use_signal(|| false);
    let mut active = use_signal(|| selected);
    let mut query = use_signal(String::new);
    // flip_up 的 set 均在 wasm 门控语句内（宿主构建下只读）。
    #[allow(unused_mut)]
    let mut flip_up = use_signal(|| false);

    let trigger_id = id.unwrap_or_else(|| format!("form-select-{id_prefix}"));
    let filter_id = format!("form-select-{id_prefix}-filter");

    // 每项的禁用原因；与 options 一一对应。
    let reasons: Vec<Option<&'static str>> = options
        .iter()
        .map(|(v, _)| {
            disabled_options
                .iter()
                .find(|(d, _)| d == v)
                .map(|(_, reason)| *reason)
        })
        .collect();
    let q = query();
    let visible: Vec<usize> = (0..len)
        .filter(|&i| matches_query(options[i].1, &q))
        .collect();
    let navigable: Vec<usize> = visible
        .iter()
        .copied()
        .filter(|&i| reasons[i].is_none())
        .collect();
    // 筛选框输入时用来重算首个可选项；渲染用的逐行数据。
    let searchable: Vec<(&'static str, bool)> = options
        .iter()
        .zip(&reasons)
        .map(|((_, label), reason)| (*label, reason.is_none()))
        .collect();
    let rows: Vec<(usize, T, &'static str, Option<&'static str>)> = visible
        .iter()
        .map(|&i| (i, options[i].0.clone(), options[i].1, reasons[i]))
        .collect();
    let nothing_visible = rows.is_empty();

    // 打开或键盘导航时，把高亮项滚入面板可视区；带筛选框时打开即聚焦筛选框。
    #[cfg(target_arch = "wasm32")]
    let filter_id_effect = filter_id.clone();
    use_effect(move || {
        if open() {
            #[cfg(target_arch = "wasm32")]
            {
                let idx = active();
                scroll_option_into_view(&format!("form-select-{id_prefix}-opt-{idx}"));
                if filterable {
                    focus_by_id(&filter_id_effect);
                }
            }
        }
    });

    let open_panel = {
        #[cfg(target_arch = "wasm32")]
        let trigger_id = trigger_id.clone();
        move || {
            #[cfg(target_arch = "wasm32")]
            flip_up.set(measure_flip(&trigger_id, len.min(FILTER_THRESHOLD + 1)));
            query.set(String::new());
            active.set(selected);
            open.set(true);
        }
    };

    // 面板打开后的键盘处理；触发器与筛选框共用。返回 true 表示已处理。
    let options_for_keys = options.clone();
    let navigable_for_keys = navigable.clone();
    #[cfg(target_arch = "wasm32")]
    let trigger_id_keys = trigger_id.clone();
    let mut handle_open_key = move |e: &KeyboardEvent| {
        let key = e.key();
        let is_space = matches!(&key, Key::Character(s) if s == " ");
        if key == Key::ArrowDown || key == Key::ArrowUp {
            e.prevent_default();
            let delta = if key == Key::ArrowDown { 1 } else { -1 };
            if let Some(next) = step_active(&navigable_for_keys, active(), delta) {
                active.set(next);
            }
        } else if key == Key::Home {
            e.prevent_default();
            if let Some(&first) = navigable_for_keys.first() {
                active.set(first);
            }
        } else if key == Key::End {
            e.prevent_default();
            if let Some(&last) = navigable_for_keys.last() {
                active.set(last);
            }
        } else if key == Key::Enter || (is_space && !filterable) {
            e.prevent_default();
            if navigable_for_keys.contains(&active()) {
                open.set(false);
                if let Some((v, _)) = options_for_keys.get(active()) {
                    onchange.call(v.clone());
                }
                #[cfg(target_arch = "wasm32")]
                focus_by_id(&trigger_id_keys);
            }
        } else if key == Key::Escape {
            e.prevent_default();
            open.set(false);
            #[cfg(target_arch = "wasm32")]
            focus_by_id(&trigger_id_keys);
        } else if key == Key::Tab {
            open.set(false);
        }
    };
    let mut handle_filter_key = handle_open_key.clone();

    let active_idx = active();
    let placement = if flip_up() { "top" } else { "bottom" };
    let active_descendant =
        (open() && !filterable).then(|| format!("form-select-{id_prefix}-opt-{}", active()));
    // wasm 下闭包捕获了 trigger_id（String），不是 Copy；宿主构建下是 Copy。
    #[allow(clippy::clone_on_copy)]
    let mut open_from_click = open_panel.clone();
    let mut open_from_key = open_panel;

    rsx! {
        div { class: "relative",
            button {
                id: "{trigger_id}",
                r#type: "button",
                class: "select-trigger {trigger_cls}",
                aria_haspopup: "listbox",
                aria_expanded: "{open()}",
                aria_activedescendant: active_descendant,
                aria_label,
                onclick: move |_| {
                    // 打开态下触发器被透明遮罩盖住，点击落在遮罩上即关闭；
                    // 这里只需处理「未开 → 开」。
                    if !open() {
                        open_from_click();
                    }
                },
                onkeydown: move |e| {
                    if open() {
                        handle_open_key(&e);
                        return;
                    }
                    let key = e.key();
                    let is_space = matches!(&key, Key::Character(s) if s == " ");
                    if key == Key::ArrowDown || key == Key::ArrowUp || key == Key::Enter || is_space {
                        e.prevent_default();
                        open_from_key();
                    }
                },
                span { class: "select-value", "{selected_label}" }
                Icon { name: IconName::ChevronDown, size: 15, class: "select-chevron" }
            }

            if open() {
                // 透明遮罩：拦截外部点击（点遮罩即关），z-40 < 面板 z-50。
                div { class: "fixed inset-0 z-40", onclick: move |_| open.set(false) }
                ul {
                    class: "listbox",
                    "data-placement": placement,
                    role: "listbox",
                    aria_labelledby: "{trigger_id}",
                    if filterable {
                        li { class: "listbox-filter", role: "presentation",
                            input {
                                id: "{filter_id}",
                                class: "field field-sm w-full",
                                r#type: "text",
                                placeholder: "筛选",
                                autocomplete: "off",
                                value: "{q}",
                                aria_label: "筛选选项",
                                aria_activedescendant: format!("form-select-{id_prefix}-opt-{active_idx}"),
                                oninput: move |e| {
                                    let text = e.value();
                                    if let Some(first) = searchable
                                        .iter()
                                        .position(|(label, enabled)| *enabled && matches_query(label, &text))
                                    {
                                        active.set(first);
                                    }
                                    query.set(text);
                                },
                                onkeydown: move |e| handle_filter_key(&e),
                            }
                        }
                    }
                    if nothing_visible {
                        li { class: "listbox-empty", role: "presentation", "没有匹配的选项" }
                    }
                    for (i, value, label, reason) in rows {
                        SelectOption {
                            key: "{i}",
                            id: format!("form-select-{id_prefix}-opt-{i}"),
                            label,
                            reason,
                            selected: i == selected,
                            active: i == active_idx,
                            on_hover: move |_| {
                                if reason.is_none() {
                                    active.set(i);
                                }
                            },
                            on_pick: {
                                #[cfg(target_arch = "wasm32")]
                                let trigger_id = trigger_id.clone();
                                move |_| {
                                    open.set(false);
                                    onchange.call(value.clone());
                                    #[cfg(target_arch = "wasm32")]
                                    focus_by_id(&trigger_id);
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

/// 下拉面板中的一项。禁用项（`reason` 为 Some）变淡、写明原因，不响应点击。
#[component]
fn SelectOption(
    id: String,
    label: &'static str,
    reason: Option<&'static str>,
    selected: bool,
    active: bool,
    on_hover: EventHandler<()>,
    on_pick: EventHandler<()>,
) -> Element {
    rsx! {
        li {
            id,
            class: "listbox-option",
            role: "option",
            aria_selected: "{selected}",
            aria_disabled: reason.is_some().then_some("true"),
            "data-active": "{active}",
            // 阻止 mousedown 默认行为：点击选项不夺走焦点。
            onmousedown: move |e| e.prevent_default(),
            onmouseenter: move |_| on_hover.call(()),
            onclick: move |_| {
                if reason.is_none() {
                    on_pick.call(());
                }
            },
            span { class: "min-w-0 flex-1",
                span { class: "block truncate", "{label}" }
                if let Some(reason) = reason {
                    span { class: "listbox-option-hint", "{reason}" }
                }
            }
            if selected {
                Icon { name: IconName::Check, size: 14, class: "listbox-check" }
            }
        }
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
/// - `id`：触发器 id，用于与 label 关联（缺省用内部计数器生成）
/// - `value`：当前值 "HH:MM"（受控；非法值回退显示 00:00，不 panic）
/// - `onchange`：选中变化回调，回传组合后的 "HH:MM"
#[component]
pub fn TimePicker(id: Option<String>, value: String, onchange: EventHandler<String>) -> Element {
    let id_prefix = use_hook(|| FORM_SELECT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
    let trigger_id = id.unwrap_or_else(|| format!("time-picker-{id_prefix}"));
    let (hour, minute) = parse_hhmm(&value);
    let text = format!("{hour:02}:{minute:02}");
    let mut open = use_signal(|| false);

    // 打开时把当前小时、分钟滚到列中间（避开上下渐隐）。
    use_effect(move || {
        if open() {
            #[cfg(target_arch = "wasm32")]
            {
                scroll_into_center(&format!("time-picker-{id_prefix}-h"));
                scroll_into_center(&format!("time-picker-{id_prefix}-m"));
            }
        }
    });

    // UTC 换算依赖浏览器时区，仅在客户端打开浮层后显示。
    #[cfg(target_arch = "wasm32")]
    let utc = Some(crate::utils::time::local_hhmm_to_utc(&text));
    #[cfg(not(target_arch = "wasm32"))]
    let utc: Option<String> = None;

    #[cfg(target_arch = "wasm32")]
    let trigger_for_close = trigger_id.clone();
    let close = move || {
        open.set(false);
        #[cfg(target_arch = "wasm32")]
        focus_by_id(&trigger_for_close);
    };
    // 同上：wasm 下捕获 String，宿主构建下是 Copy。
    #[allow(clippy::clone_on_copy)]
    let mut close_key = close.clone();
    let mut close_done = close;

    rsx! {
        div { class: "relative inline-block",
            button {
                id: "{trigger_id}",
                r#type: "button",
                class: "select-trigger field w-40",
                aria_haspopup: "dialog",
                aria_expanded: "{open()}",
                onclick: move |_| open.set(true),
                span { class: "font-mono tabular-nums", "{text}" }
                span { class: "text-xs text-fg-3", "本地时间" }
            }
            if open() {
                div { class: "fixed inset-0 z-40", onclick: move |_| open.set(false) }
                div {
                    class: "listbox time-panel",
                    "data-placement": "bottom",
                    role: "dialog",
                    aria_label: "选择时间",
                    onkeydown: move |e| {
                        if e.key() == Key::Escape {
                            e.prevent_default();
                            close_key();
                        }
                    },
                    div { class: "time-columns",
                        div {
                            class: "time-column",
                            role: "listbox",
                            aria_label: "小时",
                            onkeydown: move |e| {
                                let delta = match e.key() {
                                    Key::ArrowDown => 1,
                                    Key::ArrowUp => -1,
                                    _ => return,
                                };
                                e.prevent_default();
                                let h = wrap_index(hour as usize, delta, 24);
                                onchange.call(format!("{h:02}:{minute:02}"));
                            },
                            for h in 0..24u8 {
                                button {
                                    key: "{h}",
                                    id: if h == hour { format!("time-picker-{id_prefix}-h") } else { String::new() },
                                    class: "time-cell",
                                    r#type: "button",
                                    role: "option",
                                    aria_selected: "{h == hour}",
                                    tabindex: if h == hour { "0" } else { "-1" },
                                    onclick: move |_| onchange.call(format!("{h:02}:{minute:02}")),
                                    "{h:02}"
                                }
                            }
                        }
                        div {
                            class: "time-column",
                            role: "listbox",
                            aria_label: "分钟",
                            onkeydown: move |e| {
                                let delta = match e.key() {
                                    Key::ArrowDown => 1,
                                    Key::ArrowUp => -1,
                                    _ => return,
                                };
                                e.prevent_default();
                                let m = wrap_index(minute as usize, delta, 60);
                                onchange.call(format!("{hour:02}:{m:02}"));
                            },
                            for m in 0..60u8 {
                                button {
                                    key: "{m}",
                                    id: if m == minute { format!("time-picker-{id_prefix}-m") } else { String::new() },
                                    class: "time-cell",
                                    r#type: "button",
                                    role: "option",
                                    aria_selected: "{m == minute}",
                                    tabindex: if m == minute { "0" } else { "-1" },
                                    onclick: move |_| onchange.call(format!("{hour:02}:{m:02}")),
                                    "{m:02}"
                                }
                            }
                        }
                    }
                    div { class: "time-footer",
                        span {
                            if let Some(utc) = utc {
                                "UTC {utc}"
                            }
                        }
                        button {
                            class: "btn btn-xs btn-primary",
                            r#type: "button",
                            onclick: move |_| close_done(),
                            "确定"
                        }
                    }
                }
            }
        }
    }
}

/// 表单输入框组件。
///
/// Props：
/// - `id`：input 元素 id，用于与 label 关联
/// - `r#type`：input 类型（如 `"text"`、`"email"`、`"password"`）
/// - `placeholder`：占位提示文本
/// - `value`：当前值
/// - `disabled`：是否禁用（可选，缺省 `false`）
/// - `invalid`：是否校验未通过（红环，输出 `aria-invalid`）
/// - `oninput`：输入事件回调，返回新的字符串值
/// - `onkeydown`：可选的键盘事件回调
/// - `class`：自定义 class（可选，缺省用 [`INPUT_CLASS`] 全宽表单款）。
///   内联/固定宽度场景传 [`INPUT_INLINE_CLASS`] 或自定义类串，覆盖默认样式。
/// - `mono`：是否使用等宽字体（代码片段、表名、JSON 等，缺省 `false`）
#[component]
pub fn FormInput(
    id: Option<String>,
    r#type: &'static str,
    placeholder: &'static str,
    value: String,
    #[props(default)] disabled: bool,
    #[props(default)] invalid: bool,
    oninput: EventHandler<String>,
    #[props(default)] onkeydown: Option<EventHandler<KeyboardEvent>>,
    #[props(default)] onfocus: Option<EventHandler<FocusEvent>>,
    #[props(default)] onblur: Option<EventHandler<FocusEvent>>,
    #[props(default)] class: Option<&'static str>,
    #[props(default)] mono: bool,
    #[props(default)] inputmode: Option<&'static str>,
    #[props(default)] title: Option<&'static str>,
    #[props(default)] aria_label: Option<&'static str>,
) -> Element {
    let base = class.unwrap_or(INPUT_CLASS);
    let mono_class = if mono { " font-mono" } else { "" };
    rsx! {
        input {
            id: id.unwrap_or_default(),
            class: "{base}{mono_class}",
            r#type: "{r#type}",
            placeholder: "{placeholder}",
            value: "{value}",
            disabled,
            aria_invalid: invalid.then_some("true"),
            inputmode: inputmode.unwrap_or_default(),
            title: title.unwrap_or_default(),
            aria_label,
            oninput: move |e| oninput.call(e.value()),
            onkeydown: move |e| {
                if let Some(handler) = &onkeydown {
                    handler.call(e);
                }
            },
            onfocus: move |e| {
                if let Some(handler) = &onfocus {
                    handler.call(e);
                }
            },
            onblur: move |e| {
                if let Some(handler) = &onblur {
                    handler.call(e);
                }
            },
        }
    }
}

/// 数字输入框：单位写在框内右侧，两侧 −/+ 步进；越界时边框与说明一起变红。
///
/// 输入过程中保留草稿文本，只有在 `[min, max]` 内的整数才回调 `onchange`；
/// 越界或非数字时显示红环与范围说明，不改动父组件的值。
///
/// Props：
/// - `id`：input id，用于与 label 关联
/// - `value`：当前值（受控）
/// - `min` / `max`：允许范围（含端点）
/// - `step`：步进，缺省 1
/// - `unit`：单位，如「天」「次/分钟」
/// - `hint`：范围内时显示的说明（可选）
/// - `onchange`：合法新值回调
#[component]
pub fn NumberInput(
    id: Option<String>,
    value: i64,
    min: i64,
    max: i64,
    #[props(default = 1)] step: i64,
    #[props(default)] unit: Option<&'static str>,
    #[props(default)] hint: Option<String>,
    #[props(default)] aria_label: Option<&'static str>,
    onchange: EventHandler<i64>,
) -> Element {
    let id_prefix = use_hook(|| FORM_SELECT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
    let input_id = id.unwrap_or_else(|| format!("number-input-{id_prefix}"));
    let help_id = format!("{input_id}-help");
    let mut draft = use_signal(|| value.to_string());
    // 父组件改值（外部重置、保存回填）时同步草稿。
    let mut last_value = use_signal(|| value);
    if *last_value.peek() != value {
        last_value.set(value);
        draft.set(value.to_string());
    }

    let parsed = parse_in_range(&draft(), min, max);
    let invalid = parsed.is_none();
    let current = parsed.unwrap_or(value);
    let help = if invalid {
        Some(format!("超出范围：请输入 {min} 到 {max} 之间的整数。"))
    } else {
        hint
    };
    let described_by = help.as_ref().map(|_| help_id.clone());
    let mut set = move |next: i64| {
        let next = next.clamp(min, max);
        draft.set(next.to_string());
        onchange.call(next);
    };
    let mut set_dec = set;

    rsx! {
        div {
            div { class: "field number-field",
                button {
                    class: "btn btn-sm btn-icon btn-ghost",
                    r#type: "button",
                    aria_label: "减少",
                    disabled: current <= min,
                    onclick: move |_| set_dec(current - step),
                    Icon { name: IconName::Minus, size: 14 }
                }
                input {
                    id: "{input_id}",
                    r#type: "text",
                    inputmode: "numeric",
                    value: "{draft}",
                    aria_label,
                    aria_invalid: invalid.then_some("true"),
                    aria_describedby: described_by,
                    oninput: move |e| {
                        let text: String = e.value().chars().filter(|c| c.is_ascii_digit() || *c == '-').collect();
                        if let Some(v) = parse_in_range(&text, min, max) {
                            onchange.call(v);
                        }
                        draft.set(text);
                    },
                    onkeydown: move |e| {
                        let delta = match e.key() {
                            Key::ArrowUp => step,
                            Key::ArrowDown => -step,
                            _ => return,
                        };
                        e.prevent_default();
                        set(current + delta);
                    },
                }
                if let Some(unit) = unit {
                    span { class: "number-unit", "{unit}" }
                }
                button {
                    class: "btn btn-sm btn-icon btn-ghost",
                    r#type: "button",
                    aria_label: "增加",
                    disabled: current >= max,
                    onclick: move |_| set(current + step),
                    Icon { name: IconName::Plus, size: 14 }
                }
            }
            if let Some(help) = help {
                p {
                    id: "{help_id}",
                    class: "field-hint",
                    "data-tone": if invalid { "danger" } else { "" },
                    "{help}"
                }
            }
        }
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
pub fn FormLabel(label: String, html_for: Option<String>) -> Element {
    rsx! {
        label { class: "field-label", r#for: html_for.unwrap_or_default(), "{label}" }
    }
}

/// 提示条，用于显示成功、错误等状态消息。
///
/// Props：
/// - `message`：提示文本
/// - `variant`：`"error"`、`"success"`，其他值为中性说明
#[component]
pub fn AlertBox(message: String, variant: &'static str) -> Element {
    let (tone, icon, role) = match variant {
        "error" => ("danger", IconName::CircleAlert, "alert"),
        "success" => ("success", IconName::CircleCheck, "status"),
        _ => ("", IconName::Info, "status"),
    };
    rsx! {
        div { class: "alert mb-4", "data-tone": tone, role,
            Icon { name: icon, size: 16 }
            span { "{message}" }
        }
    }
}

/// 开关（toggle switch）组件。
///
/// 设置项的布尔切换：轨道 36×22，开启陶土色；圆点以 bouncy 弹簧滑动。
/// accessibility：`role="switch"` + `aria-checked`，键盘焦点陶土双环。
///
/// Props：
/// - `checked`：当前开关状态
/// - `ontoggle`：点击切换回调（父组件在回调内翻转 signal 并触发副作用）
/// - `aria_label`：没有可见标签时的无障碍名称
#[component]
pub fn ToggleSwitch(
    checked: bool,
    ontoggle: Callback<()>,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            role: "switch",
            class: "switch",
            aria_label: aria_label.unwrap_or_default(),
            aria_checked: "{checked}",
            disabled,
            onclick: move |_| ontoggle.call(()),
            span { class: "switch-thumb" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{matches_query, parse_hhmm, parse_in_range, should_flip, step_active, wrap_index};

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
}
