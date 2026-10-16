//! 搜索面板（⌘K）：前台全站的即时跳转。
//!
//! 页头「搜索 ⌘K」与全局快捷键打开同一个面板。文章来自 `suggest_posts`（标题、
//! 摘要、标签，最多 8 条），标签与页面在浏览器内过滤。空查询显示最近搜索与常用
//! 跳转；有查询时按「文章 · 标签 · 跳转」分组，命中词用 `<mark>` 标出。
//! 样式见 styles/components/palette.css；行为细节见 docs/v1.0-重设计方案.md「搜索面板」。

use dioxus::prelude::*;

use crate::api::posts::{list_tags, suggest_posts, PostSuggestion};
use crate::components::ui::{Icon, IconName};
use crate::router::Route;
use crate::theme::{choose_theme, use_resolved_theme, use_theme, ResolvedTheme, Theme};

/// 打开搜索面板的句柄，由 `FrontendLayout` 提供。
#[derive(Clone, Copy, PartialEq)]
pub struct SearchPaletteHandle {
    open: Signal<bool>,
}

impl SearchPaletteHandle {
    /// 打开面板。
    pub fn open(mut self) {
        self.open.set(true);
    }
}

/// 在前台布局中提供面板句柄。
pub fn use_search_palette_provider() -> SearchPaletteHandle {
    let open = use_signal(|| false);
    use_context_provider(|| SearchPaletteHandle { open })
}

/// 读取面板句柄；图鉴等不在前台布局内的场景返回 `None`。
pub fn use_search_palette() -> Option<SearchPaletteHandle> {
    try_use_context::<SearchPaletteHandle>()
}

/// 搜索范围。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    All,
    Posts,
    Tags,
    Pages,
}

impl Scope {
    const ALL: [Scope; 4] = [Scope::All, Scope::Posts, Scope::Tags, Scope::Pages];

    fn label(self) -> &'static str {
        match self {
            Scope::All => "全部",
            Scope::Posts => "文章",
            Scope::Tags => "标签",
            Scope::Pages => "页面",
        }
    }
}

