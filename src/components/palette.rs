//! ⌘K 面板公共部分：打开句柄、全局快捷键、文本匹配，以及前台搜索面板与后台命令面板
//! 共用的状态、对话框外壳和行组件。两个面板只提供各自的数据、条目映射与选中动作。
//! 样式见 styles/components/palette.css。

use dioxus::prelude::*;

use crate::components::ui::{Icon, IconName};

/// 输入防抖与进度点出现的延迟（毫秒）。
const DEBOUNCE_MS: u32 = 150;
const PROGRESS_DELAY_MS: u32 = 300;

/// 打开 ⌘K 面板的句柄。前台布局挂搜索面板，后台布局挂命令面板（`admin_palette`），
/// 两边的触发按钮与快捷键共用这个句柄。
#[derive(Clone, Copy, PartialEq)]
pub struct SearchPaletteHandle {
    pub(crate) open: Signal<bool>,
}

impl SearchPaletteHandle {
    /// 打开面板。栏目/主题过渡进行中时先跳到终态：浏览器把过渡快照画在顶层之上，
    /// 面板会被新正文盖住；跳过只结束动画，导航与换色照常完成。
    pub fn open(mut self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            crate::utils::js::invoke_optional_global(&window, "__skipViewTransition", &[]);
        }
        self.open.set(true);
    }
}

/// 在布局中提供面板句柄。
pub fn use_search_palette_provider() -> SearchPaletteHandle {
    let open = use_signal(|| false);
    use_context_provider(|| SearchPaletteHandle { open })
}

/// 读取面板句柄；图鉴等不在前台布局内的场景返回 `None`。
pub fn use_search_palette() -> Option<SearchPaletteHandle> {
    try_use_context::<SearchPaletteHandle>()
}

/// 忽略大小写的子串匹配，空查询视为匹配。
pub(crate) fn contains_ci(text: &str, query: &str) -> bool {
    let q = query.trim();
    q.is_empty() || text.to_lowercase().contains(&q.to_lowercase())
}

/// 按忽略大小写的命中切分文本，`true` 段需要高亮。按字符比较，中文与西文都适用。
pub(crate) fn highlight(text: &str, query: &str) -> Vec<(String, bool)> {
    let q: Vec<char> = query.trim().to_lowercase().chars().collect();
    let chars: Vec<char> = text.chars().collect();
    if q.is_empty() {
        return vec![(text.to_string(), false)];
    }
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < chars.len() {
        if i + q.len() <= chars.len() && lower[i..i + q.len()] == q[..] {
            if !plain.is_empty() {
                out.push((std::mem::take(&mut plain), false));
            }
            out.push((chars[i..i + q.len()].iter().collect(), true));
            i += q.len();
        } else {
            plain.push(chars[i]);
            i += 1;
        }
    }
    if !plain.is_empty() {
        out.push((plain, false));
    }
    out
}

/// 摘要片段：截取第一处命中前后 `radius` 个字，两端被截断时加省略号；
/// 没有命中时取开头。
pub(crate) fn snippet(text: &str, query: &str, radius: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let q: Vec<char> = query.trim().to_lowercase().chars().collect();
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let hit = (!q.is_empty())
        .then(|| {
            (0..chars.len()).find(|&i| i + q.len() <= chars.len() && lower[i..i + q.len()] == q[..])
        })
        .flatten()
        .unwrap_or(0);
    let start = hit.saturating_sub(radius);
    let end = (hit + q.len() + radius).min(chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(&chars[start..end]);
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// 视口中心：从面板切换外观时，圆形扩散以此为圆心。
pub(crate) fn viewport_center() -> (f64, f64) {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .map(|w| {
                let width = w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
                let height = w
                    .inner_height()
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                (width / 2.0, height / 2.0)
            })
            .unwrap_or((0.0, 0.0))
    }
    #[cfg(not(target_arch = "wasm32"))]
    (0.0, 0.0)
}

/// 按 id 聚焦元素（仅 wasm）。
#[cfg(target_arch = "wasm32")]
pub(crate) fn focus_by_id(element_id: &str) {
    use wasm_bindgen::JsCast;
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(element_id))
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.focus();
    }
}

