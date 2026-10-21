//! 全站路由配置
//!
//! 使用 Dioxus Router 定义前端路由层级，包含前台布局、后台管理布局、
//! 独立的登录与注册页面。`Route` 枚举上的 `#[route("/path")]` 属性
//! 既用于生成 URL 匹配规则，也用于组件导航。

use dioxus::prelude::*;
use dioxus::router::components::HistoryProvider;
use std::sync::Arc;

// full 客户端专属页面组件：reader 构建（无 client-full）随变体一起剥离。
#[cfg(not(ygg_client_reader))]
use crate::components::admin_layout::AdminLayout;
use crate::components::frontend_layout::FrontendLayout;
use crate::context::UserContext;
use crate::pages::about::About;
#[cfg(not(ygg_client_reader))]
use crate::pages::admin::note_editor::{EditNote, NewNote};
#[cfg(not(ygg_client_reader))]
use crate::pages::admin::notes::{AdminNotebooks, AdminNotes};
#[cfg(not(ygg_client_reader))]
use crate::pages::admin::{
    Admin, AdminComments, AdminCommentsPage, Assets, FriendsAdmin, Logs, Mcp, PostPreview, Posts,
    PostsTrash, Profile, Runner, SiteSettingsPage, System, Write, WriteEdit,
};
use crate::pages::archives::Archives;
use crate::pages::changelog::Changelog;
#[cfg(not(ygg_client_reader))]
use crate::pages::components_showcase::{ComponentDetail, ComponentShowcase};
use crate::pages::friends::Friends;
use crate::pages::home::{Home, HomePage};
#[cfg(not(ygg_client_reader))]
use crate::pages::login::Login;
use crate::pages::not_found::NotFound;
use crate::pages::notes::{NoteDetail, NotebookDetail, Notes};
use crate::pages::post_detail::PostDetail;
#[cfg(not(ygg_client_reader))]
use crate::pages::register::Register;
use crate::pages::search::Search;
use crate::pages::tags::TagDetail;
use crate::pages::writing_guide::WritingGuide;
use crate::theme::{use_theme_provider, ThemePreload};

