//! ⌘K 面板公共部分：打开句柄、全局快捷键、文本匹配，以及前台搜索面板与后台命令面板
//! 共用的状态、对话框外壳和行组件。两个面板只提供各自的数据、条目映射与选中动作。
//! 样式见 styles/components/palette.css。

use std::future::Future;

use leptos::prelude::*;

use crate::components::ui::{Icon, IconName};

/// 输入防抖与进度点出现的延迟（毫秒）。
const DEBOUNCE_MS: u32 = 150;
const PROGRESS_DELAY_MS: u32 = 300;
/// 关闭淡出时长（毫秒），与 `palette.css` 的 `palette-out` 时长一致。
const EXIT_MS: u32 = 120;

/// 打开 ⌘K 面板的句柄。前台布局挂搜索面板，后台布局挂命令面板（`admin_palette`），
/// 两边的触发按钮与快捷键共用这个句柄。
#[derive(Clone, Copy, PartialEq)]
pub struct SearchPaletteHandle {
    pub(crate) open: RwSignal<bool>,
}

impl SearchPaletteHandle {
    /// 打开面板。栏目/主题过渡进行中时先跳到终态：浏览器把过渡快照画在顶层之上，
    /// 面板会被新正文盖住；跳过只结束动画，导航与换色照常完成。
    pub fn open(self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(window) = web_sys::window() {
            crate::utils::js::invoke_optional_global(&window, "__skipViewTransition", &[]);
        }
        self.open.set(true);
    }
}

/// 在布局中提供面板句柄。
pub fn use_search_palette_provider() -> SearchPaletteHandle {
    let handle = SearchPaletteHandle {
        open: RwSignal::new(false),
    };
    provide_context(handle);
    handle
}

/// 读取面板句柄；图鉴等不在前台布局内的场景返回 `None`。
pub fn use_search_palette() -> Option<SearchPaletteHandle> {
    use_context::<SearchPaletteHandle>()
}

/// 忽略大小写的子串匹配，空查询视为匹配。
pub fn contains_ci(text: &str, query: &str) -> bool {
    let q = query.trim();
    q.is_empty() || text.to_lowercase().contains(&q.to_lowercase())
}

/// 按忽略大小写的命中切分文本，`true` 段需要高亮。按字符比较，中文与西文都适用。
pub fn highlight(text: &str, query: &str) -> Vec<(String, bool)> {
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
pub fn snippet(text: &str, query: &str, radius: usize) -> String {
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
pub fn viewport_center() -> (f64, f64) {
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
pub fn focus_by_id(element_id: &str) {
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
///
/// 须在组件函数体内调用；监听挂在 `window` 上，组件卸载时经 `on_cleanup` 移除。
/// 服务端（非 wasm32）无 DOM，什么都不做，两端调用方代码一致。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub fn use_palette_shortcut(handle: SearchPaletteHandle, enabled: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let listener = window_event_listener(
            leptos::ev::keydown,
            move |event: leptos::ev::KeyboardEvent| {
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
                let palette_open = handle.open.get_untracked();
                if editable && !palette_open {
                    return;
                }
                event.prevent_default();
                if !palette_open {
                    handle.open();
                }
            },
        );
        on_cleanup(move || listener.remove());
    }
}

/// 面板的公共状态：打开、输入、当前项、请求进度与焦点归还。由面板组件经
/// [`use_palette_state`] 创建，交给 [`PaletteShell`] 渲染。
#[derive(Clone, Copy, PartialEq)]
pub struct PaletteState {
    pub open: RwSignal<bool>,
    pub query: RwSignal<String>,
    pub active: RwSignal<usize>,
    loading: RwSignal<bool>,
    show_progress: RwSignal<bool>,
    composing: RwSignal<bool>,
    seq: RwSignal<u64>,
    /// 已关闭但仍在淡出：面板继续渲染，输入与结果保持原样，淡出结束后复位。
    closing: RwSignal<bool>,
    /// DOM id 前缀：输入框 `{prefix}-input`、列表 `{prefix}-list`、选项 `{prefix}-opt-{i}`。
    id_prefix: &'static str,
    /// 打开前的焦点，关闭后还回去。
    #[cfg(target_arch = "wasm32")]
    previous_focus: StoredValue<Option<web_sys::HtmlElement>, LocalStorage>,
}

/// 创建面板状态：打开时记下原焦点并聚焦输入框。全局快捷键由各面板的生产入口
/// 自己用 [`use_palette_shortcut`] 注册，这里不注册。
///
/// 须在已 [`use_search_palette_provider`] 的布局内调用。
pub fn use_palette_state(id_prefix: &'static str) -> PaletteState {
    let handle = expect_context::<SearchPaletteHandle>();
    let state = PaletteState {
        open: handle.open,
        query: RwSignal::new(String::new()),
        active: RwSignal::new(0_usize),
        loading: RwSignal::new(false),
        show_progress: RwSignal::new(false),
        composing: RwSignal::new(false),
        seq: RwSignal::new(0_u64),
        closing: RwSignal::new(false),
        id_prefix,
        #[cfg(target_arch = "wasm32")]
        previous_focus: StoredValue::new_local(None),
    };

    Effect::new(move |_| {
        if !state.open.get() {
            return;
        }
        state.closing.set(false);
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            state.previous_focus.set_value(
                web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.active_element())
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok()),
            );
        }
        leptos::task::spawn_local(async move {
            crate::utils::time::sleep_ms(0).await;
            state.focus_input();
        });
    });
    state
}