/// 全局 ⌘K / Ctrl+K：焦点在可编辑区域时不拦截。
#[cfg(target_arch = "wasm32")]
pub(crate) fn use_palette_shortcut(handle: SearchPaletteHandle, enabled: bool) {
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    type Listener =
        Rc<RefCell<Option<wasm_bindgen::prelude::Closure<dyn FnMut(web_sys::KeyboardEvent)>>>>;
    let listener: Listener = use_hook(|| Rc::new(RefCell::new(None)));
    let for_drop = listener.clone();
    use_hook(move || {
        let closure =
            wasm_bindgen::prelude::Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
                if !enabled
                    || !(event.meta_key() || event.ctrl_key())
                    || event.key().to_lowercase() != "k"
                {
                    return;
                }
                let editable = event
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
                    .is_some_and(|el| {
                        el.is_content_editable()
                            || matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT")
                    });
                let palette_open = *handle.open.peek();
                if editable && !palette_open {
                    return;
                }
                event.prevent_default();
                if !palette_open {
                    handle.open();
                }
            })
                as Box<dyn FnMut(web_sys::KeyboardEvent)>);
        if let Some(window) = web_sys::window() {
            let _ = window
                .add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        }
        *listener.borrow_mut() = Some(closure);
    });
    use_drop(move || {
        if let (Some(closure), Some(window)) = (for_drop.borrow_mut().take(), web_sys::window()) {
            let _ = window
                .remove_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        }
    });
}

/// 面板的公共状态：打开、输入、当前项、请求进度与焦点归还。由面板组件经
/// [`use_palette_state`] 创建，交给 [`PaletteShell`] 渲染。
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct PaletteState {
    pub(crate) open: Signal<bool>,
    pub(crate) query: Signal<String>,
    pub(crate) active: Signal<usize>,
    loading: Signal<bool>,
    show_progress: Signal<bool>,
    composing: Signal<bool>,
    seq: Signal<u64>,
    /// DOM id 前缀：输入框 `{prefix}-input`、列表 `{prefix}-list`、选项 `{prefix}-opt-{i}`。
    id_prefix: &'static str,
    /// 打开前的焦点，关闭后还回去。
    #[cfg(target_arch = "wasm32")]
    previous_focus: Signal<Option<web_sys::HtmlElement>>,
}

/// 创建面板状态：打开时记下原焦点并聚焦输入框。全局快捷键由各面板的生产入口
/// 自己用 [`use_palette_shortcut`] 注册，这里不注册。
pub(crate) fn use_palette_state(id_prefix: &'static str) -> PaletteState {
    let handle = use_context::<SearchPaletteHandle>();
    let state = PaletteState {
        open: handle.open,
        query: use_signal(String::new),
        active: use_signal(|| 0_usize),
        loading: use_signal(|| false),
        show_progress: use_signal(|| false),
        composing: use_signal(|| false),
        seq: use_signal(|| 0_u64),
        id_prefix,
        #[cfg(target_arch = "wasm32")]
        previous_focus: use_signal(|| None::<web_sys::HtmlElement>),
    };

    use_effect(move || {
        if !(state.open)() {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            let mut previous_focus = state.previous_focus;
            previous_focus.set(
                web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.active_element())
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok()),
            );
        }
        spawn(async move {
            crate::utils::time::sleep_ms(0).await;
            state.focus_input();
        });
    });
    state
}

impl PaletteState {
    /// 聚焦输入框（仅 wasm）。
    pub(crate) fn focus_input(self) {
        #[cfg(target_arch = "wasm32")]
        focus_by_id(&format!("{}-input", self.id_prefix));
    }

