//! 搜索面板（⌘K）：前台全站的即时跳转。
//!
//! 页头「搜索 ⌘K」与全局快捷键打开同一个面板。文章来自 `suggest_posts`（标题、
//! 摘要、标签，最多 8 条），标签与页面在浏览器内过滤。空查询显示最近搜索与常用
//! 跳转；有查询时按「文章 · 标签 · 跳转」分组，命中词用 `<mark>` 标出。
//! 样式见 styles/components/palette.css；行为细节见 docs/v1.0-重设计方案.md「搜索面板」。
//! 公共状态、输入与对话框结构见 [crate::components::palette]。
//!
//! 分包：[`SearchPalette`] 外壳（打开状态、快捷键、最近搜索与标签的预取）留在主包；
//! 面板主体（分组构建、条目映射、选中动作与 `PaletteShell` 视图）在
//! [`search_palette_body`]（`#[leptos::lazy]`）里，首次打开面板时才加载分包。

use leptos::prelude::*;

use crate::api::posts::{list_tags, suggest_posts, PostSuggestion};
use crate::components::palette::{
    contains_ci, snippet, use_palette_shortcut, use_palette_state, viewport_center, PaletteEntry,
    PaletteGroup, PaletteShell, PaletteState, SearchPaletteHandle,
};
use crate::components::ui::{Icon, IconName};
use crate::routes::Route;
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

/// 跳转页面的站内地址（`PAGES` 里的路径对应的 `Route`）。
fn page_href(path: &str) -> String {
    let route = match path {
        "/notes" => Route::Notes,
        "/archives" => Route::Archives,
        "/friends" => Route::Friends,
        "/about" => Route::About,
        "/about/writing" => Route::WritingGuide,
        "/changelog" => Route::Changelog,
        "/about/components" => Route::ComponentShowcase,
        _ => Route::Home,
    };
    route.to_string()
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

/// 在新标签页打开站内地址（仅 wasm）。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
fn open_in_new_tab(href: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window.open_with_url_and_target(href, "_blank");
    }
}

/// 外壳与分包主体共享的面板数据信号（全部是 `Copy` 句柄）。
#[derive(Clone, Copy)]
struct SearchData {
    scope: RwSignal<Scope>,
    posts: RwSignal<Vec<PostSuggestion>>,
    /// `posts` 对应的查询。
    posts_query: RwSignal<String>,
    /// 全部标签（名称、已发布文章数）；尚未加载为 `None`。
    tags: RwSignal<Option<Vec<(String, i64)>>>,
    /// 最近搜索；存储不可用为 `None`。
    recents: RwSignal<Option<Vec<String>>>,
}