impl PaletteState {
    /// 聚焦输入框（仅 wasm）。
    pub fn focus_input(self) {
        #[cfg(target_arch = "wasm32")]
        focus_by_id(&format!("{}-input", self.id_prefix));
    }

    /// 面板是否在屏上：打开，或正在淡出（关闭后保留 `EXIT_MS`，输入与结果原样不动）。
    pub fn visible(self) -> bool {
        self.open.get() || self.closing.get()
    }

    /// 关闭面板：丢弃未完成的请求与防抖，立即把焦点还给打开前的元素，面板淡出
    /// `EXIT_MS` 后复位输入，并调用 `reset` 让调用方复位自己的状态。已关闭时什么都不做；
    /// 淡出期间又被打开则保留上一次输入，不复位。
    pub fn close(self, reset: impl FnOnce() + 'static) {
        if !self.open.get_untracked() {
            return;
        }
        self.seq.update(|seq| *seq += 1);
        self.loading.set(false);
        self.show_progress.set(false);
        self.open.set(false);
        self.closing.set(true);
        #[cfg(target_arch = "wasm32")]
        if let Some(Some(el)) = self.previous_focus.try_update_value(Option::take) {
            let _ = el.focus();
        }
        leptos::task::spawn_local(async move {
            crate::utils::time::sleep_ms(EXIT_MS).await;
            // 淡出期间布局可能已卸载（信号被释放），此时直接放弃复位。
            if self.open.try_get_untracked().unwrap_or(true) {
                return;
            }
            self.query.set(String::new());
            self.active.set(0);
            reset();
            self.closing.set(false);
        });
    }

    /// 防抖后请求远端结果。空查询直接清空；`fetch` 返回 `None`（失败或被限流）时
    /// 保留上一次结果，不闪空列表。
    pub fn schedule_fetch<T, F, Fut>(
        self,
        results: RwSignal<Vec<T>>,
        results_query: RwSignal<String>,
        fetch: F,
    ) where
        T: Send + Sync + 'static,
        F: FnOnce(String) -> Fut + 'static,
        Fut: Future<Output = Option<Vec<T>>> + 'static,
    {
        self.seq.update(|seq| *seq += 1);
        let id = self.seq.get_untracked();
        let q = self.query.get_untracked().trim().to_string();
        if q.is_empty() {
            results.set(Vec::new());
            results_query.set(String::new());
            self.loading.set(false);
            self.show_progress.set(false);
            return;
        }
        leptos::task::spawn_local(async move {
            crate::utils::time::sleep_ms(DEBOUNCE_MS).await;
            if self.seq.try_get_untracked() != Some(id)
                || self.composing.try_get_untracked().unwrap_or(true)
            {
                return;
            }
            self.loading.set(true);
            leptos::task::spawn_local(async move {
                crate::utils::time::sleep_ms(PROGRESS_DELAY_MS).await;
                if self.seq.try_get_untracked() == Some(id)
                    && self.loading.try_get_untracked().unwrap_or(false)
                {
                    self.show_progress.set(true);
                }
            });
            let res = fetch(q.clone()).await;
            if self.seq.try_get_untracked() != Some(id) {
                return;
            }
            if let Some(items) = res {
                results.set(items);
                results_query.set(q);
            }
            self.loading.set(false);
            self.show_progress.set(false);
        });
    }
}