/// 全站路由枚举，每个变体对应一个页面路径
#[derive(Clone, Routable, Debug, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    // 不挂起的全局布局负责通知实际 DOM 提交，覆盖前台、后台及认证页面。
    #[layout(NavigationLayout)]
    // 前台页面共享布局，嵌套错误边界布局以拦截报错
    #[layout(ErrorLayout)]
        #[layout(FrontendLayout)]
            /// 首页
            #[route("/")]
            Home {},
            /// 首页分页
            #[route("/page/:page")]
            HomePage { page: i32 },
            #[route("/notes")]
            Notes {},
            #[route("/notes/book/:id")]
            NotebookDetail { id: i32 },
            #[route("/notes/:slug")]
            NoteDetail { slug: String },
            /// 文章归档与标签索引；兼容旧标签列表地址。
            #[redirect("/tags", || Route::Archives {})]
            #[route("/archives")]
            Archives {},
            /// 单个标签下的文章列表
            #[route("/tags/:tag")]
            TagDetail { tag: String },
            /// 文章详情页，按 slug 匹配
            #[route("/post/:slug")]
            PostDetail { slug: String },
            /// 搜索页
            #[route("/search")]
            Search {},
            /// 关于页面
            #[route("/about")]
            About {},
            /// Agent 写作技能与 MCP 接入指南。
            #[route("/about/writing")]
            WritingGuide {},
            // 组件图鉴：full 客户端专属（plan §6.1）。reader 构建剥离变体后
            // 该路由从枚举消失，/about/components 交由 reader 的 NotFound 兜底。
            #[cfg(not(ygg_client_reader))]
            /// 项目自写组件的公开图鉴。
            #[route("/about/components")]
            ComponentShowcase {},
            #[cfg(not(ygg_client_reader))]
            /// 单枚组件的交互示例与用法。
            #[route("/about/components/:component")]
            ComponentDetail { component: String },
            /// 友链页
            #[route("/friends")]
            Friends {},
            /// 更新日志页
            #[route("/changelog")]
            Changelog {},
            /// 404 兜底路由，匹配所有未命中路径
            #[route("/:..segments")]
            NotFound { segments: Vec<String> },
        #[end_layout]
    #[end_layout]

    // 后台管理路由：full 客户端专属（plan §6.1）。`#[cfg]` 在 Routable 派生
    // 展开前剥离整个嵌套块（含 nest/layout 属性，plan §6.2 门 1 实验），
    // reader 构建的枚举中不存在任何 admin 变体。
    #[cfg(not(ygg_client_reader))]
    // 后台管理路由嵌套在 `/admin` 下
    #[nest("/admin")]
    // 后台页面共享管理布局
    #[layout(AdminLayout)]
        /// 后台仪表盘
        #[route("/")]
        Admin {},
        /// 写文章页
        #[cfg(not(ygg_client_reader))]
        #[route("/write")]
        Write {},
        /// 编辑文章页
        #[cfg(not(ygg_client_reader))]
        #[route("/write/:id")]
        WriteEdit { id: i32 },
        /// 草稿/文章预览（管理员只读）
        #[cfg(not(ygg_client_reader))]
        #[route("/preview/:slug")]
        PostPreview { slug: String },
        /// 文章管理（全部文章列表）
        #[cfg(not(ygg_client_reader))]
        #[route("/posts")]
        Posts {},
        #[cfg(not(ygg_client_reader))]
        #[route("/notes")]
        AdminNotes {},
        #[cfg(not(ygg_client_reader))]
        #[route("/notes/new")]
        NewNote {},
        #[cfg(not(ygg_client_reader))]
        #[route("/notes/edit/:id")]
        EditNote { id: i32 },
        #[cfg(not(ygg_client_reader))]
        #[route("/notebooks")]
        AdminNotebooks {},
        /// 回收站（已软删除文章；原 /admin/posts 的客户端 tab，独立路由后归「内容管理」子菜单）
        #[cfg(not(ygg_client_reader))]
        #[route("/posts/trash")]
        PostsTrash {},
        /// 素材管理（上传图片注册表：浏览/搜索/删除/孤儿清理）
        #[cfg(not(ygg_client_reader))]
        #[route("/assets")]
        Assets {},
        /// 友链管理
        #[cfg(not(ygg_client_reader))]
        #[route("/friends")]
        FriendsAdmin {},
        /// 评论管理
        #[cfg(not(ygg_client_reader))]
        #[route("/comments")]
        AdminComments {},
        /// 评论管理分页
        #[cfg(not(ygg_client_reader))]
        #[route("/comments/:page")]
        AdminCommentsPage { page: i32 },
        /// 系统管理（数据库 + 服务器状态 + SQL 控制台 + 导出 + 备份）
        #[cfg(not(ygg_client_reader))]
        #[route("/system")]
        System {},
        /// 代码试运行沙箱（作者预览可运行代码块输出）
        #[cfg(not(ygg_client_reader))]
        #[route("/runner")]
        Runner {},
        /// MCP 令牌管理 + 客户端配置生成
        #[cfg(not(ygg_client_reader))]
        #[route("/mcp")]
        Mcp {},
        /// 运行日志（进程内日志查询 + SSE 实时流 + 导出）
        #[cfg(not(ygg_client_reader))]
        #[route("/logs")]
        Logs {},
        /// 站点配置（页脚 GitHub 链接等公开配置）
        #[cfg(not(ygg_client_reader))]
        #[route("/settings")]
        SiteSettingsPage {},
        /// 个人信息（当前账号的展示资料与密码）
        #[cfg(not(ygg_client_reader))]
        #[route("/profile")]
        Profile {},
    #[end_layout]
    #[end_nest]

    /// 登录页面（full 客户端专属；reader 经服务端 admin_guard 302 到 /login）
    #[cfg(not(ygg_client_reader))]
    #[route("/login")]
    Login {},
    /// 注册页面（full 客户端专属）
    #[cfg(not(ygg_client_reader))]
    #[route("/register")]
    Register {},
}

