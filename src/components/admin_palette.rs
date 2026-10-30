//! 后台命令面板（⌘K）：搜索文章、执行操作、跳转栏目。
//!
//! 输入、防抖请求、键盘选择与对话框结构由 [crate::components::palette::PaletteShell] 提供；数据与
//! 动作是后台自己的。空查询显示常用操作与全部栏目；有查询时按「文章 · 操作 · 跳转」
//! 分组，文章来自后台列表接口（含草稿，按标题匹配，最多 6 篇），回车进入编辑。

use std::future::Future;
use std::pin::Pin;

use leptos::prelude::*;

use crate::components::admin_layout::AdminSection;
use crate::components::palette::{
    contains_ci, use_palette_shortcut, use_palette_state, viewport_center, PaletteEntry,
    PaletteGroup, PaletteShell, SearchPaletteHandle,
};
use crate::components::ui::IconName;
use crate::models::post::{PostListItem, PostStatus};
use crate::routes::Route;
use crate::theme::{choose_theme, use_resolved_theme, use_theme, ResolvedTheme, Theme};

/// 文章搜索结果的条数上限。
const POST_LIMIT: i32 = 6;

/// 面板里的动作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
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
    targets.push(("回收站", IconName::Trash, Route::PostsTrash));
    targets.push(("笔记本", IconName::BookOpen, Route::AdminNotebooks));
    targets.push(("个人资料", IconName::User, Route::Profile));
    targets
}

/// 文章检索结果；`None` 表示请求失败，面板保留上一批结果。
pub type PostSearch = Pin<Box<dyn Future<Output = Option<Vec<PostListItem>>>>>;

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
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

impl Item {
    pub fn title(&self) -> String {
        match self {
            Item::Post(post) => post.title.clone(),
            Item::Action(action) => action.label().to_string(),
            Item::Jump { label, .. } => label.to_string(),
        }
    }
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

/// 后台命令面板。挂在后台布局里一次，由 [`crate::components::palette::SearchPaletteHandle`] 打开。
///
/// Props：
/// - `on_logout`：「退出登录」命令，与侧栏退出按钮同一处理
#[component]
pub fn AdminPalette(#[prop(into)] on_logout: Callback<()>) -> impl IntoView {
    use_palette_shortcut(expect_context::<SearchPaletteHandle>(), true);
    let theme = use_theme();
    let search = Callback::new(|q: String| -> PostSearch {
        Box::pin(async move {
            crate::api::posts::list_posts(
                1,
                POST_LIMIT,
                crate::api::posts::AdminPostQuery::search(q),
            )
            .await
            .ok()
            .map(|res| res.posts)
        })
    });
    let on_activate = Callback::new(move |item: Item| match item {
        Item::Post(post) => {
            crate::navigation::push(&Route::WriteEdit { id: post.id }.to_string());
        }
        Item::Jump { route, .. } => crate::navigation::push(&route.to_string()),
        Item::Action(action) => match action {
            Action::WritePost => crate::navigation::push(&Route::Write.to_string()),
            Action::NewNote => crate::navigation::push(&Route::NewNote.to_string()),
            Action::Upload => crate::navigation::push(&Route::Assets.to_string()),
            Action::OpenSite => crate::navigation::push(&Route::Home.to_string()),
            Action::Theme(to) => choose_theme(theme, to, viewport_center()),
            Action::Logout => on_logout.run(()),
        },
    });
    view! { <AdminPaletteView search=search on_activate=on_activate/> }
}

/// 面板中的一行的展示内容。
fn entry(item: &Item) -> PaletteEntry {
    let (icon, title, meta) = match item {
        Item::Post(post) => (
            IconName::FileText,
            post.title.clone(),
            match post.status {
                PostStatus::Draft => "草稿".to_string(),
                PostStatus::Published => post
                    .published_at
                    .map(|d| crate::utils::time::ymd(&d, '.'))
                    .unwrap_or_default(),
            },
        ),
        Item::Action(action) => (action.icon(), action.label().to_string(), String::new()),
        Item::Jump { label, icon, .. } => (*icon, label.to_string(), "跳转".to_string()),
    };
    PaletteEntry {
        icon,
        title,
        summary: None,
        meta,
        mono_meta: false,
        highlight_hits: true,
    }
}

/// 命令面板视图：文章检索与条目执行由调用方提供，其余交给 [`PaletteShell`]。
///
/// Props：
/// - `search`：按标题检索文章，返回 `None` 表示失败（面板保留上一批结果）
/// - `on_activate`：选中条目；面板先关闭再回调
#[component]
pub(crate) fn AdminPaletteView(
    #[prop(into)] search: Callback<String, PostSearch>,
    #[prop(into)] on_activate: Callback<Item>,
) -> impl IntoView {
    let palette = use_palette_state("admin-palette");
    let query = palette.query;

    let posts = RwSignal::new(Vec::<PostListItem>::new());
    let posts_query = RwSignal::new(String::new());
    let resolved = use_resolved_theme();
    // 只在「是否在屏上」翻转时重建面板：关闭淡出期间 `open` 与 `closing` 先后变化，
    // 直接读 `palette.visible()` 会让面板在淡出开始时被重建，动画丢失。
    let visible = Memo::new(move |_| palette.visible());

    let close = move || {
        palette.close(move || {
            let _ = posts.try_set(Vec::new());
            let _ = posts_query.try_set(String::new());
        });
    };

    // 检索返回 `None` 时保留上一批结果。
    let schedule_fetch = move || palette.schedule_fetch(posts, posts_query, move |q| search.run(q));

    let groups = Memo::new(move |_| {
        let q = query.get();
        let dark = resolved.get() == ResolvedTheme::Dark;
        posts.with(|posts| build_groups(&q, posts, dark))
    });
    let view_groups = Signal::derive(move || {
        groups.with(|groups| {
            groups
                .iter()
                .map(|g| PaletteGroup {
                    label: g.label,
                    entries: g.items.iter().map(entry).collect(),
                })
                .collect::<Vec<_>>()
        })
    });
    let waiting = Signal::derive(move || posts_query.read().as_str() != query.read().trim());

    move || {
        visible.get().then(|| {
            view! {
                <PaletteShell
                    state=palette
                    label="命令面板"
                    input_label="搜索文章或输入命令"
                    placeholder="搜索文章，或输入命令…"
                    list_label="命令与搜索结果"
                    enter_hint="执行"
                    empty_hint="文章标题、命令与栏目里都没有命中。"
                    groups=view_groups
                    waiting=waiting
                    on_query_change=move |_| schedule_fetch()
                    on_pick=move |(index, _): (usize, bool)| {
                        let item = groups
                            .with_untracked(|groups| {
                                groups.iter().flat_map(|g| g.items.iter()).nth(index).cloned()
                            });
                        if let Some(item) = item {
                            close();
                            on_activate.run(item);
                        }
                    }
                    on_close=move |_| close()
                />
            }
        })
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