    /// 关闭面板：丢弃未完成的请求与防抖，复位输入并把焦点还给打开前的元素。
    /// 已关闭时返回 `false`；返回 `true` 时面板继续复位自己的状态。
    pub(crate) fn close(mut self) -> bool {
        if !*self.open.peek() {
            return false;
        }
        self.seq += 1;
        self.loading.set(false);
        self.show_progress.set(false);
        self.query.set(String::new());
        self.active.set(0);
        self.open.set(false);
        #[cfg(target_arch = "wasm32")]
        if let Some(el) = self.previous_focus.take() {
            let _ = el.focus();
        }
        true
    }

    /// 防抖后请求远端结果。空查询直接清空；`fetch` 返回 `None`（失败或被限流）时
    /// 保留上一次结果，不闪空列表。
    pub(crate) fn schedule_fetch<T, F, Fut>(
        self,
        results: Signal<Vec<T>>,
        results_query: Signal<String>,
        fetch: F,
    ) where
        T: 'static,
        F: FnOnce(String) -> Fut + 'static,
        Fut: std::future::Future<Output = Option<Vec<T>>> + 'static,
    {
        let mut state = self;
        let mut results = results;
        let mut results_query = results_query;
        state.seq += 1;
        let id = (state.seq)();
        let q = state.query.peek().trim().to_string();
        if q.is_empty() {
            results.set(Vec::new());
            results_query.set(String::new());
            state.loading.set(false);
            state.show_progress.set(false);
            return;
        }
        spawn(async move {
            crate::utils::time::sleep_ms(DEBOUNCE_MS).await;
            if (state.seq)() != id || (state.composing)() {
                return;
            }
            state.loading.set(true);
            spawn(async move {
                crate::utils::time::sleep_ms(PROGRESS_DELAY_MS).await;
                if (state.seq)() == id && (state.loading)() {
                    state.show_progress.set(true);
                }
            });
            let res = fetch(q.clone()).await;
            if (state.seq)() != id {
                return;
            }
            if let Some(items) = res {
                results.set(items);
                results_query.set(q);
            }
            state.loading.set(false);
            state.show_progress.set(false);
        });
    }
}

