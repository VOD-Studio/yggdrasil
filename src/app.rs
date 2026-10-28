//! HTML 外壳、应用根组件与路由表。
//!
//! `shell()` 是服务端渲染的完整 HTML 文档；全局样式与启动脚本的地址由
//! [`crate::static_assets::url`] 按文件内容生成版本号，改文件不需要手动改地址。
//!
//! 路由表里每个页面都是 `Lazy::<T>`：路由数据（`LazyRoute::data`）在主包里同步创建资源，
//! 视图代码在 `cargo leptos --split` 下拆进独立 WASM 分包。前台布局（页头、页脚、主题）
//! 留在主包；后台外壳是懒加载的父路由，子页面切换时外壳不重建。

use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Title};
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::{path, Lazy, SsrMode};

use crate::components::frontend_layout::FrontendLayout;
use crate::components::share_meta::provide_share_context;
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
use crate::theme::provide_theme;

/// 服务端渲染的 HTML 文档外壳。
#[cfg(feature = "server")]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    use crate::static_assets::{self, CORE_CSS, CORE_JS, HIGHLIGHT_CSS, KATEX_CSS, STYLE_CSS};
    use crate::theme::ThemePreload;
    use leptos_meta::MetaTags;

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
                <link rel="stylesheet" href=static_assets::url(STYLE_CSS)/>
                <link rel="stylesheet" href=static_assets::url(HIGHLIGHT_CSS)/>
                <link rel="stylesheet" href=static_assets::url(KATEX_CSS)/>
                <link rel="stylesheet" href=static_assets::url(CORE_CSS)/>
                <script
                    type="application/json"
                    id=static_assets::VERSIONS_ELEMENT_ID
                    inner_html=static_assets::versions_json()
                ></script>
                <script src=static_assets::url(CORE_JS)></script>
                <ThemePreload/>
                <AutoReload options=options.clone()/>
                <HydrationScriptsWithPreload options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

/// 水合脚本，只为当前路由预载 WASM 分包。
///
/// `HydrationScripts` 把分包清单放进上下文，`leptos_integration_utils` 据此为 SSR 期间调用过的
/// 每个 lazy fn 展开清单里的全部分包 `<link rel="preload" as="fetch">`，且不去重（首页 360 余条，
/// 含水合用不到的取数函数分包）。实测弱网下它们与 CSS 抢带宽，移动端 FCP 慢 0.5–2.5 s（见
/// `docs/首屏性能实测-2026-10-28.md`）。这里在 `HydrationScripts` 之后立即取走清单，改为按请求
/// 路径只预载路由视图分包（[`crate::route_prefetch::route_view_names`]，去重）。不预载时它们要等
/// 主包实例化、水合开始后才发出，整批多等一轮；EdgeOne 对同一客户端的并发请求大致逐个处理，
/// 这一轮在线上要 100–200 ms。加载器 `__wasm_split.*.js` 在主入口的静态导入链上，单独
/// modulepreload。
#[cfg(feature = "server")]
#[component]
fn HydrationScriptsWithPreload(options: LeptosOptions) -> impl IntoView {
    let scripts = view! { <HydrationScripts options/> };
    let names = crate::request_ctx::request_parts()
        .and_then(|parts| crate::route_prefetch::match_route(parts.uri.path()))
        .map(|target| crate::route_prefetch::route_view_names(&target))
        .unwrap_or_default();
    let (loader, modules) = leptos::reactive::owner::take_context::<leptos::WasmSplitManifest>()
        .and_then(|manifest| {
            manifest.0.try_read_value().map(|value| {
                let (pkg, manifest, loader) = &*value;
                (
                    format!("{pkg}/{loader}"),
                    route_modules(manifest, names)
                        .into_iter()
                        .map(|module| format!("{pkg}/{module}.wasm"))
                        .collect::<Vec<_>>(),
                )
            })
        })
        .unzip();
    view! {
        {scripts}
        {loader.map(|href| view! { <link rel="modulepreload" href=href/> })}
        {modules
            .unwrap_or_default()
            .into_iter()
            .map(|href| {
                view! {
                    <link
                        rel="preload"
                        href=href
                        r#as="fetch"
                        r#type="application/wasm"
                        crossorigin="anonymous"
                        fetchpriority="low"
                    />
                }
            })
            .collect_view()}
    }
}

/// 清单里这些路由视图（键形如 `__<name>_route_view_<哈希>`）依赖的全部模块，去重后按首次出现排序。
#[cfg(feature = "server")]
fn route_modules<'a>(
    manifest: &'a std::collections::HashMap<String, Vec<String>>,
    names: &[&str],
) -> Vec<&'a str> {
    let mut modules: Vec<&str> = Vec::new();
    for name in names {
        let prefix = format!("__{name}_route_view_");
        let Some((_, list)) = manifest.iter().find(|(key, _)| {
            key.strip_prefix(&prefix)
                .is_some_and(|hash| hash.bytes().all(|b| b.is_ascii_digit()))
        }) else {
            continue;
        };
        for module in list {
            if !modules.contains(&module.as_str()) {
                modules.push(module);
            }
        }
    }
    modules
}

/// 应用根组件：全局上下文、路由器、导航桥接与全站提示。
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    provide_share_context();
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
                        // 非法页码要回 404：前台页面都在 FrontendLayout 的 Suspense 里，InOrder 下
                        // 状态码能否赶上首块取决于线程调度，只有 Async 等全部渲染完才写状态。
                        <Route
                            path=path!("page/:page")
                            view={Lazy::<HomeRoute>::new()}
                            ssr=SsrMode::Async
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

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::route_modules;

    #[test]
    fn route_modules_match_whole_route_name_and_dedupe() {
        let manifest = [
            (
                "__note_route_view_11",
                vec!["chunk_1.a", "split___note_route_view_11.x"],
            ),
            ("__notebook_route_view_12", vec!["chunk_2.a"]),
            ("__friends_admin_route_view_13", vec!["chunk_3.a"]),
            (
                "__admin_shell_route_view_14",
                vec!["chunk_1.a", "chunk_4.a"],
            ),
            ("__dashboard_route_view_15", vec!["chunk_4.a", "chunk_5.a"]),
            ("fetch_note_16", vec!["chunk_6.a"]),
        ]
        .into_iter()
        .map(|(key, modules)| {
            (
                key.to_string(),
                modules.into_iter().map(String::from).collect(),
            )
        })
        .collect();

        // `note` 不能带出 `notebook` 的分包，取数函数的分包也不预载。
        assert_eq!(
            route_modules(&manifest, &["note"]),
            ["chunk_1.a", "split___note_route_view_11.x"]
        );
        // 后台外壳与子页面共用的分包只出现一次。
        assert_eq!(
            route_modules(&manifest, &["admin_shell", "dashboard"]),
            ["chunk_1.a", "chunk_4.a", "chunk_5.a"]
        );
        // 清单里没有的路由（`friends` 只有后台同名前缀）不预载任何东西。
        assert!(route_modules(&manifest, &["friends"]).is_empty());
    }
}
