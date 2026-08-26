//! 表单控件组件
//!
//! 提供登录、注册、评论等页面共享的输入框、按钮与提示框样式常量与组件。

use dioxus::prelude::*;

/// 输入框基础 CSS 类，统一文本框、邮箱框、URL 框等样式。
pub const INPUT_CLASS: &str = "w-full px-4 py-2 border border-paper-border rounded-2xl bg-paper-entry text-paper-primary placeholder:text-paper-tertiary focus:outline-none focus:border-paper-accent focus:ring-1 focus:ring-paper-accent/30 transition-colors duration-200";

/// 内联输入框 CSS 类：与 [`INPUT_CLASS`] 同主题，但用 `flex-1 min-w-0` 取代 `w-full`，
/// 用于与按钮并排、需填充剩余宽度的场景（搜索栏、URL 输入栏等）。
pub const INPUT_INLINE_CLASS: &str = "flex-1 min-w-0 px-4 py-2 border border-paper-border rounded-2xl bg-paper-entry text-paper-primary placeholder:text-paper-tertiary focus:outline-none focus:border-paper-accent focus:ring-1 focus:ring-paper-accent/30 transition-colors duration-200";

/// 可清除输入框 CSS 类：与 [`INPUT_CLASS`] 同主题（`w-full` 撑满外层 relative 包裹），
/// 右侧 `pr-10` 为输入框内的自定义清除按钮（Material Symbols `close` 图标）让位，
/// `ygg-search-clear` 钩子隐藏 WebKit/Blink 原生 `::-webkit-search-cancel-button`
/// （见 input.css）。用于「输入框内带清除图标」场景（/search 页）。
pub const INPUT_SEARCH_CLASS: &str = "w-full pr-10 px-4 py-2 border border-paper-border rounded-2xl bg-paper-entry text-paper-primary placeholder:text-paper-tertiary focus:outline-none focus:border-paper-accent focus:ring-1 focus:ring-paper-accent/30 transition-colors duration-200 ygg-search-clear";

/// 主按钮 CSS 类，用于表单提交等主操作按钮。
///
/// 用 `text-paper-theme` 而非硬编码 `text-white`：`--color-paper-theme` 随主题反转
/// （浅色主题近白、深色主题近黑），配合深色主题下偏浅的 `--color-paper-accent`
/// 背景色（`#a6e3a1` 柔和绿）才能保住文字对比度——硬编码白字在深色主题下几乎不可读。
pub const BUTTON_PRIMARY_CLASS: &str = "w-full py-2.5 px-4 bg-paper-accent text-paper-theme font-medium rounded-full hover:brightness-110 active:scale-[0.98] transition-all duration-200 cursor-pointer";

/// FormSelect 实例 id 计数器（跨泛型单例化全局唯一）。
static FORM_SELECT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// FormSelect 紧凑触发器样式：工具栏内联小下拉（自动刷新、导出格式等）。
/// 自动宽度 + text-sm + 小圆角，chevron 与面板样式与默认表单款一致。
pub const FORM_SELECT_COMPACT_CLASS: &str = "inline-flex w-auto cursor-pointer select-none text-left text-sm pl-3 pr-8 py-1 border border-paper-border rounded-lg bg-paper-theme text-paper-primary focus:outline-none focus:border-paper-accent focus:ring-1 focus:ring-paper-accent/30 transition-colors duration-200";

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
    /// 面板与触发器的间隙（mt-1.5）加视口边缘留白。
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