/// 面板一行的展示内容。
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaletteEntry {
    pub(crate) icon: IconName,
    pub(crate) title: String,
    /// 标题下的摘要片段（只有前台文章有）。
    pub(crate) summary: Option<String>,
    /// 右侧元信息，空串不显示。
    pub(crate) meta: String,
    pub(crate) mono_meta: bool,
    /// 是否用 `<mark>` 标出命中（最近搜索、外观切换不标）。
    pub(crate) highlight_hits: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaletteGroup {
    pub(crate) label: &'static str,
    pub(crate) entries: Vec<PaletteEntry>,
}

/// 面板中的一行：左侧图标块，中间标题（可带摘要片段），右侧元信息或 ↵ 键帽。
#[component]
fn PaletteRow(
    id: String,
    entry: PaletteEntry,
    query: String,
    selected: bool,
    on_hover: EventHandler<()>,
    on_pick: EventHandler<bool>,
) -> Element {
    let highlight_query = if entry.highlight_hits {
        query
    } else {
        String::new()
    };
    let meta_class = if entry.mono_meta {
        "palette-option-meta font-mono"
    } else {
        "palette-option-meta"
    };
    rsx! {
        div {
            id: "{id}",
            class: "palette-option",
            role: "option",
            aria_selected: "{selected}",
            // 只有鼠标真的移动才改变当前项，列表滚动时当前项不会乱跳。
            onmousemove: move |_| {
                if !selected {
                    on_hover.call(());
                }
            },
            onmousedown: move |e| e.prevent_default(),
            onclick: move |e| {
                let modifiers = e.modifiers();
                on_pick.call(modifiers.meta() || modifiers.ctrl());
            },
            span { class: "palette-option-tile", Icon { name: entry.icon, size: 15 } }
            span { class: "palette-option-text",
                span { class: "palette-option-title",
                    for (text, hit) in highlight(&entry.title, &highlight_query) {
                        if hit {
                            mark { "{text}" }
                        } else {
                            "{text}"
                        }
                    }
                }
                if let Some(summary) = entry.summary {
                    span { class: "palette-option-summary",
                        for (text, hit) in highlight(&summary, &highlight_query) {
                            if hit {
                                mark { "{text}" }
                            } else {
                                "{text}"
                            }
                        }
                    }
                }
            }
            if selected {
                kbd { class: "palette-enter", "↵" }
            } else if !entry.meta.is_empty() {
                span { class: meta_class, "{entry.meta}" }
            }
        }
    }
}

/// 面板对话框外壳：scrim、输入行、列表、空结果与页脚。只在面板打开时渲染。
///
/// 输入、键位与列表渲染都在这里；数据、条目选中动作和各插槽内容由面板提供。
#[component]
pub(crate) fn PaletteShell(
    state: PaletteState,
    /// 对话框 aria-label，同时作为 trap_tab 的名称（"搜索" / "命令面板"）
    label: &'static str,
    input_label: &'static str,
    placeholder: &'static str,
    list_label: &'static str,
    /// 页脚 ↵ 的说明（"打开" / "执行"）
    enter_hint: &'static str,
    empty_hint: &'static str,
    groups: Vec<PaletteGroup>,
    /// 远端结果对应的查询与当前输入不同（与 loading 同时为真时不显示空结果）
    waiting: bool,
    #[props(default)] clearable: bool,
    /// 输入行与列表之间的插槽（前台范围标签）
    toolbar: Element,
    /// 空结果提示下方的插槽（前台「在全文中搜索」）
    empty_action: Element,
    /// 页脚快捷键说明右侧的插槽（前台「全文搜索」）
    footer_action: Element,
    on_query_change: EventHandler<()>,
    /// (扁平序号, 是否按住 ⌘/Ctrl)
    on_pick: EventHandler<(usize, bool)>,
    on_close: EventHandler<()>,
) -> Element {
    let mut query = state.query;
    let mut active = state.active;
    let mut composing = state.composing;
    let prefix = state.id_prefix;

    let q = query();
    let has_query = !q.trim().is_empty();
    let total: usize = groups.iter().map(|g| g.entries.len()).sum();
    let active_idx = if total == 0 {
        0
    } else {
        active().min(total - 1)
    };
    let no_results = has_query && total == 0 && !(waiting && (state.loading)());

    // 清空输入并重新检索（Escape 与清除按钮共用）。
    let mut reset_query = move || {
        query.set(String::new());
        active.set(0);
        on_query_change.call(());
    };

    rsx! {
        div { class: "palette-scrim", onclick: move |_| on_close.call(()) }
        div {
            class: "palette",
            role: "dialog",
            aria_modal: "true",
            aria_label: label,
            // 复用弹窗的焦点锁定（Tab 只在面板内循环）。
            "data-ygg-modal-panel": "true",
            "data-ygg-modal-open": "true",
            onkeydown: move |e| {
                if e.key() == Key::Tab {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use dioxus::web::WebEventExt;
                        crate::components::ui::trap_tab(&e.as_web_event(), label);
                    }
                }
            },
            div { class: "palette-input-row",
                Icon { name: IconName::Search, size: 18, class: "palette-input-icon" }
                input {
                    id: "{prefix}-input",
                    class: "palette-input",
                    r#type: "text",
                    role: "combobox",
                    aria_label: input_label,
                    aria_expanded: "true",
                    aria_controls: "{prefix}-list",
                    aria_activedescendant: (total > 0).then(|| format!("{prefix}-opt-{active_idx}")),
                    aria_autocomplete: "list",
                    autocomplete: "off",
                    spellcheck: "false",
                    enterkeyhint: "go",
                    maxlength: "100",
                    placeholder,
                    value: "{q}",
                    oninput: move |e| {
                        query.set(e.value());
                        active.set(0);
                        if !composing() {
                            on_query_change.call(());
                        }
                    },
                    oncompositionstart: move |_| composing.set(true),
                    oncompositionend: move |_| {
                        composing.set(false);
                        on_query_change.call(());
                    },
                    onkeydown: move |e| {
                        // 输入法组合期间（含 keyCode 229 的 Process 键）不处理任何按键。
                        if composing() || e.data().is_composing() || e.key() == Key::Process {
                            return;
                        }
                        match e.key() {
                            Key::ArrowDown | Key::ArrowUp => {
                                e.prevent_default();
                                if total > 0 {
                                    let delta: i32 = if e.key() == Key::ArrowDown { 1 } else { -1 };
                                    active.set((active_idx as i32 + delta).rem_euclid(total as i32) as usize);
                                }
                            }
                            Key::Enter => {
                                e.prevent_default();
                                if total > 0 {
                                    let modifiers = e.modifiers();
                                    on_pick.call((active_idx, modifiers.meta() || modifiers.ctrl()));
                                }
                            }
                            Key::Escape => {
                                e.prevent_default();
                                if query.peek().is_empty() {
                                    on_close.call(());
                                } else {
                                    reset_query();
                                }
                            }
                            _ => {}
                        }
                    },
                }
                if (state.show_progress)() {
                    span { class: "palette-progress", role: "status", aria_label: "正在搜索",
                        span {}
                        span {}
                        span {}
                    }
                }
                if clearable && has_query {
                    button {
                        class: "btn btn-xs btn-icon btn-ghost",
                        r#type: "button",
                        aria_label: "清除输入",
                        onclick: move |_| {
                            reset_query();
                            state.focus_input();
                        },
                        Icon { name: IconName::X, size: 14 }
                    }
                }
                button {
                    class: "palette-esc",
                    r#type: "button",
                    onclick: move |_| on_close.call(()),
                    "Esc"
                }
            }
            {toolbar}
            div {
                id: "{prefix}-list",
                class: "palette-list",
                role: "listbox",
                aria_label: list_label,
                {
                    let mut index = 0_usize;
                    rsx! {
                        for group in groups {
                            div { key: "{group.label}", role: "group", aria_label: group.label,
                                p { class: "palette-group-label", "{group.label}" }
                                for entry in group.entries {
                                    {
                                        let i = index;
                                        index += 1;
                                        rsx! {
                                            PaletteRow {
                                                key: "{i}",
                                                id: format!("{prefix}-opt-{i}"),
                                                entry,
                                                query: q.clone(),
                                                selected: i == active_idx,
                                                on_hover: move |_| active.set(i),
                                                on_pick: move |new_tab: bool| on_pick.call((i, new_tab)),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if no_results {
                    div { class: "palette-empty",
                        span { class: "empty-state-icon", Icon { name: IconName::SearchX, size: 18 } }
                        p { class: "palette-empty-title", "没有找到「{q.trim()}」" }
                        p { class: "palette-empty-hint", "{empty_hint}" }
                        {empty_action}
                    }
                }
            }
            div { class: "palette-footer",
                div { class: "palette-hints",
                    span { kbd { "↑" } kbd { "↓" } "选择" }
                    span { kbd { "↵" } "{enter_hint}" }
                    span { kbd { "Esc" } "关闭" }
                }
                {footer_action}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_marks_every_case_insensitive_hit() {
        assert_eq!(
            highlight("Rust 与 rust", "RUST"),
            vec![
                ("Rust".into(), true),
                (" 与 ".into(), false),
                ("rust".into(), true)
            ]
        );
        assert_eq!(
            highlight("源码剖析", "剖析"),
            vec![("源码".into(), false), ("剖析".into(), true)]
        );
        assert_eq!(highlight("无命中", ""), vec![("无命中".into(), false)]);
    }

    #[test]
    fn snippet_centers_on_the_first_hit_with_ellipses() {
        let text = "一二三四五六七八九十命中词十一十二十三十四十五";
        assert_eq!(snippet(text, "命中", 3), "…八九十命中词十一…");
        assert_eq!(snippet("命中在开头", "命中", 2), "命中在开…");
        // 没有命中时取开头。
        assert_eq!(snippet("没有命中的一段话", "zz", 2), "没有命中…");
    }
}
