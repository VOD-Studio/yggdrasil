//! 链接悬停预取：把站内地址映射到路由，提前下载目标页面的 WASM 分包。
//!
//! JS 协调器（`libs/yggdrasil-core/src/route-transitions.ts`）在鼠标停留、键盘聚焦或触屏
//! 按下站内链接时，把“路径 + 查询”（无前缀、无哈希）交给 [`prefetch`]。
//!
//! [`match_route`] 的匹配表与 `src/app.rs` 的路由表一一对应，**新增路由时两处一起改**；
//! 漏改只会少一次预取，不影响导航。

use leptos_router::location::Url;

/// 悬停目标。参数是路由器会看到的值：路径段经 `Url::unescape` 解码，查询值同样解码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteTarget {
    /// `/`（`None`）或 `/page/:page`（原始段，由 `HomeRoute` 自己解析）。
    Home {
        page: Option<String>,
    },
    Notes,
    Notebook {
        id: String,
    },
    /// `/notes/:slug`，`book` 来自查询串 `?book=`。
    Note {
        slug: String,
        book: Option<String>,
    },
    Archives,
    Tag {
        tag: String,
    },
    Post {
        slug: String,
    },
    Search,
    About,
    WritingGuide,
    Showcase,
    ShowcaseDetail,
    Friends,
    Changelog,
    Login,
    Register,
    Admin(AdminPage),
}

/// `/admin/*` 子页面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminPage {
    Dashboard,
    Write,
    WriteEdit,
    Preview,
    Posts,
    PostsTrash,
    Notes,
    NewNote,
    EditNote,
    Notebooks,
    Assets,
    Friends,
    Comments,
    System,
    Runner,
    Mcp,
    Logs,
    Settings,
    Profile,
}

/// 查询串里第一个名为 `key` 的参数值（键、值都解码）。
fn query_value(search: &str, key: &str) -> Option<String> {
    search.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (Url::unescape(k) == key).then(|| Url::unescape(v))
    })
}

/// `route` 为“路径 + 可选查询”（无前缀、无哈希）。不匹配任何页面返回 `None`。
pub fn match_route(route: &str) -> Option<RouteTarget> {
    let (path, search) = route.split_once('?').unwrap_or((route, ""));
    let rest = path.strip_prefix('/')?;
    let segments: Vec<&str> = if rest.is_empty() {
        Vec::new()
    } else {
        rest.split('/').collect()
    };
    // 任何空段（尾斜杠、`//`）都不匹配，与路由器一致。
    if segments.iter().any(|s| s.is_empty()) {
        return None;
    }
    let param = |s: &str| Url::unescape(s);
    Some(match segments.as_slice() {
        [] => RouteTarget::Home { page: None },
        ["page", page] => RouteTarget::Home {
            page: Some(param(page)),
        },
        ["notes"] => RouteTarget::Notes,
        ["notes", "book", id] => RouteTarget::Notebook { id: param(id) },
        ["notes", slug] => RouteTarget::Note {
            slug: param(slug),
            book: query_value(search, "book"),
        },
        ["archives"] => RouteTarget::Archives,
        ["tags", tag] => RouteTarget::Tag { tag: param(tag) },
        ["post", slug] => RouteTarget::Post { slug: param(slug) },
        ["search"] => RouteTarget::Search,
        ["about"] => RouteTarget::About,
        ["about", "writing"] => RouteTarget::WritingGuide,
        ["about", "components"] => RouteTarget::Showcase,
        ["about", "components", _] => RouteTarget::ShowcaseDetail,
        ["friends"] => RouteTarget::Friends,
        ["changelog"] => RouteTarget::Changelog,
        ["login"] => RouteTarget::Login,
        ["register"] => RouteTarget::Register,
        ["admin", rest @ ..] => RouteTarget::Admin(match rest {
            [] => AdminPage::Dashboard,
            ["write"] => AdminPage::Write,
            ["write", _] => AdminPage::WriteEdit,
            ["preview", _] => AdminPage::Preview,
            ["posts"] => AdminPage::Posts,
            ["posts", "trash"] => AdminPage::PostsTrash,
            ["notes"] => AdminPage::Notes,
            ["notes", "new"] => AdminPage::NewNote,
            ["notes", "edit", _] => AdminPage::EditNote,
            ["notebooks"] => AdminPage::Notebooks,
            ["assets"] => AdminPage::Assets,
            ["friends"] => AdminPage::Friends,
            ["comments"] | ["comments", _] => AdminPage::Comments,
            ["system"] => AdminPage::System,
            ["runner"] => AdminPage::Runner,
            ["mcp"] => AdminPage::Mcp,
            ["logs"] => AdminPage::Logs,
            ["settings"] => AdminPage::Settings,
            ["profile"] => AdminPage::Profile,
            _ => return None,
        }),
        _ => return None,
    })
}