/// 应用路由器组件
///
/// 初始化主题提供者、全局用户上下文，并挂载样式表与 `Router`。
#[component]
pub fn AppRouter() -> Element {
    let _theme = use_theme_provider();

    // 提供全局用户上下文，供登录状态与路由守卫使用
    let user = use_signal(|| None::<Arc<crate::models::user::PublicUser>>);
    let checked = use_signal(|| false);
    use_context_provider(|| UserContext { user, checked });
    // 图鉴过滤器上下文：full 客户端专属。上下文不产生 DOM，服务端（完整编译）
    // 多注入的这一条对 SSR HTML 与 reader 客户端的 hydration 一致性无影响。
    #[cfg(not(ygg_client_reader))]
    let showcase_filters = use_signal(crate::pages::components_showcase::ShowcaseFilters::default);
    #[cfg(not(ygg_client_reader))]
    use_context_provider(|| showcase_filters);

    rsx! {
        document::Title { "Yggdrasil Blog" }
        document::Link { rel: "icon", href: "/favicon.ico" }
        div {
            ThemePreload {}
            HistoryProvider {
                history: move |_| crate::navigation::transition_history(dioxus::history::history()),
                Router::<Route> {}
            }
        }
    }
}

#[component]
fn NavigationLayout() -> Element {
    let route = dioxus::router::router().full_route_string();
    let navigation = crate::bridges::navigation::navigation_id();
    use_effect(use_reactive(
        (&route, &navigation),
        |(route, navigation)| {
            crate::bridges::navigation::rendered(navigation, &route);
        },
    ));

    rsx! {
        div {
            style: "display: contents",
            "data-vt-route": route,
            "data-vt-navigation": navigation,
            Outlet::<Route> {}
        }
    }
}

#[component]
fn ErrorLayout() -> Element {
    rsx! {
        ErrorBoundary {
            handle_error: move |err: ErrorContext| {
                // 克隆一份错误边界句柄，供 fallback 内的「返回首页」按钮清除错误。
                // ErrorContext 内部是 Rc<RefCell<...>>，clone 廉价。
                // 不清除就导航会卡死：ErrorBoundary 持有错误时永远渲染 fallback，
                // 不渲染 children（Outlet），导致 URL 变了但页面不变。
                let err_ctx = err.clone();
                // Commit the status code on the server side
                #[cfg(feature = "server")]
                {
                    if let Some(captured_error) = err.error() {
                        let _ = dioxus::fullstack::FullstackContext::commit_error_status(
                            captured_error,
                        );
                    }
                }
                let mut is_404 = false;
                if let Some(captured_error) = err.error() {
                    if let Some(ServerFnError::ServerError { code, message, .. }) = captured_error
                        .downcast_ref::<ServerFnError>()

                    {
                        if *code == 404 || message == "not_found" {
                            is_404 = true;
                        }
                    } else { // Warning / Alert Icon in a soft red container
                        let err_str = format!("{:?}", captured_error);
                        if err_str.contains("NotFound") || err_str.contains("404")
                            || err_str.contains("not found")
                        {
                            is_404 = true;
                        }
                    }
                }
                if is_404 {
                    rsx! {
                        NotFound { segments: vec![] }
                    }
                } else {
                    rsx! {
                        div { class: "flex flex-col items-center justify-center min-h-[50vh] md:min-h-[55vh] px-6 animate-page-enter",
                            div { class: "w-full max-w-md bg-paper-entry border border-paper-border rounded-[2rem] p-8 md:p-10 shadow-sm hover:border-paper-secondary transition-all duration-300 flex flex-col items-center text-center",
                                // Warning / Alert Icon in a soft red container
                                div { class: "relative mb-6",
                                    div { class: "w-16 h-16 bg-red-500/10 dark:bg-red-500/20 text-red-500 rounded-full flex items-center justify-center",
                                        svg {
                                            xmlns: "http://www.w3.org/2000/svg",
                                            width: "28",
                                            height: "28",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            circle { cx: "12", cy: "12", r: "10" }
                                            line {
                                                x1: "12",
                                                y1: "8",
                                                x2: "12",
                                                y2: "12",
                                            }
                                            line {
                                                x1: "12",
                                                y1: "16",
                                                x2: "12.01",
                                                y2: "16",
                                            }
                                        }
                                    }
                                }

                                // Premium Typography
                                h1 { class: "text-lg md:text-xl font-bold tracking-tight text-paper-primary mb-3",
                                    "加载失败"
                                }
                                p { class: "text-sm text-paper-secondary leading-relaxed mb-8 max-w-[280px]",
                                    "抱歉，加载页面时出现了一些错误，请稍后再试。"
                                }

                                // CTA: 清除错误边界后命令式导航回首页。
                                // 必须用按钮而非 Link —— Link 无法在导航前清除错误，
                                // 会导致 ErrorBoundary 卡在 fallback，页面不随路由切换更新。
                                button {
                                    r#type: "button",
                                    onclick: {
                                        let err_ctx = err_ctx.clone();
                                        move |_| {
                                            err_ctx.clear_errors();
                                            let _ = dioxus::router::navigator().push(Route::Home {});
                                        }
                                    },
                                    class: "group inline-flex items-center gap-2 px-6 py-2.5 text-sm font-medium text-paper-primary bg-paper-theme border border-paper-border rounded-full hover:border-paper-secondary hover:bg-paper-border transition-all duration-200 cursor-pointer shadow-sm active:scale-[0.98]",
                                    svg {
                                        xmlns: "http://www.w3.org/2000/svg",
                                        width: "16",
                                        height: "16",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        class: "transition-transform group-hover:-translate-x-0.5",
                                        path { d: "M19 12H5M12 19l-7-7 7-7" }
                                    }
                                    "返回首页"
                                }
                            }
                        }
                    }
                }
            },
            Outlet::<Route> {}
        }
    }
}

