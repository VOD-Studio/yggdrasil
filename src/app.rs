//! HTML 外壳、应用根组件与路由表。
//!
//! `shell()` 是服务端渲染的完整 HTML 文档；全局样式与启动脚本的资源版本号
//! （`?v=N`）在这里维护，修改对应静态文件后必须同步递增，浏览器才会重新拉取。
//!
//! 路由表里每个页面都是 `Lazy::<T>`：路由数据（`LazyRoute::data`）在主包里同步创建资源，
//! 视图代码在 `cargo leptos --split` 下拆进独立 WASM 分包。前台布局（页头、页脚、主题）
//! 留在主包；后台外壳是懒加载的父路由，子页面切换时外壳不重建。

use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Title};
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::{path, Lazy, SsrMode};

use crate::components::frontend_layout::FrontendLayout;
use crate::components::ui::{provide_toaster, ToastRegion};
use crate::context::{ShowcaseFilters, UserContext};
use crate::navigation::{NavigationBridge, RouteMarker, RouteProgress};
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
use crate::pages::not_found::NotFound;
use crate::pages::notes::{NoteRoute, NotebookRoute, NotesRoute};
use crate::pages::post_detail::PostRoute;
use crate::pages::register::RegisterRoute;
use crate::pages::search::SearchRoute;
use crate::pages::tags::TagRoute;
use crate::pages::writing_guide::WritingGuideRoute;
use crate::theme::{provide_theme, ThemePreload};

/// 服务端渲染的 HTML 文档外壳。
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="theme-color" content="#f5f5f3" media="(prefers-color-scheme: light)"/>
                <meta name="theme-color" content="#0f0f0e" media="(prefers-color-scheme: dark)"/>
                <link rel="icon" href="/favicon.svg" type="image/svg+xml"/>
                <link rel="icon" href="/favicon.ico" sizes="16x16 32x32 48x48"/>
                <link rel="apple-touch-icon" href="/apple-touch-icon.png"/>
                <link rel="manifest" href="/manifest.webmanifest"/>
                <link rel="stylesheet" href="/style.css?v=6"/>
                <link rel="stylesheet" href="/highlight.css?v=2"/>
                <link rel="stylesheet" href="/katex/katex.min.css?v=3"/>
                <link rel="stylesheet" href="/yggdrasil-core/yggdrasil-core.css?v=4"/>
                <script src="/yggdrasil-core/yggdrasil-core.js?v=8"></script>
                <ThemePreload/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