/// 预取目标路由：总是预取视图分包；`with_data` 时再预取页面数据（第 2 步接入）。
/// `is_admin` 为假时跳过 `/admin*`：访客悬停后台链接不下载后台分包。
pub fn prefetch(route: &str, with_data: bool, is_admin: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = with_data;
        if let Some(target) = match_route(route) {
            wasm::preload(target, is_admin);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (route, with_data, is_admin);
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::{AdminPage, RouteTarget};
    use crate::pages::about::AboutRoute;
    use crate::pages::admin::assets::AssetsRoute;
    use crate::pages::admin::comments::AdminCommentsRoute;
    use crate::pages::admin::dashboard::DashboardRoute;
    use crate::pages::admin::friends::FriendsAdminRoute;
    use crate::pages::admin::logs::LogsRoute;
    use crate::pages::admin::mcp::McpRoute;
    use crate::pages::admin::note_editor::{EditNoteRoute, NewNoteRoute};
    use crate::pages::admin::notes::notebooks::NotebooksRoute;
    use crate::pages::admin::notes::AdminNotesRoute;
    use crate::pages::admin::posts::PostsRoute;
    use crate::pages::admin::posts_trash::PostsTrashRoute;
    use crate::pages::admin::preview::PreviewRoute;
    use crate::pages::admin::profile::ProfileRoute;
    use crate::pages::admin::runner::RunnerRoute;
    use crate::pages::admin::settings::SettingsRoute;
    use crate::pages::admin::system::SystemRoute;
    use crate::pages::admin::write::{WriteEditRoute, WriteRoute};
    use crate::pages::admin::AdminShellRoute;
    use crate::pages::archives::ArchivesRoute;
    use crate::pages::changelog::ChangelogRoute;
    use crate::pages::components_showcase::{ShowcaseDetailRoute, ShowcaseRoute};
    use crate::pages::friends::FriendsRoute;
    use crate::pages::home::HomeRoute;
    use crate::pages::login::LoginRoute;
    use crate::pages::notes::{NoteRoute, NotebookRoute, NotesRoute};
    use crate::pages::post_detail::PostRoute;
    use crate::pages::register::RegisterRoute;
    use crate::pages::search::SearchRoute;
    use crate::pages::tags::TagRoute;
    use crate::pages::writing_guide::WritingGuideRoute;
    use leptos::task::spawn_local;
    use leptos_router::LazyRoute;

    /// 预取视图分包。`preload()` 失败不缓存，下次悬停或点击仍会重试。
    pub(super) fn preload(target: RouteTarget, is_admin: bool) {
        match target {
            RouteTarget::Home { .. } => spawn_local(HomeRoute::preload()),
            RouteTarget::Notes => spawn_local(NotesRoute::preload()),
            RouteTarget::Notebook { .. } => spawn_local(NotebookRoute::preload()),
            RouteTarget::Note { .. } => spawn_local(NoteRoute::preload()),
            RouteTarget::Archives => spawn_local(ArchivesRoute::preload()),
            RouteTarget::Tag { .. } => spawn_local(TagRoute::preload()),
            RouteTarget::Post { .. } => spawn_local(PostRoute::preload()),
            RouteTarget::Search => spawn_local(SearchRoute::preload()),
            RouteTarget::About => spawn_local(AboutRoute::preload()),
            RouteTarget::WritingGuide => spawn_local(WritingGuideRoute::preload()),
            RouteTarget::Showcase => spawn_local(ShowcaseRoute::preload()),
            RouteTarget::ShowcaseDetail => spawn_local(ShowcaseDetailRoute::preload()),
            RouteTarget::Friends => spawn_local(FriendsRoute::preload()),
            RouteTarget::Changelog => spawn_local(ChangelogRoute::preload()),
            RouteTarget::Login => spawn_local(LoginRoute::preload()),
            RouteTarget::Register => spawn_local(RegisterRoute::preload()),
            RouteTarget::Admin(page) if is_admin => {
                spawn_local(AdminShellRoute::preload());
                match page {
                    AdminPage::Dashboard => spawn_local(DashboardRoute::preload()),
                    AdminPage::Write => spawn_local(WriteRoute::preload()),
                    AdminPage::WriteEdit => spawn_local(WriteEditRoute::preload()),
                    AdminPage::Preview => spawn_local(PreviewRoute::preload()),
                    AdminPage::Posts => spawn_local(PostsRoute::preload()),
                    AdminPage::PostsTrash => spawn_local(PostsTrashRoute::preload()),
                    AdminPage::Notes => spawn_local(AdminNotesRoute::preload()),
                    AdminPage::NewNote => spawn_local(NewNoteRoute::preload()),
                    AdminPage::EditNote => spawn_local(EditNoteRoute::preload()),
                    AdminPage::Notebooks => spawn_local(NotebooksRoute::preload()),
                    AdminPage::Assets => spawn_local(AssetsRoute::preload()),
                    AdminPage::Friends => spawn_local(FriendsAdminRoute::preload()),
                    AdminPage::Comments => spawn_local(AdminCommentsRoute::preload()),
                    AdminPage::System => spawn_local(SystemRoute::preload()),
                    AdminPage::Runner => spawn_local(RunnerRoute::preload()),
                    AdminPage::Mcp => spawn_local(McpRoute::preload()),
                    AdminPage::Logs => spawn_local(LogsRoute::preload()),
                    AdminPage::Settings => spawn_local(SettingsRoute::preload()),
                    AdminPage::Profile => spawn_local(ProfileRoute::preload()),
                }
            }
            RouteTarget::Admin(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(slug: &str) -> Option<RouteTarget> {
        Some(RouteTarget::Post { slug: slug.into() })
    }

    #[test]
    fn notebook_needs_three_segments() {
        assert_eq!(
            match_route("/notes/book/3"),
            Some(RouteTarget::Notebook { id: "3".into() })
        );
        assert_eq!(
            match_route("/notes/book"),
            Some(RouteTarget::Note {
                slug: "book".into(),
                book: None
            })
        );
    }

    #[test]
    fn params_are_percent_decoded() {
        assert_eq!(
            match_route("/tags/%E4%B8%AD%E6%96%87"),
            Some(RouteTarget::Tag {
                tag: "中文".into()
            })
        );
        assert_eq!(match_route("/post/a%20b"), post("a b"));
    }

    #[test]
    fn note_takes_the_first_book_query() {
        assert_eq!(
            match_route("/notes/x?book=7&book=8"),
            Some(RouteTarget::Note {
                slug: "x".into(),
                book: Some("7".into())
            })
        );
        assert_eq!(
            match_route("/notes/x?other=1"),
            Some(RouteTarget::Note {
                slug: "x".into(),
                book: None
            })
        );
    }

    #[test]
    fn home_pages() {
        assert_eq!(match_route("/"), Some(RouteTarget::Home { page: None }));
        assert_eq!(
            match_route("/page/2"),
            Some(RouteTarget::Home {
                page: Some("2".into())
            })
        );
    }

    #[test]
    fn admin_pages() {
        assert_eq!(
            match_route("/admin"),
            Some(RouteTarget::Admin(AdminPage::Dashboard))
        );
        assert_eq!(
            match_route("/admin/comments/2"),
            Some(RouteTarget::Admin(AdminPage::Comments))
        );
        assert_eq!(
            match_route("/admin/write/5"),
            Some(RouteTarget::Admin(AdminPage::WriteEdit))
        );
        assert_eq!(
            match_route("/admin/notes/edit/9"),
            Some(RouteTarget::Admin(AdminPage::EditNote))
        );
        assert_eq!(match_route("/admin/unknown"), None);
    }

    #[test]
    fn unmatched_routes() {
        for route in [
            "/post/",
            "/post",
            "/post/a/b",
            "//",
            "/unknown",
            "/api/x",
            "",
            "post/a",
            "/archives/",
        ] {
            assert_eq!(match_route(route), None, "{route:?}");
        }
    }
}