/// 面板一行的展示内容。
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteEntry {
    pub icon: IconName,
    pub title: String,
    /// 标题下的摘要片段（只有前台文章有）。
    pub summary: Option<String>,
    /// 右侧元信息，空串不显示。
    pub meta: String,
    pub mono_meta: bool,
    /// 是否用 `<mark>` 标出命中（最近搜索、外观切换不标）。
    pub highlight_hits: bool,
}

/// 面板中的一组条目（组标签 + 若干行）。
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteGroup {
    pub label: &'static str,
    pub entries: Vec<PaletteEntry>,
}

/// 把文本按命中切成纯文本与 `<mark>` 片段。
fn highlighted(text: &str, query: &str) -> impl IntoView {
    highlight(text, query)
        .into_iter()
        .map(|(text, hit)| {
            if hit {
                view! { <mark>{text}</mark> }.into_any()
            } else {
                text.into_any()
            }
        })
        .collect_view()
}

/// 面板中的一行：左侧图标块，中间标题（可带摘要片段），右侧元信息或 ↵ 键帽。
#[component]
fn PaletteRow(
    id: String,
    entry: PaletteEntry,
    query: String,
    #[prop(into)] selected: Signal<bool>,
    #[prop(into)] on_hover: Callback<()>,
    #[prop(into)] on_pick: Callback<bool>,
) -> impl IntoView {
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
    let title = highlighted(&entry.title, &highlight_query);
    let summary = entry.summary.map(|summary| {
        view! { <span class="palette-option-summary">{highlighted(&summary, &highlight_query)}</span> }
    });
    let meta = entry.meta;
    view! {
        <div
            id=id
            class="palette-option"
            role="option"
            aria-selected=move || selected.get().to_string()
            // 只有鼠标真的移动才改变当前项，列表滚动时当前项不会乱跳。
            on:mousemove=move |_| {
                if !selected.get_untracked() {
                    on_hover.run(());
                }
            }
            on:mousedown=move |e| e.prevent_default()
            on:click=move |e| on_pick.run(e.meta_key() || e.ctrl_key())
        >
            <span class="palette-option-tile">
                <Icon name=entry.icon size=15/>
            </span>
            <span class="palette-option-text">
                <span class="palette-option-title">{title}</span>
                {summary}
            </span>
            {move || {
                if selected.get() {
                    view! { <kbd class="palette-enter">"↵"</kbd> }.into_any()
                } else if !meta.is_empty() {
                    view! { <span class=meta_class>{meta.clone()}</span> }.into_any()
                } else {
                    ().into_any()
                }
            }}
        </div>
    }
}