/// 面板主体：分组、条目、选中动作与对话框视图。分包函数，首次调用时加载分包。
///
/// 参数全部是 `Copy` 句柄，由 [`SearchPalette`] 外壳在打开时传入。
#[leptos::lazy]
fn search_palette_body(
    palette: PaletteState,
    data: SearchData,
    theme: RwSignal<Theme>,
    resolved: Memo<ResolvedTheme>,
) -> AnyView {
    let SearchData {
        scope,
        posts,
        posts_query,
        tags,
        recents,
    } = data;
    let query = palette.query;
    let active = palette.active;

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

    let has_query = move || !query.read().trim().is_empty();
    // 建议结果属于更早的查询时仍先显示（加载中保留上一次结果）。
    let groups = Memo::new(move |_| {
        let q = query.get();
        let scope = scope.get();
        let dark = resolved.get() == ResolvedTheme::Dark;
        posts.with(|posts| {
            tags.with(|tags| {
                recents.with(|recents| {
                    build_groups(
                        &q,
                        scope,
                        posts,
                        tags.as_deref().unwrap_or_default(),
                        recents.as_deref(),
                        dark,
                    )
                })
            })
        })
    });
    let view_groups = Signal::derive(move || {
        let q = query.get();
        groups.with(|groups| {
            groups
                .iter()
                .map(|g| PaletteGroup {
                    label: g.label,
                    entries: g.items.iter().map(|item| entry(item, &q)).collect(),
                })
                .collect::<Vec<_>>()
        })
    });
    let waiting = Signal::derive(move || posts_query.read().as_str() != query.read().trim());

    let remember_query = move |current: &str| {
        let next = recents.with_untracked(|r| remember(r.as_deref().unwrap_or_default(), current));
        write_recents(&next);
    };

    let activate = move |item: Item, new_tab: bool| {
        let current = query.get_untracked();
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
                remember_query(&current);
                close();
                let href = Route::PostDetail { slug: post.slug }.to_string();
                if new_tab {
                    open_in_new_tab(&href);
                } else {
                    crate::navigation::push(&href);
                }
            }
            Item::Tag { name, .. } => {
                remember_query(&current);
                close();
                crate::navigation::push(&Route::TagDetail { tag: name }.to_string());
            }
            Item::Page { path, .. } => {
                close();
                crate::navigation::push(&page_href(path));
            }
        }
    };

    let full_text_search = move || {
        let q = query.get_untracked().trim().to_string();
        close();
        crate::navigation::push(&Route::Search { q }.to_string());
    };

    view! {
        <PaletteShell
            state=palette
            label="搜索"
            input_label="搜索文章、标签或页面"
            placeholder="搜索文章、标签，或跳转到页面…"
            list_label="搜索结果"
            enter_hint="打开"
            empty_hint="标题、摘要与标签里都没有命中，试试更短的关键词，或去全文搜索。"
            groups=view_groups
            waiting=waiting
            clearable=true
            on_query_change=move |_| schedule_fetch()
            on_pick=move |(index, new_tab): (usize, bool)| {
                let item = groups
                    .with_untracked(|gs| gs.iter().flat_map(|g| g.items.iter()).nth(index).cloned());
                if let Some(item) = item {
                    activate(item, new_tab);
                }
            }
            on_close=move |_| close()
            toolbar=move || {
                view! {
                    <div class="palette-scopes" role="tablist" aria-label="搜索范围">
                        {Scope::ALL
                            .into_iter()
                            .map(|s| {
                                view! {
                                    <button
                                        class="palette-scope"
                                        type="button"
                                        role="tab"
                                        aria-selected=move || (scope.get() == s).to_string()
                                        on:click=move |_| {
                                            scope.set(s);
                                            active.set(0);
                                            palette.focus_input();
                                        }
                                    >
                                        {s.label()}
                                        {move || {
                                            has_query()
                                                .then(|| {
                                                    view! {
                                                        <span class="palette-scope-count">
                                                            {move || {
                                                                let q = query.get();
                                                                posts
                                                                    .with(|posts| {
                                                                        tags.with(|tags| {
                                                                            scope_count(
                                                                                s,
                                                                                &q,
                                                                                posts,
                                                                                tags.as_deref().unwrap_or_default(),
                                                                            )
                                                                        })
                                                                    })
                                                            }}
                                                        </span>
                                                    }
                                                })
                                        }}
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                }
            }
            empty_action=move || {
                view! {
                    <button
                        class="btn btn-sm btn-secondary mt-4"
                        type="button"
                        on:click=move |_| full_text_search()
                    >
                        "在全文中搜索"
                        <Icon name=IconName::ArrowRight size=14/>
                    </button>
                }
            }
            footer_action=move || {
                // ViewFn 只渲染一次：用内层闭包让按钮随查询词出现/消失。
                move || {
                    has_query()
                        .then(|| {
                            view! {
                                <button
                                    class="palette-full-text"
                                    type="button"
                                    on:click=move |_| full_text_search()
                                >
                                    {move || format!("全文搜索「{}」", query.read().trim())}
                                    <Icon name=IconName::ArrowRight size=13/>
                                </button>
                            }
                        })
                }
            }
        />
    }
    .into_any()
}

/// 搜索面板。挂在前台布局里一次，由 [`SearchPaletteHandle`] 打开。
///
/// 外壳只维护打开状态、最近搜索与标签预取；面板内容首次打开时才从分包加载
/// （[`search_palette_body`]），关闭淡出结束后卸载。
///
/// Props：
/// - `shortcut`：是否注册全局 ⌘K / Ctrl+K（图鉴演示实例关闭，避免与正式面板同时打开）
#[component]
pub fn SearchPalette(#[prop(default = true)] shortcut: bool) -> impl IntoView {
    use_palette_shortcut(expect_context::<SearchPaletteHandle>(), shortcut);
    let palette = use_palette_state("palette");
    let open = palette.open;

    let data = SearchData {
        scope: RwSignal::new(Scope::All),
        posts: RwSignal::new(Vec::new()),
        posts_query: RwSignal::new(String::new()),
        tags: RwSignal::new(None),
        recents: RwSignal::new(None),
    };
    let theme = use_theme();
    let resolved = use_resolved_theme();

    // 打开：读最近搜索、首次加载标签。
    Effect::new(move |_| {
        if !open.get() {
            return;
        }
        data.recents.set(read_recents());
        if data.tags.read_untracked().is_none() {
            leptos::task::spawn_local(async move {
                if let Ok(res) = list_tags().await {
                    let _ = data.tags.try_set(Some(
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

    view! {
        <Suspense fallback=|| ()>
            {move || {
                palette
                    .visible()
                    .then(|| {
                        Suspend::new(async move {
                            search_palette_body(palette, data, theme, resolved).await
                        })
                    })
            }}
        </Suspense>
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
            assert_eq!(page_href(path), *path, "{path}");
        }
    }
}
