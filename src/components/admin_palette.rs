//! 后台命令面板（⌘K）：搜索文章、执行操作、跳转栏目。
//!
//! 与前台搜索面板共用打开句柄、全局快捷键、命中高亮与 palette.css 的样式；数据与
//! 动作是后台自己的。空查询显示常用操作与全部栏目；有查询时按「文章 · 操作 · 跳转」
//! 分组，文章来自后台列表接口（含草稿，按标题匹配，最多 6 篇），回车进入编辑。

use dioxus::prelude::*;

use crate::components::admin_layout::AdminSection;
use crate::components::search_palette::{
    contains_ci, highlight, viewport_center, SearchPaletteHandle,
};
use crate::components::ui::{Icon, IconName};
use crate::models::post::{PostListItem, PostStatus};
use crate::router::Route;
use crate::theme::{choose_theme, use_resolved_theme, use_theme, ResolvedTheme, Theme};

/// 文章搜索结果的条数上限。
const POST_LIMIT: i32 = 6;
/// 输入防抖与进度点出现的延迟（毫秒），与前台面板一致。
const DEBOUNCE_MS: u32 = 150;
const PROGRESS_DELAY_MS: u32 = 300;

/// 面板里的动作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    WritePost,
    NewNote,
    Upload,
    OpenSite,
    Theme(Theme),
    Logout,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::WritePost => "写文章",
            Action::NewNote => "新建笔记",
            Action::Upload => "上传素材",
            Action::OpenSite => "查看前台",
            Action::Theme(Theme::Dark) => "切换到深色",
            Action::Theme(_) => "切换到浅色",
            Action::Logout => "退出登录",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Action::WritePost => IconName::Pencil,
            Action::NewNote => IconName::NotebookPen,
            Action::Upload => IconName::Upload,
            Action::OpenSite => IconName::ArrowUpRight,
            Action::Theme(Theme::Dark) => IconName::Moon,
            Action::Theme(_) => IconName::Sun,
            Action::Logout => IconName::LogOut,
        }
    }
}

/// 可跳转的页面：侧栏栏目之外，再加回收站、笔记本与个人资料。
fn jump_targets() -> Vec<(&'static str, IconName, Route)> {
    let mut targets: Vec<_> = [
        AdminSection::Dashboard,
        AdminSection::Posts,
        AdminSection::Notes,
        AdminSection::Assets,
        AdminSection::Comments,
        AdminSection::Friends,
        AdminSection::Mcp,
        AdminSection::Settings,
        AdminSection::System,
        AdminSection::Logs,
        AdminSection::Runner,
    ]
    .into_iter()
    .map(|section| (section.label(), section.icon(), section.route()))
    .collect();
    targets.push(("回收站", IconName::Trash, Route::PostsTrash {}));
    targets.push(("笔记本", IconName::BookOpen, Route::AdminNotebooks {}));
    targets.push(("个人资料", IconName::User, Route::Profile {}));
    targets
}