/// 面板对话框外壳：scrim、输入行、列表、空结果与页脚。只在面板打开时渲染。
///
/// 输入、键位与列表渲染都在这里；数据、条目选中动作和各插槽内容由面板提供。
///
/// Props：
/// - `state`：[`use_palette_state`] 创建的公共状态
/// - `label`：对话框 aria-label，同时作为 `trap_tab` 的名称（"搜索" / "命令面板"）
/// - `input_label` / `placeholder` / `list_label`：输入框与列表的无障碍名称、占位文案
/// - `enter_hint`：页脚 ↵ 的说明（"打开" / "执行"）
/// - `empty_hint`：空结果提示文案
/// - `groups`：当前要显示的分组（扁平序号按组顺序累计）
/// - `waiting`：远端结果对应的查询与当前输入不同（与 loading 同时为真时不显示空结果）
/// - `clearable`：有输入时在输入行显示清除按钮
/// - `toolbar`：输入行与列表之间的插槽（前台范围标签）
/// - `empty_action`：空结果提示下方的插槽（前台「在全文中搜索」）
/// - `footer_action`：页脚快捷键说明右侧的插槽（前台「全文搜索」）
/// - `on_query_change`：输入变化（非输入法组合期间）或清空后重新检索
/// - `on_pick`：选中条目，参数 `(扁平序号, 是否按住 ⌘/Ctrl)`
/// - `on_close`：点击遮罩、Esc 按钮或空输入时按 Escape
#[component]
pub(crate) fn PaletteShell(
    state: PaletteState,
    label: &'static str,
    input_label: &'static str,
    placeholder: &'static str,
    list_label: &'static str,
    enter_hint: &'static str,
    empty_hint: &'static str,
    #[prop(into)] groups: Signal<Vec<PaletteGroup>>,
    #[prop(into)] waiting: Signal<bool>,
    #[prop(optional)] clearable: bool,
    #[prop(optional, into)] toolbar: ViewFn,
    #[prop(optional, into)] empty_action: ViewFn,
    #[prop(optional, into)] footer_action: ViewFn,
    #[prop(into)] on_query_change: Callback<()>,
    #[prop(into)] on_pick: Callback<(usize, bool)>,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let query = state.query;
    let active = state.active;
    let composing = state.composing;
    let prefix = state.id_prefix;
    let closing = move || !state.open.get();

    let has_query = move || !query.read().trim().is_empty();
    let total =
        Memo::new(move |_| groups.with(|g| g.iter().map(|g| g.entries.len()).sum::<usize>()));
    let active_idx = move || {
        let total = total.get();
        if total == 0 {
            0
        } else {
            active.get().min(total - 1)
        }
    };
    let no_results =
        move || has_query() && total.get() == 0 && !(waiting.get() && state.loading.get());

    // 清空输入并重新检索（Escape 与清除按钮共用）。
    let reset_query = move || {
        query.set(String::new());
        active.set(0);
        on_query_change.run(());
    };

    // 面板挂载（含首次打开时分包刚加载完）后聚焦输入框；淡出中的重建不抢焦点。
    // NodeRef 在元素创建时就被填充，此时元素可能还没插入文档（异步分包渲染），
    // 所以等下一帧再聚焦。
    let input_ref = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        let input = input_ref.get();
        #[cfg(target_arch = "wasm32")]
        if let Some(input) = input {
            request_animation_frame(move || {
                if state.open.try_get_untracked().unwrap_or(false) {
                    let _ = input.focus();
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = input;
    });

    view! {
        <div
            class=move || if closing() { "palette-scrim is-closing" } else { "palette-scrim" }
            on:click=move |_| on_close.run(())
        ></div>
        <div
            class=move || if closing() { "palette is-closing" } else { "palette" }
            role="dialog"
            aria-modal="true"
            aria-label=label
            // 复用弹窗的焦点锁定（Tab 只在面板内循环）。
            data-ygg-modal-panel="true"
            data-ygg-modal-open=move || if closing() { "false" } else { "true" }
            inert=move || closing().then_some("true")
            on:keydown=move |e| {
                if e.key() == "Tab" {
                    #[cfg(target_arch = "wasm32")]
                    crate::components::ui::trap_tab(&e, label);
                }
            }
        >
            <div class="palette-input-row">
                <Icon name=IconName::Search size=18 class="palette-input-icon"/>
                <input
                    id=format!("{prefix}-input")
                    node_ref=input_ref
                    class="palette-input"
                    type="text"
                    role="combobox"
                    aria-label=input_label
                    aria-expanded="true"
                    aria-controls=format!("{prefix}-list")
                    aria-activedescendant=move || {
                        (total.get() > 0).then(|| format!("{prefix}-opt-{}", active_idx()))
                    }
                    aria-autocomplete="list"
                    autocomplete="off"
                    spellcheck="false"
                    enterkeyhint="go"
                    maxlength="100"
                    placeholder=placeholder
                    prop:value=move || query.get()
                    on:input=move |e| {
                        query.set(event_target_value(&e));
                        active.set(0);
                        if !composing.get_untracked() {
                            on_query_change.run(());
                        }
                    }
                    on:compositionstart=move |_| composing.set(true)
                    on:compositionend=move |_| {
                        composing.set(false);
                        on_query_change.run(());
                    }
                    on:keydown=move |e| {
                        // 输入法组合期间（含 keyCode 229 的 Process 键）不处理任何按键。
                        if crate::components::ui::ime_composing(composing.get_untracked(), &e) {
                            return;
                        }
                        match e.key().as_str() {
                            key @ ("ArrowDown" | "ArrowUp") => {
                                e.prevent_default();
                                let total = total.get_untracked();
                                if total > 0 {
                                    let delta: i32 = if key == "ArrowDown" { 1 } else { -1 };
                                    active.set(
                                        (active_idx() as i32 + delta).rem_euclid(total as i32) as usize,
                                    );
                                }
                            }
                            "Enter" => {
                                e.prevent_default();
                                if total.get_untracked() > 0 {
                                    on_pick.run((active_idx(), e.meta_key() || e.ctrl_key()));
                                }
                            }
                            "Escape" => {
                                e.prevent_default();
                                if query.read_untracked().is_empty() {
                                    on_close.run(());
                                } else {
                                    reset_query();
                                }
                            }
                            _ => {}
                        }
                    }
                />
                {move || {
                    state
                        .show_progress
                        .get()
                        .then(|| {
                            view! {
                                <span class="palette-progress" role="status" aria-label="正在搜索">
                                    <span></span>
                                    <span></span>
                                    <span></span>
                                </span>
                            }
                        })
                }}
                {move || {
                    (clearable && has_query())
                        .then(|| {
                            view! {
                                <button
                                    class="btn btn-xs btn-icon btn-ghost"
                                    type="button"
                                    aria-label="清除输入"
                                    on:click=move |_| {
                                        reset_query();
                                        state.focus_input();
                                    }
                                >
                                    <Icon name=IconName::X size=14/>
                                </button>
                            }
                        })
                }}
                <button class="palette-esc" type="button" on:click=move |_| on_close.run(())>
                    <span class="palette-esc-key">"Esc"</span>
                    <span class="palette-esc-cancel">"取消"</span>
                </button>
            </div>
            {toolbar.run()}
            <div
                id=format!("{prefix}-list")
                class="palette-list"
                role="listbox"
                aria-label=list_label
            >
                {move || {
                    let q = query.get();
                    let mut index = 0_usize;
                    groups
                        .get()
                        .into_iter()
                        .map(|group| {
                            let rows = group
                                .entries
                                .into_iter()
                                .map(|entry| {
                                    let i = index;
                                    index += 1;
                                    view! {
                                        <PaletteRow
                                            id=format!("{prefix}-opt-{i}")
                                            entry=entry
                                            query=q.clone()
                                            selected=Signal::derive(move || i == active_idx())
                                            on_hover=move |_| active.set(i)
                                            on_pick=move |new_tab: bool| on_pick.run((i, new_tab))
                                        />
                                    }
                                })
                                .collect_view();
                            view! {
                                <div role="group" aria-label=group.label>
                                    <p class="palette-group-label">{group.label}</p>
                                    {rows}
                                </div>
                            }
                        })
                        .collect_view()
                }}
                {move || {
                    no_results()
                        .then(|| {
                            view! {
                                <div class="palette-empty">
                                    <span class="empty-state-icon">
                                        <Icon name=IconName::SearchX size=18/>
                                    </span>
                                    <p class="palette-empty-title">
                                        {move || format!("没有找到「{}」", query.read().trim())}
                                    </p>
                                    <p class="palette-empty-hint">{empty_hint}</p>
                                    {empty_action.run()}
                                </div>
                            }
                        })
                }}
            </div>
            <div class="palette-footer">
                <div class="palette-hints">
                    <span>
                        <kbd>"↑"</kbd>
                        <kbd>"↓"</kbd>
                        "选择"
                    </span>
                    <span>
                        <kbd>"↵"</kbd>
                        {enter_hint}
                    </span>
                    <span>
                        <kbd>"Esc"</kbd>
                        "关闭"
                    </span>
                </div>
                {footer_action.run()}
            </div>
        </div>
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

    fn entry(title: &str) -> PaletteEntry {
        PaletteEntry {
            icon: IconName::FileText,
            title: title.to_string(),
            summary: None,
            meta: "/x".to_string(),
            mono_meta: true,
            highlight_hits: true,
        }
    }

    #[test]
    fn shell_renders_dialog_combobox_and_grouped_options() {
        let html = Owner::new().with(|| {
            let handle = use_search_palette_provider();
            handle.open.set(true);
            let state = use_palette_state("palette");
            state.query.set("ru".to_string());
            view! {
                <PaletteShell
                    state=state
                    label="搜索"
                    input_label="搜索文章、标签或页面"
                    placeholder="搜索…"
                    list_label="搜索结果"
                    enter_hint="打开"
                    empty_hint="无命中"
                    groups=vec![PaletteGroup { label: "文章", entries: vec![entry("Rust 笔记"), entry("Other")] }]
                    waiting=false
                    on_query_change=|_| {}
                    on_pick=|_| {}
                    on_close=|_| {}
                />
            }
            .to_html()
        });
        assert!(html.contains("role=\"dialog\""), "{html}");
        assert!(html.contains("data-ygg-modal-open=\"true\""), "{html}");
        assert!(html.contains("id=\"palette-input\""), "{html}");
        assert!(html.contains("role=\"combobox\""), "{html}");
        assert!(
            html.contains("aria-activedescendant=\"palette-opt-0\""),
            "{html}"
        );
        assert!(html.contains("id=\"palette-opt-0\""), "{html}");
        assert!(html.contains("id=\"palette-opt-1\""), "{html}");
        // 首项选中显示 ↵ 键帽，命中词用 <mark> 标出。
        assert!(
            html.contains("<kbd class=\"palette-enter\">↵</kbd>"),
            "{html}"
        );
        assert!(html.contains("<mark>Ru</mark>"), "{html}");
    }
}
