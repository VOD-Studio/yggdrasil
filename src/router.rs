//! 全站路由配置
//!
//! 使用 Dioxus Router 定义前端路由层级，包含前台布局、后台管理布局、
//! 独立的登录与注册页面。`Route` 枚举定义在 [`routes`] 子模块，其上的
//! `#[route("/path")]` 属性既用于生成 URL 匹配规则，也用于组件导航。

use dioxus::prelude::*;
use dioxus::router::components::HistoryProvider;
use std::sync::Arc;

mod routes;

pub use routes::Route;

use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::ui::IconName;
use crate::context::UserContext;
use crate::pages::not_found::NotFound;
use crate::theme::{use_theme_provider, ThemePreload};

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
    crate::components::ui::use_toast_provider();

    rsx! {
        document::Title { "Yggdrasil Blog" }
        // 网站图标由 scripts/generate-favicons.cjs 生成：SVG 随系统明暗换色，
        // 旧浏览器回退 ICO；manifest 供安装为应用。
        document::Link { rel: "icon", href: "/favicon.svg", r#type: "image/svg+xml" }
        document::Link { rel: "icon", href: "/favicon.ico", sizes: "16x16 32x32 48x48" }
        document::Link { rel: "apple-touch-icon", href: "/apple-touch-icon.png" }
        document::Link { rel: "manifest", href: "/manifest.webmanifest" }
        document::Meta { name: "theme-color", content: "#f5f5f3", "media": "(prefers-color-scheme: light)" }
        document::Meta { name: "theme-color", content: "#0f0f0e", "media": "(prefers-color-scheme: dark)" }
        div {
            ThemePreload {}
            HistoryProvider {
                history: move |_| crate::navigation::transition_history(dioxus::history::history()),
                Router::<Route> {}
            }
            crate::components::ui::ToastRegion {}
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
                    } else {
                        // 浏览器水合时错误从服务端数据反序列化而来，已不是 ServerFnError，
                        // 只剩文本（如 "error running server function: not_found ..."）。
                        // 这里判错会让客户端渲染另一套 fallback，与服务端 HTML 对不上。
                        let err_str = format!("{:?}", captured_error);
                        if err_str.contains("NotFound") || err_str.contains("404")
                            || err_str.contains("not found") || err_str.contains("not_found")
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
                        div { class: "flex min-h-[50vh] items-center justify-center px-6 md:min-h-[55vh] animate-page-enter",
                            EmptyState {
                                icon: IconName::CircleAlert,
                                title: "页面加载失败",
                                description: "网络或服务出了点问题，稍后再试一次。",
                                // 清除错误边界后命令式导航回首页。不能用 Link：导航前
                                // 无法清除错误，ErrorBoundary 会卡在 fallback，页面不随路由更新。
                                action: EmptyStateAction {
                                    label: "返回首页".to_string(),
                                    onclick: Callback::new(move |_| {
                                        err_ctx.clear_errors();
                                        let _ = dioxus::router::navigator().push(Route::Home {});
                                    }),
                                },
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
        | Route::Search { .. }
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
                book: "7".into(),
            },
            Route::Archives {},
            Route::TagDetail { tag: "rust".into() },
            Route::PostDetail {
                slug: "some-post".into(),
            },
            Route::Search { q: "rust".into() },
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