/// 当前路由是否属于组件图鉴（full 客户端专属路由，plan §6.1）。
///
/// 前台布局（宽版容器）与导航高亮用它区分图鉴路由；reader 构建没有图鉴
/// 变体，恒为 `false`。集中在此而不是散落 `cfg` 表达式，调用点保持单一形态。
#[cfg(not(ygg_client_reader))]
pub fn route_is_showcase(route: &Route) -> bool {
    matches!(
        route,
        Route::ComponentShowcase {} | Route::ComponentDetail { .. }
    )
}

#[cfg(ygg_client_reader)]
pub fn route_is_showcase(_route: &Route) -> bool {
    false
}

/// 服务端按路径选择客户端模板：该路径是否应使用 reader 客户端（plan §6.5
/// 「第一版按路径固定选择 flavor」）。
///
/// 用完整 `Route` 枚举解析路径后逐变体归类——服务端永远编译完整应用，
/// 分类结果由 [`route_is_reader`] 的穷尽 match 锁定：新增变体时服务端会
/// 编译失败，强制作者把新路由归类到 reader 或 full，两份清单不会手工漂移。
/// 解析失败（未知路径）落入 reader 的 NotFound 兜底，同样视为 reader。
#[cfg(feature = "server")]
pub fn is_reader_path(path: &str) -> bool {
    use std::str::FromStr;
    match Route::from_str(path) {
        Ok(route) => route_is_reader(&route),
        // 未匹配任何路由 → reader 客户端的 NotFound catch-all 渲染 404。
        Err(_) => true,
    }
}