/// 测量触发器视口位置，决定面板展开方向（仅 wasm；SSR 无 DOM）。
#[cfg(target_arch = "wasm32")]
pub(crate) fn measure_flip(trigger_id: &str, option_count: usize) -> bool {
    /// 选项行高：24px 行盒 + py-2.5（20px 垂直内边距）。
    const ROW_HEIGHT: f64 = 44.0;
    /// 面板 1px 边框 ×2 + p-1.5 内边距。
    const PANEL_CHROME: f64 = 14.0;
    /// 面板高度上限：max-h-60（240px）+ PANEL_CHROME。
    const PANEL_MAX: f64 = 254.0;

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

/// 把指定选项滚入面板可视区（打开/键盘导航时跟随；仅 wasm）。
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

/// 下拉选择框组件（自定义弹层，全主题化）。
///
/// 原生 `<select>` 的弹出列表由 OS/浏览器渲染，无法跟随主题（暗色下是白底
/// 系统菜单），故用 `button[aria-haspopup=listbox]` + 绝对定位面板重写：
/// - 面板/选项全部使用 Catppuccin 语义色，带 `select-enter` 入场动画；
/// - focus 始终留在触发器，键盘经 `aria-activedescendant` 高亮（↑↓ 循环、
///   Enter/Space 选中、Esc 关闭、Home/End 跳首尾、Tab 关闭并自然流转焦点）；
/// - 透明遮罩拦截外部点击关闭（同 Popover 模式）；选项 `onmousedown` 阻止默认
///   行为，点击不夺走触发器焦点；
/// - 打开时视口下方空间不足且上方更宽余则向上展开（`should_flip`）；
/// - 泛型值绑定：`onchange` 直接回传选中项的 `T`，无需字符串反查。
///
/// Props：
/// - `id`：触发器 id，用于与 label 关联（缺省用内部计数器生成）
/// - `value`：当前选中项（受控）
/// - `options`：可选项 `(值, 标签)` 列表
/// - `onchange`：选中变化回调，回传新选中项的值
/// - `aria_label`：触发器无障碍标签（可选）；可见文本不足以表意时传入，
///   如 TimePicker 的「小时」/「分钟」
#[component]
pub fn FormSelect<T: Clone + PartialEq + 'static>(
    id: Option<String>,
    value: T,
    options: Vec<(T, &'static str)>,
    onchange: EventHandler<T>,
    /// 触发器样式覆盖：缺省为全宽表单款；工具栏内联场景传
    /// [`FORM_SELECT_COMPACT_CLASS`]，或自定义类串（如编辑器底部胶囊）。
    #[props(default)]
    trigger_class: Option<&'static str>,
    /// 触发器无障碍标签：缺省不加 aria-label（触发器可见文本即标签）。
    #[props(default)]
    aria_label: Option<&'static str>,
) -> Element {
    // 面板与 POPOVER_PANEL_CLASS 同源（卡片化圆角 + 阴影）。宽度取 max(触发器,
    // 最长选项)：紧凑触发器（如“手动”）下面板仍能完整展示长选项；上限防出屏。
    // 水平以触发器中心居中：面板宽于触发器时两侧对称探出，窄触发器不失衡。
    // 居中用 [transform:translateX(-50%)] 而非 -translate-x-1/2 utility：Tailwind
    // v4 的 translate utility 走独立 translate 属性，会与 select-enter 关键帧的
    // transform 叠加造成双倍位移；关键帧的 fill 值与此处 transform 完全一致。
    // 定义在函数体内：模块级私有常量若仅被 wasm 门控调用点引用，会在 server
    // 构建下触发 dead_code。
    const TRIGGER_CLASS: &str = "w-full block cursor-pointer truncate select-none text-left pl-4 pr-10 py-2 border border-paper-border rounded-2xl bg-paper-entry text-paper-primary focus:outline-none focus:border-paper-accent focus:ring-1 focus:ring-paper-accent/30 transition-colors duration-200";
    const PANEL_CLASS: &str = "absolute left-1/2 z-50 w-max min-w-full max-w-[calc(100vw_-_2rem)] [transform:translateX(-50%)] max-h-60 overflow-y-auto rounded-2xl border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] p-1.5 shadow-lg animate-select-enter";

    let trigger_cls = trigger_class.unwrap_or(TRIGGER_CLASS);

    let id_prefix = use_hook(|| FORM_SELECT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst));

    // 受控选中序号；value 不在 options 时兜底 0（与浏览器默认选中第一项一致）。
    let selected = options.iter().position(|(v, _)| *v == value).unwrap_or(0);
    let selected_label = options.get(selected).map(|(_, l)| *l).unwrap_or_default();
    let len = options.len();

    let mut open = use_signal(|| false);
    let mut active = use_signal(|| selected);
    // flip_up 的 set 均在 wasm 门控语句内（宿主构建下只读），参照 FilterTabs 先例。
    #[allow(unused_mut)]
    let mut flip_up = use_signal(|| false);

    // onkeydown 闭包与下方选项渲染各需一份 options（键盘选中回查 / 渲染）。
    let options_for_keys = options.clone();

    // 触发器 id：外部未指定时用内部前缀生成，ARIA 关联统一走它。
    let trigger_id = id.unwrap_or_else(|| format!("form-select-{id_prefix}"));
    // 两个事件闭包各持一份（仅 wasm 用于 flip 测量按 id 查元素）。
    #[cfg(target_arch = "wasm32")]
    let trigger_id_click = trigger_id.clone();
    #[cfg(target_arch = "wasm32")]
    let trigger_id_keys = trigger_id.clone();

    // 打开或键盘导航时，把高亮项滚入面板可视区。
    use_effect(move || {
        if open() {
            #[cfg(target_arch = "wasm32")]
            {
                let idx = active();
                scroll_option_into_view(&format!("form-select-{id_prefix}-opt-{idx}"));
            }
        }
    });

    // 预计算每行展示态，rsx 循环内只做移动与闭包捕获。
    let active_idx = active();
    let rows: Vec<(usize, T, &'static str, &'static str, &'static str)> = options
        .iter()
        .enumerate()
        .map(|(i, (v, l))| {
            let highlight = if i == active_idx {
                "bg-[var(--color-paper-accent-soft)]"
            } else {
                ""
            };
            let text = if i == selected {
                "text-paper-accent"
            } else {
                "text-[var(--color-paper-primary)]"
            };
            (i, v.clone(), *l, highlight, text)
        })
        .collect();

    let chevron_rotate = if open() { "rotate-180" } else { "" };
    let placement_cls = if flip_up() {
        "bottom-full mb-1.5 origin-bottom"
    } else {
        "top-full mt-1.5 origin-top"
    };
    let active_descendant = open().then(|| {
        let idx = active();
        format!("form-select-{id_prefix}-opt-{idx}")
    });

    rsx! {
        div { class: "relative",
            button {
                id: "{trigger_id}",
                r#type: "button",
                class: "{trigger_cls}",
                aria_haspopup: "listbox",
                aria_expanded: "{open()}",
                aria_activedescendant: active_descendant,
                aria_label,
                onclick: move |_| {
                    // 打开态下触发器被透明遮罩盖住，点击落在遮罩上即关闭；
                    // 这里只需处理「未开 → 开」。
                    if !open() {
                        #[cfg(target_arch = "wasm32")]
                        flip_up.set(measure_flip(&trigger_id_click, len));
                        active.set(selected);
                        open.set(true);
                    }
                },
                onkeydown: move |e| {
                    let key = e.key();
                    let is_space = matches!(&key, Key::Character(s) if s == " ");
                    if !open() {
                        if key == Key::ArrowDown || key == Key::ArrowUp || key == Key::Enter
                            || is_space
                        {
                            e.prevent_default();
                            #[cfg(target_arch = "wasm32")]
                            flip_up.set(measure_flip(&trigger_id_keys, len));
                            active.set(selected);
                            open.set(true);
                        }
                        return;
                    }
                    if key == Key::ArrowDown {
                        e.prevent_default();
                        active.set(wrap_index(active(), 1, len));
                    } else if key == Key::ArrowUp {
                        e.prevent_default();
                        active.set(wrap_index(active(), -1, len));
                    } else if key == Key::Home { // 不拦截：关闭后焦点自然流转到下一个控件。
                        e.prevent_default();
                        active.set(0);
                    } else if key == Key::End {
                        e.prevent_default();
                        active.set(len.saturating_sub(1));
                    } else if key == Key::Enter || is_space {
                        e.prevent_default();
                        if let Some((v, _)) = options_for_keys.get(active()) {
                            onchange.call(v.clone());
                        }
                        open.set(false);
                    } else if key == Key::Escape {
                        e.prevent_default();
                        open.set(false);
                    } else if key == Key::Tab {
                        open.set(false);
                    }
                },
                "{selected_label}"
                // 下拉箭头（打开时翻转）
                svg {
                    class: "pointer-events-none absolute right-4 top-1/2 -translate-y-1/2 w-4 h-4 text-paper-secondary transition-transform duration-200 {chevron_rotate}",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M6 9l6 6 6-6",
                    }
                }
            }

            if open() {
                // 透明遮罩：拦截外部点击（点遮罩即关），z-40 < 面板 z-50。
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |_| open.set(false),
                }
                ul {
                    class: "{PANEL_CLASS} {placement_cls}",
                    role: "listbox",
                    aria_labelledby: "{trigger_id}",
                    for (i, opt_value, opt_label, highlight_cls, text_cls) in rows {
                        li {
                            id: "form-select-{id_prefix}-opt-{i}",
                            class: "flex items-center justify-between gap-2 px-3 py-2.5 rounded-xl cursor-pointer select-none transition-colors hover:bg-[var(--color-paper-accent-soft)] {text_cls} {highlight_cls}",
                            role: "option",
                            aria_selected: "{i == selected}",
                            // 阻止 mousedown 默认行为：点击选项不夺走触发器焦点。
                            onmousedown: move |e| e.prevent_default(),
                            onclick: move |_| {
                                onchange.call(opt_value.clone());
                                open.set(false);
                            },
                            onmouseenter: move |_| active.set(i),
                            span { class: "truncate", "{opt_label}" }
                            if i == selected {
                                svg {
                                    class: "w-4 h-4 flex-shrink-0",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M20 6L9 17l-5-5",
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 小时/分钟选项标签表（"00"–"23" / "00"–"59"）。
/// 静态表避免每次渲染堆分配；FormSelect 选项标签要求 `&'static str`。
const HOUR_LABELS: [&str; 24] = [
    "00", "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "13", "14", "15",
    "16", "17", "18", "19", "20", "21", "22", "23",
];
const MINUTE_LABELS: [&str; 60] = [
    "00", "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "13", "14", "15",
    "16", "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30", "31",
    "32", "33", "34", "35", "36", "37", "38", "39", "40", "41", "42", "43", "44", "45", "46", "47",
    "48", "49", "50", "51", "52", "53", "54", "55", "56", "57", "58", "59",
];

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

/// 时间选择器（24 小时制 "HH:MM"）。
///
/// 原生 `<input type="time">` 的弹出层由浏览器/OS 绘制，暗色主题下是白底
/// 系统菜单（与原生 `<select>` 同款问题），故用两个 [`FormSelect`]（时/分）
/// 组合重写，弹层配色、`select-enter` 动画、键盘导航、视口翻转与遮罩关闭
/// 逻辑全部继承：
/// - 外框容器镜像步进器控件（rounded-lg 边框 + `bg-paper-entry`），触发器无边框；
/// - 打开任一段下拉自动滚动到当前值，点击即回调组合后的 "HH:MM"；
/// - 键盘：Tab 进入时/分列，↑↓ 或 Enter 展开，Enter 选定，Esc 关闭。
///
/// Props：
/// - `id`：小时触发器 id，用于与 label 关联（缺省用内部计数器生成）
/// - `value`：当前值 "HH:MM"（受控；非法值回退显示 00:00，不 panic）
/// - `onchange`：选中变化回调，回传组合后的 "HH:MM"
#[component]
pub fn TimePicker(id: Option<String>, value: String, onchange: EventHandler<String>) -> Element {
    // 无边框紧凑触发器：外框由容器统一绘制（镜像「保留份数」步进器）。
    // 定义在函数体内：模块级私有常量若仅被 wasm 门控调用点引用，会在 server
    // 构建下触发 dead_code（同 FormSelect 的 TRIGGER_CLASS 先例）。
    const TIME_TRIGGER_CLASS: &str = "inline-flex w-auto cursor-pointer select-none text-sm tabular-nums pl-2.5 pr-8 py-2 rounded-md bg-transparent text-paper-primary focus:outline-none focus:ring-1 focus:ring-paper-accent/30 transition-colors duration-200";

    let (hour, minute) = parse_hhmm(&value);
    let hour_options: Vec<(u8, &'static str)> = HOUR_LABELS
        .iter()
        .enumerate()
        .map(|(i, l)| (i as u8, *l))
        .collect();
    let minute_options: Vec<(u8, &'static str)> = MINUTE_LABELS
        .iter()
        .enumerate()
        .map(|(i, l)| (i as u8, *l))
        .collect();

    rsx! {
        div { class: "inline-flex items-center gap-0.5 rounded-lg border border-paper-border bg-paper-entry",
            FormSelect {
                id,
                aria_label: "小时",
                value: hour,
                options: hour_options,
                trigger_class: TIME_TRIGGER_CLASS,
                onchange: move |h: u8| onchange.call(format!("{h:02}:{minute:02}")),
            }
            span { class: "text-sm text-paper-tertiary select-none", ":" }
            FormSelect {
                aria_label: "分钟",
                value: minute,
                options: minute_options,
                trigger_class: TIME_TRIGGER_CLASS,
                onchange: move |m: u8| onchange.call(format!("{hour:02}:{m:02}")),
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
    oninput: EventHandler<String>,
    #[props(default)] onkeydown: Option<EventHandler<KeyboardEvent>>,
    #[props(default)] onfocus: Option<EventHandler<FocusEvent>>,
    #[props(default)] onblur: Option<EventHandler<FocusEvent>>,
    #[props(default)] class: Option<&'static str>,
    #[props(default)] mono: bool,
    #[props(default)] inputmode: Option<&'static str>,
    #[props(default)] title: Option<&'static str>,
) -> Element {
    let base = class.unwrap_or(INPUT_CLASS);
    let mono_class = if mono { " font-mono" } else { "" };
    let disabled_class = if disabled {
        " opacity-60 cursor-not-allowed"
    } else {
        ""
    };
    rsx! {
        input {
            id: id.unwrap_or_default(),
            class: "{base}{mono_class}{disabled_class}",
            r#type: "{r#type}",
            placeholder: "{placeholder}",
            value: "{value}",
            disabled,
            inputmode: inputmode.unwrap_or_default(),
            title: title.unwrap_or_default(),
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

/// 表单标签组件。
///
/// Props：
/// - `label`：标签文本
/// - `html_for`：关联的 input id
#[component]
pub fn FormLabel(label: String, html_for: Option<String>) -> Element {
    rsx! {
        label {
            class: "block text-sm font-medium text-paper-secondary mb-1",
            r#for: html_for.unwrap_or_default(),
            "{label}"
        }
    }
}

/// 提示框组件，用于显示成功、错误等状态消息。
///
/// Props：
/// - `message`：提示文本
/// - `variant`：风格类型，支持 `"error"`、`"success"` 与其他默认类型
#[component]
pub fn AlertBox(message: String, variant: &'static str) -> Element {
    let (bg_class, text_class) = match variant {
        "error" => (
            "bg-red-100 dark:bg-red-900/30",
            "text-red-700 dark:text-red-300",
        ),
        "success" => (
            "bg-green-100 dark:bg-green-900/30",
            "text-green-700 dark:text-green-300",
        ),
        _ => ("bg-paper-code-bg", "text-paper-secondary"),
    };
    rsx! {
        div { class: "mb-4 p-3 {bg_class} {text_class} rounded-lg text-center", "{message}" }
    }
}

/// 开关（toggle switch）组件。
///
/// 自定义滑块开关，取代原生 checkbox 用于设置项的布尔切换。视觉与交互全站统一：
/// 轨道 44×24px，开启主题绿、关闭 paper-tertiary；圆点 20px 白色，开启右移 20px。
/// accessibility：`role="switch"` + `aria-checked`，键盘 focus-visible 描边。
///
/// Props：
/// - `checked`：当前开关状态
/// - `ontoggle`：点击切换回调（父组件在回调内翻转 signal 并触发副作用）
#[component]
pub fn ToggleSwitch(checked: bool, ontoggle: Callback<()>) -> Element {
    let track_class = if checked {
        "relative w-11 h-6 flex-shrink-0 rounded-full bg-paper-accent cursor-pointer transition-colors duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-paper-accent/40"
    } else {
        "relative w-11 h-6 flex-shrink-0 rounded-full bg-paper-tertiary cursor-pointer transition-colors duration-200 focus:outline-none focus-visible:ring-2 focus-visible:ring-paper-accent/40"
    };
    let thumb_class = if checked {
        "absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm dark:shadow-black/30 transition-transform duration-200 translate-x-5"
    } else {
        "absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white shadow-sm dark:shadow-black/30 transition-transform duration-200"
    };
    rsx! {
        button {
            role: "switch",
            aria_checked: "{checked}",
            class: "{track_class}",
            onclick: move |_| ontoggle.call(()),
            span { class: "{thumb_class}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_hhmm, should_flip, wrap_index};

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
}