/// 应用根组件：全局上下文、路由器、导航桥接与全站提示。
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    provide_theme();
    provide_context(UserContext::new());
    provide_context(RwSignal::new(ShowcaseFilters::default()));
    provide_toaster();
    // 路由器是否正在切换：RouteMarker 据此避开切换中的无效通知，RouteProgress 据此显示进度条。
    let (is_routing, set_is_routing) = signal(false);

    // 悬停预取的路由匹配表在 `route_prefetch::match_route`，改路由表时同步修改。
    view! {
        <Title text="Yggdrasil Blog"/>
        <Router set_is_routing>
            <NavigationBridge/>
            <RouteMarker is_routing>
                <Routes fallback=|| {
                    view! {
                        <FrontendLayout>
                            <NotFound/>
                        </FrontendLayout>
                    }
                }>
                    <ParentRoute path=path!("") view=|| view! { <FrontendLayout/> }>
                        <Route
                            path=path!("")
                            view={Lazy::<HomeRoute>::new()}
                            ssr=SsrMode::InOrder
                        />
                        <Route
                            path=path!("page/:page")
                            view={Lazy::<HomeRoute>::new()}
                            ssr=SsrMode::InOrder
                        />
                        <Route
                            path=path!("notes")
                            view={Lazy::<NotesRoute>::new()}
                            ssr=SsrMode::InOrder
                        />
                        <Route
                            path=path!("notes/book/:id")
                            view={Lazy::<NotebookRoute>::new()}
                            ssr=SsrMode::Async
                        />
                        <Route
                            path=path!("notes/:slug")
                            view={Lazy::<NoteRoute>::new()}
                            ssr=SsrMode::Async
                        />
                        <Route
                            path=path!("archives")
                            view={Lazy::<ArchivesRoute>::new()}
                            ssr=SsrMode::InOrder
                        />
                        <Route
                            path=path!("tags/:tag")
                            view={Lazy::<TagRoute>::new()}
                            ssr=SsrMode::Async
                        />
                        <Route
                            path=path!("post/:slug")
                            view={Lazy::<PostRoute>::new()}
                            ssr=SsrMode::Async
                        />
                        <Route path=path!("search") view={Lazy::<SearchRoute>::new()}/>
                        <Route
                            path=path!("about")
                            view={Lazy::<AboutRoute>::new()}
                            ssr=SsrMode::InOrder
                        />
                        <Route
                            path=path!("about/writing")
                            view={Lazy::<WritingGuideRoute>::new()}
                            ssr=SsrMode::InOrder
                        />
                        <Route
                            path=path!("about/components")
                            view={Lazy::<ShowcaseRoute>::new()}
                            ssr=SsrMode::Async
                        />
                        <Route
                            path=path!("about/components/:component")
                            view={Lazy::<ShowcaseDetailRoute>::new()}
                            ssr=SsrMode::Async
                        />
                        <Route
                            path=path!("friends")
                            view={Lazy::<FriendsRoute>::new()}
                            ssr=SsrMode::InOrder
                        />
                        <Route
                            path=path!("changelog")
                            view={Lazy::<ChangelogRoute>::new()}
                            ssr=SsrMode::Async
                        />
                    </ParentRoute>
                    <ParentRoute path=path!("admin") view={Lazy::<AdminShellRoute>::new()}>
                        <Route path=path!("") view={Lazy::<DashboardRoute>::new()}/>
                        <Route path=path!("write") view={Lazy::<WriteRoute>::new()}/>
                        <Route path=path!("write/:id") view={Lazy::<WriteEditRoute>::new()}/>
                        <Route path=path!("preview/:slug") view={Lazy::<PreviewRoute>::new()}/>
                        <Route path=path!("posts") view={Lazy::<PostsRoute>::new()}/>
                        <Route path=path!("posts/trash") view={Lazy::<PostsTrashRoute>::new()}/>
                        <Route path=path!("notes") view={Lazy::<AdminNotesRoute>::new()}/>
                        <Route path=path!("notes/new") view={Lazy::<NewNoteRoute>::new()}/>
                        <Route path=path!("notes/edit/:id") view={Lazy::<EditNoteRoute>::new()}/>
                        <Route path=path!("notebooks") view={Lazy::<NotebooksRoute>::new()}/>
                        <Route path=path!("assets") view={Lazy::<AssetsRoute>::new()}/>
                        <Route path=path!("friends") view={Lazy::<FriendsAdminRoute>::new()}/>
                        <Route path=path!("comments") view={Lazy::<AdminCommentsRoute>::new()}/>
                        <Route
                            path=path!("comments/:page")
                            view={Lazy::<AdminCommentsRoute>::new()}
                        />
                        <Route path=path!("system") view={Lazy::<SystemRoute>::new()}/>
                        <Route path=path!("runner") view={Lazy::<RunnerRoute>::new()}/>
                        <Route path=path!("mcp") view={Lazy::<McpRoute>::new()}/>
                        <Route path=path!("logs") view={Lazy::<LogsRoute>::new()}/>
                        <Route path=path!("settings") view={Lazy::<SettingsRoute>::new()}/>
                        <Route path=path!("profile") view={Lazy::<ProfileRoute>::new()}/>
                    </ParentRoute>
                    <Route path=path!("login") view={Lazy::<LoginRoute>::new()}/>
                    <Route path=path!("register") view={Lazy::<RegisterRoute>::new()}/>
                </Routes>
            </RouteMarker>
        </Router>
        <RouteProgress is_routing/>
        <ToastRegion/>
    }
}