/// 面板中的一项。
#[derive(Clone, Debug, PartialEq)]
enum Item {
    Post(PostSuggestion),
    Tag {
        name: String,
        count: i64,
    },
    Page {
        label: &'static str,
        path: &'static str,
    },
    Recent(String),
    Theme {
        to: Theme,
        label: &'static str,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct Group {
    label: &'static str,
    items: Vec<Item>,
}

/// 可跳转的页面：标签与路径。
const PAGES: [(&str, &str); 8] = [
    ("首页", "/"),
    ("笔记", "/notes"),
    ("归档", "/archives"),
    ("友链", "/friends"),
    ("关于", "/about"),
    ("写作指南", "/about/writing"),
    ("更新日志", "/changelog"),
    ("组件图鉴", "/about/components"),
];

/// 最近搜索在 localStorage 中的键与条数上限。
#[cfg(target_arch = "wasm32")]
const RECENT_KEY: &str = "yggdrasil-search-recent";
const RECENT_LIMIT: usize = 5;
/// 输入防抖与进度点出现的延迟（毫秒）。
const DEBOUNCE_MS: u32 = 150;
const PROGRESS_DELAY_MS: u32 = 300;

fn page_route(path: &str) -> Route {
    match path {
        "/notes" => Route::Notes {},
        "/archives" => Route::Archives {},
        "/friends" => Route::Friends {},
        "/about" => Route::About {},
        "/about/writing" => Route::WritingGuide {},
        "/changelog" => Route::Changelog {},
        "/about/components" => Route::ComponentShowcase {},
        _ => Route::Home {},
    }
}

/// 忽略大小写的子串匹配，空查询视为匹配。
fn contains_ci(text: &str, query: &str) -> bool {
    let q = query.trim();
    q.is_empty() || text.to_lowercase().contains(&q.to_lowercase())
}

/// 按忽略大小写的命中切分文本，`true` 段需要高亮。按字符比较，中文与西文都适用。
fn highlight(text: &str, query: &str) -> Vec<(String, bool)> {
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
fn snippet(text: &str, query: &str, radius: usize) -> String {
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

/// 各组的条数上限：全部范围下文章 5、标签 3、跳转 3；单一范围 8。
fn group_limit(scope: Scope, group: Scope) -> usize {
    match (scope, group) {
        (Scope::All, Scope::Posts) => 5,
        (Scope::All, _) => 3,
        _ => 8,
    }
}

/// 按查询与范围组织面板内容。`posts` 为建议接口对当前查询的结果。
fn build_groups(
    query: &str,
    scope: Scope,
    posts: &[PostSuggestion],
    tags: &[(String, i64)],
    recents: Option<&[String]>,
    dark: bool,
) -> Vec<Group> {
    let q = query.trim();
    let pages = |limit: usize| -> Vec<Item> {
        PAGES
            .iter()
            .filter(|(label, path)| contains_ci(label, q) || contains_ci(path, q))
            .take(limit)
            .map(|(label, path)| Item::Page { label, path })
            .collect()
    };
    let mut groups = Vec::new();
    if q.is_empty() {
        if let Some(recents) = recents.filter(|r| !r.is_empty()) {
            groups.push(Group {
                label: "最近搜索",
                items: recents.iter().cloned().map(Item::Recent).collect(),
            });
        }
        let mut jump = pages(4);
        jump.push(if dark {
            Item::Theme {
                to: Theme::Light,
                label: "切换到浅色",
            }
        } else {
            Item::Theme {
                to: Theme::Dark,
                label: "切换到深色",
            }
        });
        groups.push(Group {
            label: "跳转",
            items: jump,
        });
        return groups;
    }
    if matches!(scope, Scope::All | Scope::Posts) {
        let items: Vec<Item> = posts
            .iter()
            .take(group_limit(scope, Scope::Posts))
            .cloned()
            .map(Item::Post)
            .collect();
        if !items.is_empty() {
            groups.push(Group {
                label: "文章",
                items,
            });
        }
    }
    if matches!(scope, Scope::All | Scope::Tags) {
        let items: Vec<Item> = tags
            .iter()
            .filter(|(name, _)| contains_ci(name, q))
            .take(group_limit(scope, Scope::Tags))
            .map(|(name, count)| Item::Tag {
                name: name.clone(),
                count: *count,
            })
            .collect();
        if !items.is_empty() {
            groups.push(Group {
                label: "标签",
                items,
            });
        }
    }
    if matches!(scope, Scope::All | Scope::Pages) {
        let items = pages(group_limit(scope, Scope::Pages));
        if !items.is_empty() {
            groups.push(Group {
                label: "跳转",
                items,
            });
        }
    }
    groups
}

/// 有查询时各范围的命中数（用于范围标签）。
fn scope_count(
    scope: Scope,
    query: &str,
    posts: &[PostSuggestion],
    tags: &[(String, i64)],
) -> usize {
    let q = query.trim();
    match scope {
        Scope::All => {
            scope_count(Scope::Posts, q, posts, tags)
                + scope_count(Scope::Tags, q, posts, tags)
                + scope_count(Scope::Pages, q, posts, tags)
        }
        Scope::Posts => posts.len(),
        Scope::Tags => tags.iter().filter(|(name, _)| contains_ci(name, q)).count(),
        Scope::Pages => PAGES
            .iter()
            .filter(|(label, path)| contains_ci(label, q) || contains_ci(path, q))
            .count(),
    }
}

/// 读取最近搜索；存储不可用或内容损坏时返回 `None`（该组不显示）。
fn read_recents() -> Option<Vec<String>> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = web_sys::window()?.local_storage().ok()??;
        let raw = storage.get_item(RECENT_KEY).ok()?;
        match raw {
            None => Some(Vec::new()),
            Some(raw) => serde_json::from_str::<Vec<String>>(&raw).ok(),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

/// 把查询记入最近搜索：去重、置顶、最多保留 5 条。
fn remember(recents: &[String], query: &str) -> Vec<String> {
    let q = query.trim();
    let mut next: Vec<String> = Vec::with_capacity(RECENT_LIMIT);
    if !q.is_empty() {
        next.push(q.to_string());
    }
    next.extend(recents.iter().filter(|r| r.as_str() != q).cloned());
    next.truncate(RECENT_LIMIT);
    next
}

#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
fn write_recents(recents: &[String]) {
    #[cfg(target_arch = "wasm32")]
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        if let Ok(raw) = serde_json::to_string(recents) {
            let _ = storage.set_item(RECENT_KEY, &raw);
        }
    }
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

/// 全局 ⌘K / Ctrl+K：焦点在可编辑区域时不拦截。
#[cfg(target_arch = "wasm32")]
fn use_palette_shortcut(handle: SearchPaletteHandle, enabled: bool) {
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

/// 搜索面板。挂在前台布局里一次，由 [`SearchPaletteHandle`] 打开。
///
/// Props：
/// - `shortcut`：是否注册全局 ⌘K / Ctrl+K（图鉴演示实例关闭，避免与正式面板同时打开）
#[component]
pub fn SearchPalette(#[props(default = true)] shortcut: bool) -> Element {
    let handle = use_context::<SearchPaletteHandle>();
    let mut open = handle.open;
    #[cfg(target_arch = "wasm32")]
    use_palette_shortcut(handle, shortcut);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = shortcut;

    let mut query = use_signal(String::new);
    let mut scope = use_signal(|| Scope::All);
    let mut active = use_signal(|| 0_usize);
    let mut posts = use_signal(Vec::<PostSuggestion>::new);
    let mut posts_query = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut show_progress = use_signal(|| false);
    let mut seq = use_signal(|| 0_u64);
    let mut composing = use_signal(|| false);
    let mut tags = use_signal(|| None::<Vec<(String, i64)>>);
    let mut recents = use_signal(|| None::<Vec<String>>);
    let theme = use_theme();
    let resolved = use_resolved_theme();
    let navigator = use_navigator();
    // 打开前的焦点，关闭后还回去。
    #[cfg(target_arch = "wasm32")]
    let mut previous_focus = use_signal(|| None::<web_sys::HtmlElement>);

    // 打开：记下原焦点、读最近搜索、首次加载标签、聚焦输入框。
    use_effect(move || {
        if !open() {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            previous_focus.set(
                web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.active_element())
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok()),
            );
        }
        recents.set(read_recents());
        if tags.peek().is_none() {
            spawn(async move {
                if let Ok(res) = list_tags().await {
                    tags.set(Some(
                        res.tags
                            .into_iter()
                            .filter(|t| t.post_count > 0)
                            .map(|t| (t.name, t.post_count))
                            .collect(),
                    ));
                }
            });
        }
        spawn(async move {
            crate::utils::time::sleep_ms(0).await;
            #[cfg(target_arch = "wasm32")]
            focus_by_id("palette-input");
        });
    });

    let mut close = move || {
        if !*open.peek() {
            return;
        }
        // 丢弃未完成的请求与防抖；下次打开从空白开始（显示最近搜索与跳转）。
        seq += 1;
        loading.set(false);
        show_progress.set(false);
        query.set(String::new());
        scope.set(Scope::All);
        active.set(0);
        posts.set(Vec::new());
        posts_query.set(String::new());
        open.set(false);
        #[cfg(target_arch = "wasm32")]
        if let Some(el) = previous_focus.take() {
            let _ = el.focus();
        }
    };

    let mut schedule_fetch = move || {
        seq += 1;
        let id = seq();
        let q = query.peek().trim().to_string();
        if q.is_empty() {
            posts.set(Vec::new());
            posts_query.set(String::new());
            loading.set(false);
            show_progress.set(false);
            return;
        }
        spawn(async move {
            crate::utils::time::sleep_ms(DEBOUNCE_MS).await;
            if seq() != id || composing() {
                return;
            }
            loading.set(true);
            spawn(async move {
                crate::utils::time::sleep_ms(PROGRESS_DELAY_MS).await;
                if seq() == id && loading() {
                    show_progress.set(true);
                }
            });
            let res = suggest_posts(q.clone()).await;
            if seq() != id {
                return;
            }
            // 失败或被限流时保留上一次结果，不闪空列表。
            if let Ok(res) = res {
                posts.set(res.posts);
                posts_query.set(q);
            }
            loading.set(false);
            show_progress.set(false);
        });
    };

    if !open() {
        return rsx! {};
    }

    let q = query();
    let has_query = !q.trim().is_empty();
    // 建议结果属于更早的查询时仍先显示（加载中保留上一次结果）。
    let tag_list = tags().unwrap_or_default();
    let groups = build_groups(
        &q,
        scope(),
        &posts(),
        &tag_list,
        recents().as_deref(),
        resolved() == ResolvedTheme::Dark,
    );
    let flat: Vec<Item> = groups.iter().flat_map(|g| g.items.clone()).collect();
    let total = flat.len();
    let active_idx = if total == 0 {
        0
    } else {
        active().min(total - 1)
    };
    let waiting_posts = has_query && posts_query() != q.trim() && loading();
    let no_results = has_query && total == 0 && !waiting_posts;

    let mut activate = move |item: Item, new_tab: bool| {
        let current = query.peek().clone();
        match item {
            Item::Recent(term) => {
                query.set(term);
                active.set(0);
                schedule_fetch();
            }
            Item::Theme { to, .. } => {
                close();
                #[cfg(target_arch = "wasm32")]
                let origin = web_sys::window()
                    .map(|w| {
                        let width = w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let height = w
                            .inner_height()
                            .ok()
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        (width / 2.0, height / 2.0)
                    })
                    .unwrap_or((0.0, 0.0));
                #[cfg(not(target_arch = "wasm32"))]
                let origin = (0.0, 0.0);
                choose_theme(theme, to, origin);
            }
            Item::Post(post) => {
                let next = remember(&recents.peek().clone().unwrap_or_default(), &current);
                write_recents(&next);
                close();
                if new_tab {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(window) = web_sys::window() {
                        let _ = window
                            .open_with_url_and_target(&format!("/post/{}", post.slug), "_blank");
                    }
                } else {
                    navigator.push(Route::PostDetail { slug: post.slug });
                }
            }
            Item::Tag { name, .. } => {
                let next = remember(&recents.peek().clone().unwrap_or_default(), &current);
                write_recents(&next);
                close();
                navigator.push(Route::TagDetail { tag: name });
            }
            Item::Page { path, .. } => {
                close();
                navigator.push(page_route(path));
            }
        }
    };
    let flat_for_keys = flat.clone();
    let full_text_query = q.trim().to_string();

    rsx! {
        div { class: "palette-scrim", onclick: move |_| close() }
        div {
            class: "palette",
            role: "dialog",
            aria_modal: "true",
            aria_label: "搜索",
            // 复用弹窗的焦点锁定（Tab 只在面板内循环）。
            "data-ygg-modal-panel": "true",
            "data-ygg-modal-open": "true",
            onkeydown: move |e| {
                if e.key() == Key::Tab {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use dioxus::web::WebEventExt;
                        crate::components::ui::trap_tab(&e.as_web_event(), "搜索");
                    }
                }
            },
            div { class: "palette-input-row",
                Icon { name: IconName::Search, size: 18, class: "palette-input-icon" }
                input {
                    id: "palette-input",
                    class: "palette-input",
                    r#type: "text",
                    role: "combobox",
                    aria_label: "搜索文章、标签或页面",
                    aria_expanded: "true",
                    aria_controls: "palette-list",
                    aria_activedescendant: (total > 0).then(|| format!("palette-opt-{active_idx}")),
                    aria_autocomplete: "list",
                    autocomplete: "off",
                    spellcheck: "false",
                    enterkeyhint: "go",
                    maxlength: "100",
                    placeholder: "搜索文章、标签，或跳转到页面…",
                    value: "{q}",
                    oninput: move |e| {
                        query.set(e.value());
                        active.set(0);
                        if !composing() {
                            schedule_fetch();
                        }
                    },
                    oncompositionstart: move |_| composing.set(true),
                    oncompositionend: move |_| {
                        composing.set(false);
                        schedule_fetch();
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
                                if let Some(item) = flat_for_keys.get(active_idx).cloned() {
                                    let modifiers = e.modifiers();
                                    activate(item, modifiers.meta() || modifiers.ctrl());
                                }
                            }
                            Key::Escape => {
                                e.prevent_default();
                                if query.peek().is_empty() {
                                    close();
                                } else {
                                    query.set(String::new());
                                    active.set(0);
                                    schedule_fetch();
                                }
                            }
                            _ => {}
                        }
                    },
                }
                if show_progress() {
                    span { class: "palette-progress", role: "status", aria_label: "正在搜索",
                        span {}
                        span {}
                        span {}
                    }
                }
                if has_query {
                    button {
                        class: "btn btn-xs btn-icon btn-ghost",
                        r#type: "button",
                        aria_label: "清除输入",
                        onclick: move |_| {
                            query.set(String::new());
                            active.set(0);
                            schedule_fetch();
                            #[cfg(target_arch = "wasm32")]
                            focus_by_id("palette-input");
                        },
                        Icon { name: IconName::X, size: 14 }
                    }
                }
                button { class: "palette-esc", r#type: "button", onclick: move |_| close(), "Esc" }
            }
            div { class: "palette-scopes", role: "tablist", aria_label: "搜索范围",
                for s in Scope::ALL {
                    button {
                        key: "{s.label()}",
                        class: "palette-scope",
                        r#type: "button",
                        role: "tab",
                        aria_selected: "{scope() == s}",
                        onclick: move |_| {
                            scope.set(s);
                            active.set(0);
                            #[cfg(target_arch = "wasm32")]
                            focus_by_id("palette-input");
                        },
                        "{s.label()}"
                        if has_query {
                            span { class: "palette-scope-count",
                                "{scope_count(s, &q, &posts(), &tag_list)}"
                            }
                        }
                    }
                }
            }
            div { id: "palette-list", class: "palette-list", role: "listbox", aria_label: "搜索结果",
                {
                    let mut index = 0_usize;
                    rsx! {
                        for group in groups {
                            div { key: "{group.label}", role: "group", aria_label: group.label,
                                p { class: "palette-group-label", "{group.label}" }
                                for item in group.items {
                                    {
                                        let i = index;
                                        index += 1;
                                        let item_for_click = item.clone();
                                        rsx! {
                                            PaletteRow {
                                                key: "{i}",
                                                index: i,
                                                item,
                                                query: q.clone(),
                                                selected: i == active_idx,
                                                on_hover: move |_| active.set(i),
                                                on_pick: move |new_tab: bool| activate(item_for_click.clone(), new_tab),
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
                        p { class: "palette-empty-title", "没有找到「{full_text_query}」" }
                        p { class: "palette-empty-hint", "标题、摘要与标签里都没有命中，试试更短的关键词，或去全文搜索。" }
                        button {
                            class: "btn btn-sm btn-secondary mt-4",
                            r#type: "button",
                            onclick: {
                                let full_text_query = full_text_query.clone();
                                move |_| {
                                    close();
                                    navigator.push(Route::Search { q: full_text_query.clone() });
                                }
                            },
                            "在全文中搜索"
                            Icon { name: IconName::ArrowRight, size: 14 }
                        }
                    }
                }
            }
            div { class: "palette-footer",
                div { class: "palette-hints",
                    span { kbd { "↑" } kbd { "↓" } "选择" }
                    span { kbd { "↵" } "打开" }
                    span { kbd { "Esc" } "关闭" }
                }
                if has_query {
                    button {
                        class: "palette-full-text",
                        r#type: "button",
                        onclick: move |_| {
                            close();
                            navigator.push(Route::Search { q: full_text_query.clone() });
                        },
                        "全文搜索「{q.trim()}」"
                        Icon { name: IconName::ArrowRight, size: 13 }
                    }
                }
            }
        }
    }
}

/// 面板中的一行：左侧图标块，中间标题（文章另有摘要片段），右侧元信息或 ↵ 键帽。
#[component]
fn PaletteRow(
    index: usize,
    item: Item,
    query: String,
    selected: bool,
    on_hover: EventHandler<()>,
    on_pick: EventHandler<bool>,
) -> Element {
    let (icon, title, summary, meta, mono) = match &item {
        Item::Post(post) => (
            IconName::FileText,
            post.title.clone(),
            post.summary
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(|s| snippet(s, &query, 18)),
            post.published_at
                .map(|d| d.format("%Y.%m.%d").to_string())
                .unwrap_or_default(),
            true,
        ),
        Item::Tag { name, count } => (
            IconName::Tag,
            name.clone(),
            None,
            format!("{count} 篇"),
            false,
        ),
        Item::Page { label, path } => (
            IconName::CornerDownLeft,
            label.to_string(),
            None,
            path.to_string(),
            true,
        ),
        Item::Recent(term) => (
            IconName::RotateCcwClock,
            term.clone(),
            None,
            String::new(),
            false,
        ),
        Item::Theme { label, .. } => (
            IconName::Moon,
            label.to_string(),
            None,
            "外观".to_string(),
            false,
        ),
    };
    let highlight_query = if matches!(item, Item::Recent(_) | Item::Theme { .. }) {
        String::new()
    } else {
        query
    };
    rsx! {
        div {
            id: "palette-opt-{index}",
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
            span { class: "palette-option-tile", Icon { name: icon, size: 15 } }
            span { class: "palette-option-text",
                span { class: "palette-option-title",
                    for (text, hit) in highlight(&title, &highlight_query) {
                        if hit {
                            mark { "{text}" }
                        } else {
                            "{text}"
                        }
                    }
                }
                if let Some(summary) = summary {
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
            } else if !meta.is_empty() {
                span { class: if mono { "palette-option-meta font-mono" } else { "palette-option-meta" }, "{meta}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(slug: &str, title: &str) -> PostSuggestion {
        PostSuggestion {
            slug: slug.into(),
            title: title.into(),
            summary: None,
            published_at: None,
            tags: Vec::new(),
        }
    }

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

    #[test]
    fn empty_query_shows_recents_and_jumps() {
        let recents = vec!["waker".to_string()];
        let groups = build_groups("", Scope::All, &[], &[], Some(&recents), false);
        assert_eq!(groups[0].label, "最近搜索");
        assert_eq!(groups[1].label, "跳转");
        assert_eq!(
            groups[1].items.last(),
            Some(&Item::Theme {
                to: Theme::Dark,
                label: "切换到深色"
            })
        );
        // 存储不可用时不显示最近搜索。
        assert_eq!(
            build_groups("", Scope::All, &[], &[], None, true)[0].label,
            "跳转"
        );
    }

    #[test]
    fn groups_respect_scope_limits() {
        let posts: Vec<_> = (0..8).map(|i| post(&format!("p{i}"), "关于源码")).collect();
        let tags: Vec<_> = (0..5).map(|i| (format!("关于{i}"), 1)).collect();
        let all = build_groups("关于", Scope::All, &posts, &tags, None, false);
        assert_eq!(
            all.iter()
                .map(|g| (g.label, g.items.len()))
                .collect::<Vec<_>>(),
            [("文章", 5), ("标签", 3), ("跳转", 1)]
        );
        let only_posts = build_groups("关于", Scope::Posts, &posts, &tags, None, false);
        assert_eq!(only_posts.len(), 1);
        assert_eq!(only_posts[0].items.len(), 8);
        assert_eq!(scope_count(Scope::All, "关于", &posts, &tags), 8 + 5 + 1);
    }

    #[test]
    fn remember_dedupes_and_caps_recents() {
        let recents: Vec<String> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(remember(&recents, " c "), ["c", "a", "b", "d", "e"]);
        assert_eq!(remember(&recents, "f"), ["f", "a", "b", "c", "d"]);
        assert_eq!(remember(&recents, " "), ["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn page_routes_cover_every_jump_target() {
        for (_, path) in PAGES {
            let route = page_route(path);
            assert_eq!(route.to_string(), path, "{path}");
        }
    }
}