/// reader 客户端保留的路由集合（plan §6.1 表格的代码化）。
///
/// 必须保持穷尽 match：reader/full 的归属关系以本函数为唯一事实来源，
/// [`flavor_tests::reader_full_classification_is_locked`] 负责锁定行为。
#[cfg(feature = "server")]
fn route_is_reader(route: &Route) -> bool {
    match route {
        Route::Home {}
        | Route::HomePage { .. }
        | Route::Notes {}
        | Route::NotebookDetail { .. }
        | Route::NoteDetail { .. }
        | Route::Archives {}
        | Route::TagDetail { .. }
        | Route::PostDetail { .. }
        | Route::Search {}
        | Route::About {}
        | Route::WritingGuide {}
        | Route::Friends {}
        | Route::Changelog {}
        | Route::NotFound { .. } => true,
        Route::ComponentShowcase { .. }
        | Route::ComponentDetail { .. }
        | Route::Admin {}
        | Route::Write {}
        | Route::WriteEdit { .. }
        | Route::PostPreview { .. }
        | Route::Posts {}
        | Route::AdminNotes {}
        | Route::NewNote {}
        | Route::EditNote { .. }
        | Route::AdminNotebooks {}
        | Route::PostsTrash {}
        | Route::Assets {}
        | Route::FriendsAdmin {}
        | Route::AdminComments {}
        | Route::AdminCommentsPage { .. }
        | Route::System {}
        | Route::Runner {}
        | Route::Mcp {}
        | Route::Logs {}
        | Route::SiteSettingsPage {}
        | Route::Profile {}
        | Route::Login {}
        | Route::Register {} => false,
    }
}

#[cfg(all(test, feature = "server"))]
mod flavor_tests {
    use super::*;
    use std::str::FromStr;

    /// 锁定 reader/full 路径分类（plan §6.2 门 2：路由归属由共享定义维护）。
    ///
    /// - 每个 reader 变体：`is_reader_path(canonical_url)` 为 true，且
    ///   `Route::from_str(url)` 回到同一变体（解析/序列化互逆）。
    /// - 每个 full 专属变体：`is_reader_path` 为 false。
    /// - 未知路径（NotFound catch-all 兜底）归 reader。
    ///
    /// `route_is_reader` 是穷尽 match，新增变体会先在服务端编译失败；
    /// 本测试再锁定分类行为与 URL 往返，防止解析层面的意外偏离。
    #[test]
    fn reader_full_classification_is_locked() {
        // (reader 变体, 该变体的示例参数)
        let reader_cases: Vec<Route> = vec![
            Route::Home {},
            Route::HomePage { page: 2 },
            Route::Notes {},
            Route::NotebookDetail { id: 7 },
            Route::NoteDetail {
                slug: "hello".into(),
            },
            Route::Archives {},
            Route::TagDetail { tag: "rust".into() },
            Route::PostDetail {
                slug: "some-post".into(),
            },
            Route::Search {},
            Route::About {},
            Route::WritingGuide {},
            Route::Friends {},
            Route::Changelog {},
            Route::NotFound {
                segments: vec!["no".into(), "such".into()],
            },
        ];
        for route in reader_cases {
            let url = route.to_string();
            assert!(is_reader_path(&url), "{url} 应归类为 reader（{route:?}）");
            let reparsed = Route::from_str(&url).unwrap_or_else(|e| {
                panic!("{url} 应能解析回 Route：{e}");
            });
            assert_eq!(reparsed, route, "{url} 解析往返应回到同一变体");
        }

        let full_cases: Vec<Route> = vec![
            Route::ComponentShowcase {},
            Route::ComponentDetail {
                component: "button".into(),
            },
            Route::Admin {},
            Route::Write {},
            Route::WriteEdit { id: 1 },
            Route::PostPreview {
                slug: "draft".into(),
            },
            Route::Posts {},
            Route::AdminNotes {},
            Route::NewNote {},
            Route::EditNote { id: 1 },
            Route::AdminNotebooks {},
            Route::PostsTrash {},
            Route::Assets {},
            Route::FriendsAdmin {},
            Route::AdminComments {},
            Route::AdminCommentsPage { page: 3 },
            Route::System {},
            Route::Runner {},
            Route::Mcp {},
            Route::Logs {},
            Route::SiteSettingsPage {},
            Route::Profile {},
            Route::Login {},
            Route::Register {},
        ];
        for route in full_cases {
            let url = route.to_string();
            assert!(!is_reader_path(&url), "{url} 应归类为 full（{route:?}）");
        }

        // 未知路径走 reader 的 NotFound 兜底模板。
        assert!(is_reader_path("/no/such/path"));
        // 带 fragment/query 的输入不在本函数职责内（调用方传 uri.path()）。
        assert!(Route::from_str("/post/x?y=1").is_err() || is_reader_path("/post/x"));
    }
}