#[derive(Clone, Debug, PartialEq)]
enum Item {
    Post(PostListItem),
    Action(Action),
    Jump {
        label: &'static str,
        icon: IconName,
        route: Route,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct Group {
    label: &'static str,
    items: Vec<Item>,
}

/// 按查询组织面板内容。空查询：常用操作与全部栏目；有查询：命中的文章、操作与栏目。
fn build_groups(query: &str, posts: &[PostListItem], dark: bool) -> Vec<Group> {
    let q = query.trim();
    let theme = Action::Theme(if dark { Theme::Light } else { Theme::Dark });
    let actions = [
        Action::WritePost,
        Action::NewNote,
        Action::Upload,
        theme,
        Action::OpenSite,
        Action::Logout,
    ];
    let mut groups = Vec::new();
    if !q.is_empty() && !posts.is_empty() {
        groups.push(Group {
            label: "文章",
            items: posts.iter().cloned().map(Item::Post).collect(),
        });
    }
    let actions: Vec<Item> = actions
        .into_iter()
        // 空查询时不把退出登录放进常用操作，避免误触。
        .filter(|a| {
            if q.is_empty() {
                *a != Action::Logout
            } else {
                contains_ci(a.label(), q)
            }
        })
        .map(Item::Action)
        .collect();
    if !actions.is_empty() {
        groups.push(Group {
            label: "操作",
            items: actions,
        });
    }
    let jumps: Vec<Item> = jump_targets()
        .into_iter()
        .filter(|(label, _, _)| contains_ci(label, q))
        .map(|(label, icon, route)| Item::Jump { label, icon, route })
        .collect();
    if !jumps.is_empty() {
        groups.push(Group {
            label: "跳转",
            items: jumps,
        });
    }
    groups
}

/// 后台命令面板。挂在后台布局里一次，由 [`SearchPaletteHandle`] 打开。
///
/// Props：
/// - `on_logout`：「退出登录」命令，与侧栏退出按钮同一处理
#[component]
pub fn AdminPalette(on_logout: EventHandler<()>) -> Element {
    let handle = use_context::<SearchPaletteHandle>();
    let mut open = handle.open;
    #[cfg(target_arch = "wasm32")]
    crate::components::search_palette::use_palette_shortcut(handle, true);

    let mut query = use_signal(String::new);
    let mut active = use_signal(|| 0_usize);
    let mut posts = use_signal(Vec::<PostListItem>::new);
    let mut posts_query = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut show_progress = use_signal(|| false);
    let mut seq = use_signal(|| 0_u64);
    let mut composing = use_signal(|| false);
    let theme = use_theme();
    let resolved = use_resolved_theme();
    let navigator = use_navigator();
    #[cfg(target_arch = "wasm32")]
    let mut previous_focus = use_signal(|| None::<web_sys::HtmlElement>);

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
        spawn(async move {
            crate::utils::time::sleep_ms(0).await;
            #[cfg(target_arch = "wasm32")]
            crate::components::search_palette::focus_by_id("admin-palette-input");
        });
    });

    let mut close = move || {
        if !*open.peek() {
            return;
        }
        seq += 1;
        loading.set(false);
        show_progress.set(false);
        query.set(String::new());
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
            let res = crate::api::posts::list_posts(
                1,
                POST_LIMIT,
                crate::api::posts::AdminPostQuery::search(q.clone()),
            )
            .await;
            if seq() != id {
                return;
            }
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
    let groups = build_groups(&q, &posts(), resolved() == ResolvedTheme::Dark);
    let flat: Vec<Item> = groups.iter().flat_map(|g| g.items.clone()).collect();
    let total = flat.len();
    let active_idx = if total == 0 {
        0
    } else {
        active().min(total - 1)
    };
    let waiting_posts = has_query && posts_query() != q.trim() && loading();
    let no_results = has_query && total == 0 && !waiting_posts;

    let mut activate = move |item: Item| {
        close();
        match item {
            Item::Post(post) => {
                navigator.push(Route::WriteEdit { id: post.id });
            }
            Item::Jump { route, .. } => {
                navigator.push(route);
            }
            Item::Action(action) => match action {
                Action::WritePost => {
                    navigator.push(Route::Write {});
                }
                Action::NewNote => {
                    navigator.push(Route::NewNote {});
                }
                Action::Upload => {
                    navigator.push(Route::Assets {});
                }
                Action::OpenSite => {
                    // admin→前台跨 layout，保留整页加载。
                    #[cfg(target_arch = "wasm32")]
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().set_href("/");
                    }
                }
                Action::Theme(to) => choose_theme(theme, to, viewport_center()),
                Action::Logout => on_logout.call(()),
            },
        }
    };
    let flat_for_keys = flat.clone();

