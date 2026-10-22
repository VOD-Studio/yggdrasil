//! 搜索面板（⌘K）：前台全站的即时跳转。
//!
//! 页头「搜索 ⌘K」与全局快捷键打开同一个面板。文章来自 `suggest_posts`（标题、
//! 摘要、标签，最多 8 条），标签与页面在浏览器内过滤。空查询显示最近搜索与常用
//! 跳转；有查询时按「文章 · 标签 · 跳转」分组，命中词用 `<mark>` 标出。
//! 样式见 styles/components/palette.css；行为细节见 docs/v1.0-重设计方案.md「搜索面板」。
//! 公共状态、输入与对话框结构见 [crate::components::palette]。

use dioxus::prelude::*;

use crate::api::posts::{list_tags, suggest_posts, PostSuggestion};
use crate::components::palette::{
    contains_ci, snippet, use_palette_state, viewport_center, PaletteEntry, PaletteGroup,
    PaletteShell,
};
use crate::components::ui::{Icon, IconName};
use crate::router::Route;
use crate::theme::{choose_theme, use_resolved_theme, use_theme, ResolvedTheme, Theme};

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

fn page_target(path: &str) -> NavigationTarget<Route> {
    let route = match path {
        "/notes" => Route::Notes {},
        "/archives" => Route::Archives {},
        "/friends" => Route::Friends {},
        "/about" => Route::About {},
        "/about/writing" => Route::WritingGuide {},
        "/changelog" => Route::Changelog {},
        // 图鉴是 full 客户端专属路由（plan §6.1/§6.6）：reader 客户端没有该变体，
        // 两个 flavor 统一整页导航，由服务端按路径选择 full 模板。
        "/about/components" => return NavigationTarget::External(path.to_string()),
        _ => Route::Home {},
    };
    route.into()
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

/// 面板中的一行的展示内容。
fn entry(item: &Item, query: &str) -> PaletteEntry {
    match item {
        Item::Post(post) => PaletteEntry {
            icon: IconName::FileText,
            title: post.title.clone(),
            summary: post
                .summary
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(|s| snippet(s, query, 18)),
            meta: post
                .published_at
                .map(|d| d.format("%Y.%m.%d").to_string())
                .unwrap_or_default(),
            mono_meta: true,
            highlight_hits: true,
        },
        Item::Tag { name, count } => PaletteEntry {
            icon: IconName::Tag,
            title: name.clone(),
            summary: None,
            meta: format!("{count} 篇"),
            mono_meta: false,
            highlight_hits: true,
        },
        Item::Page { label, path } => PaletteEntry {
            icon: IconName::CornerDownLeft,
            title: label.to_string(),
            summary: None,
            meta: path.to_string(),
            mono_meta: true,
            highlight_hits: true,
        },
        Item::Recent(term) => PaletteEntry {
            icon: IconName::RotateCcwClock,
            title: term.clone(),
            summary: None,
            meta: String::new(),
            mono_meta: false,
            highlight_hits: false,
        },
        Item::Theme { label, .. } => PaletteEntry {
            icon: IconName::Moon,
            title: label.to_string(),
            summary: None,
            meta: "外观".to_string(),
            mono_meta: false,
            highlight_hits: false,
        },
    }
}

/// 搜索面板。挂在前台布局里一次，由 [`SearchPaletteHandle`] 打开。
///
/// Props：
/// - `shortcut`：是否注册全局 ⌘K / Ctrl+K（图鉴演示实例关闭，避免与正式面板同时打开）
#[component]
pub fn SearchPalette(#[props(default = true)] shortcut: bool) -> Element {
    #[cfg(target_arch = "wasm32")]
    crate::components::palette::use_palette_shortcut(
        use_context::<crate::components::palette::SearchPaletteHandle>(),
        shortcut,
    );
    #[cfg(not(target_arch = "wasm32"))]
    let _ = shortcut;
    let palette = use_palette_state("palette");
    let open = palette.open;
    let mut query = palette.query;
    let mut active = palette.active;

    let mut scope = use_signal(|| Scope::All);
    let mut posts = use_signal(Vec::<PostSuggestion>::new);
    let mut posts_query = use_signal(String::new);
    let mut tags = use_signal(|| None::<Vec<(String, i64)>>);
    let mut recents = use_signal(|| None::<Vec<String>>);
    let theme = use_theme();
    let resolved = use_resolved_theme();
    let navigator = use_navigator();

    // 打开：读最近搜索、首次加载标签。
    use_effect(move || {
        if !open() {
            return;
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
    });

    let close = move || {
        // 淡出结束后复位，下次打开从空白开始（显示最近搜索与跳转）。
        palette.close(move || {
            scope.set(Scope::All);
            posts.set(Vec::new());
            posts_query.set(String::new());
        });
    };

    let schedule_fetch = move || {
        // 失败或被限流时保留上一次结果，不闪空列表。
        palette.schedule_fetch(posts, posts_query, |q| async move {
            suggest_posts(q).await.ok().map(|res| res.posts)
        })
    };

    if !palette.visible() {
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
    let view: Vec<PaletteGroup> = groups
        .iter()
        .map(|g| PaletteGroup {
            label: g.label,
            entries: g.items.iter().map(|item| entry(item, &q)).collect(),
        })
        .collect();

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
                choose_theme(theme, to, viewport_center());
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
                navigator.push(page_target(path));
            }
        }
    };
    let empty_query = q.trim().to_string();
    let footer_query = empty_query.clone();

    rsx! {
        PaletteShell {
            state: palette,
            label: "搜索",
            input_label: "搜索文章、标签或页面",
            placeholder: "搜索文章、标签，或跳转到页面…",
            list_label: "搜索结果",
            enter_hint: "打开",
            empty_hint: "标题、摘要与标签里都没有命中，试试更短的关键词，或去全文搜索。",
            groups: view,
            waiting: posts_query() != q.trim(),
            clearable: true,
            on_query_change: move |_| schedule_fetch(),
            on_pick: move |(index, new_tab): (usize, bool)| {
                if let Some(item) = flat.get(index).cloned() {
                    activate(item, new_tab);
                }
            },
            on_close: move |_| close(),
            toolbar: rsx! {
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
                                palette.focus_input();
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
            },
            empty_action: rsx! {
                button {
                    class: "btn btn-sm btn-secondary mt-4",
                    r#type: "button",
                    onclick: move |_| {
                        close();
                        navigator.push(Route::Search { q: empty_query.clone() });
                    },
                    "在全文中搜索"
                    Icon { name: IconName::ArrowRight, size: 14 }
                }
            },
            footer_action: rsx! {
                if has_query {
                    button {
                        class: "palette-full-text",
                        r#type: "button",
                        onclick: move |_| {
                            close();
                            navigator.push(Route::Search { q: footer_query.clone() });
                        },
                        "全文搜索「{q.trim()}」"
                        Icon { name: IconName::ArrowRight, size: 13 }
                    }
                }
            },
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
            let target = match page_target(path) {
                NavigationTarget::Internal(route) => route.to_string(),
                NavigationTarget::External(url) => url,
            };
            assert_eq!(target, *path, "{path}");
        }
    }
}
