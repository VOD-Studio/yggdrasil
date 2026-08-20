//! 全站路由配置
//!
//! 使用 Dioxus Router 定义前端路由层级，包含前台布局、后台管理布局、
//! 独立的登录与注册页面。`Route` 枚举上的 `#[route("/path")]` 属性
//! 既用于生成 URL 匹配规则，也用于组件导航。

use dioxus::prelude::*;
use std::sync::Arc;

use crate::components::admin_layout::AdminLayout;
use crate::components::frontend_layout::FrontendLayout;
use crate::context::UserContext;
use crate::pages::about::About;
use crate::pages::admin::{
    Admin, AdminComments, AdminCommentsPage, Assets, FriendsAdmin, Logs, Mcp, PostPreview, Posts,
    PostsTrash, Profile, Runner, SiteSettingsPage, System, Write, WriteEdit,
};
use crate::pages::archives::Archives;
use crate::pages::changelog::Changelog;
use crate::pages::friends::Friends;
use crate::pages::home::{Home, HomePage};
use crate::pages::login::Login;
use crate::pages::not_found::NotFound;
use crate::pages::post_detail::PostDetail;
use crate::pages::register::Register;
use crate::pages::search::Search;
use crate::pages::tags::{TagDetail, Tags};
use crate::theme::{use_theme_provider, ThemePreload};

/// 全站路由枚举，每个变体对应一个页面路径
#[derive(Clone, Routable, Debug, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    // 前台页面共享布局，最外层嵌套错误边界布局以拦截报错
    #[layout(ErrorLayout)]
        #[layout(FrontendLayout)]
            /// 首页
            #[route("/")]
            Home {},
            /// 首页分页
            #[route("/page/:page")]
            HomePage { page: i32 },
            /// 文章归档页
            #[route("/archives")]
            Archives {},
            /// 标签列表页
            #[route("/tags")]
            Tags {},
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

    // 后台管理路由嵌套在 `/admin` 下
    #[nest("/admin")]
    // 后台页面共享管理布局
    #[layout(AdminLayout)]
        /// 后台仪表盘
        #[route("/")]
        Admin {},
        /// 写文章页
        #[route("/write")]
        Write {},
        /// 编辑文章页
        #[route("/write/:id")]
        WriteEdit { id: i32 },
        /// 草稿/文章预览（管理员只读）
        #[route("/preview/:slug")]
        PostPreview { slug: String },
        /// 文章管理（全部文章列表）
        #[route("/posts")]
        Posts {},
        /// 回收站（已软删除文章；原 /admin/posts 的客户端 tab，独立路由后归「内容管理」子菜单）
        #[route("/posts/trash")]
        PostsTrash {},
        /// 素材管理（上传图片注册表：浏览/搜索/删除/孤儿清理）
        #[route("/assets")]
        Assets {},
        /// 友链管理
        #[route("/friends")]
        FriendsAdmin {},
        /// 评论管理
        #[route("/comments")]
        AdminComments {},
        /// 评论管理分页
        #[route("/comments/:page")]
        AdminCommentsPage { page: i32 },
        /// 系统管理（数据库 + 服务器状态 + SQL 控制台 + 导出 + 备份）
        #[route("/system")]
        System {},
        /// 代码试运行沙箱（作者预览可运行代码块输出）
        #[route("/runner")]
        Runner {},
        /// MCP 令牌管理 + 客户端配置生成
        #[route("/mcp")]
        Mcp {},
        /// 运行日志（进程内日志查询 + SSE 实时流 + 导出）
        #[route("/logs")]
        Logs {},
        /// 站点配置（页脚 GitHub 链接等公开配置）
        #[route("/settings")]
        SiteSettingsPage {},
        /// 个人信息（当前账号的展示资料与密码）
        #[route("/profile")]
        Profile {},
    #[end_layout]
    #[end_nest]

    /// 登录页面
    #[route("/login")]
    Login {},
    /// 注册页面
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

    rsx! {
        document::Stylesheet { href: "/style.css" }
        document::Stylesheet { href: "/highlight.css" }
        document::Title { "Yggdrasil Blog" }
        document::Link { rel: "icon", href: "/favicon.ico" }
        div {
            ThemePreload {}
            Router::<Route> {}
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