    rsx! {
        div { class: "palette-scrim", onclick: move |_| close() }
        div {
            class: "palette",
            role: "dialog",
            aria_modal: "true",
            aria_label: "命令面板",
            "data-ygg-modal-panel": "true",
            "data-ygg-modal-open": "true",
            onkeydown: move |e| {
                if e.key() == Key::Tab {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use dioxus::web::WebEventExt;
                        crate::components::ui::trap_tab(&e.as_web_event(), "命令面板");
                    }
                }
            },
            div { class: "palette-input-row",
                Icon { name: IconName::Search, size: 18, class: "palette-input-icon" }
                input {
                    id: "admin-palette-input",
                    class: "palette-input",
                    r#type: "text",
                    role: "combobox",
                    aria_label: "搜索文章或输入命令",
                    aria_expanded: "true",
                    aria_controls: "admin-palette-list",
                    aria_activedescendant: (total > 0).then(|| format!("admin-palette-opt-{active_idx}")),
                    aria_autocomplete: "list",
                    autocomplete: "off",
                    spellcheck: "false",
                    enterkeyhint: "go",
                    maxlength: "100",
                    placeholder: "搜索文章，或输入命令…",
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
                                    activate(item);
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
                button { class: "palette-esc", r#type: "button", onclick: move |_| close(), "Esc" }
            }
            div {
                id: "admin-palette-list",
                class: "palette-list",
                role: "listbox",
                aria_label: "命令与搜索结果",
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
                                            AdminPaletteRow {
                                                key: "{i}",
                                                index: i,
                                                item,
                                                query: q.clone(),
                                                selected: i == active_idx,
                                                on_hover: move |_| active.set(i),
                                                on_pick: move |_| activate(item_for_click.clone()),
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
                        p { class: "palette-empty-hint", "文章标题、命令与栏目里都没有命中。" }
                    }
                }
            }
            div { class: "palette-footer",
                div { class: "palette-hints",
                    span { kbd { "↑" } kbd { "↓" } "选择" }
                    span { kbd { "↵" } "执行" }
                    span { kbd { "Esc" } "关闭" }
                }
            }
        }
    }
}

/// 面板中的一行：左侧图标块，中间标题，右侧元信息或 ↵ 键帽。
#[component]
fn AdminPaletteRow(
    index: usize,
    item: Item,
    query: String,
    selected: bool,
    on_hover: EventHandler<()>,
    on_pick: EventHandler<()>,
) -> Element {
    let (icon, title, meta) = match &item {
        Item::Post(post) => (
            IconName::FileText,
            post.title.clone(),
            match post.status {
                PostStatus::Draft => "草稿".to_string(),
                PostStatus::Published => post
                    .published_at
                    .map(|d| d.format("%Y.%m.%d").to_string())
                    .unwrap_or_default(),
            },
        ),
        Item::Action(action) => (action.icon(), action.label().to_string(), String::new()),
        Item::Jump { label, icon, .. } => (*icon, label.to_string(), "跳转".to_string()),
    };
    rsx! {
        div {
            id: "admin-palette-opt-{index}",
            class: "palette-option",
            role: "option",
            aria_selected: "{selected}",
            onmousemove: move |_| {
                if !selected {
                    on_hover.call(());
                }
            },
            onmousedown: move |e| e.prevent_default(),
            onclick: move |_| on_pick.call(()),
            span { class: "palette-option-tile", Icon { name: icon, size: 15 } }
            span { class: "palette-option-text",
                span { class: "palette-option-title",
                    for (text, hit) in highlight(&title, &query) {
                        if hit {
                            mark { "{text}" }
                        } else {
                            "{text}"
                        }
                    }
                }
            }
            if selected {
                kbd { class: "palette-enter", "↵" }
            } else if !meta.is_empty() {
                span { class: "palette-option-meta", "{meta}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(groups: &[Group]) -> Vec<(&'static str, usize)> {
        groups.iter().map(|g| (g.label, g.items.len())).collect()
    }

    #[test]
    fn empty_query_lists_common_actions_without_logout() {
        let groups = build_groups("", &[], false);
        assert_eq!(labels(&groups), [("操作", 5), ("跳转", 14)]);
        assert!(!groups[0].items.contains(&Item::Action(Action::Logout)));
        assert!(groups[0]
            .items
            .contains(&Item::Action(Action::Theme(Theme::Dark))));
        let dark = build_groups("", &[], true);
        assert!(dark[0]
            .items
            .contains(&Item::Action(Action::Theme(Theme::Light))));
    }

    #[test]
    fn query_filters_actions_and_jumps() {
        let groups = build_groups("退出", &[], false);
        assert_eq!(groups[0].items, [Item::Action(Action::Logout)]);
        let groups = build_groups("笔记", &[], false);
        assert_eq!(labels(&groups), [("操作", 1), ("跳转", 2)]);
        assert!(build_groups("zzz", &[], false).is_empty());
    }
}
