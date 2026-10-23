//! `Route` 枚举及其 `Routable` 派生
//!
//! 单独成模块，是为了把下面的 lint 豁免限制在派生产物上：`Routable` 为
//! `String` 段生成 `map_err(|err| …ParseError(err))`，`err` 的类型是
//! `Infallible`，nightly 的 unreachable_code 会把它报成不可达调用
//! （rust-lang/rust#149571）。派生 impl 与枚举同级，挂在枚举上的 `allow`
//! 覆盖不到，只能放在模块上。
#![allow(unreachable_code)]

use dioxus::prelude::*;

use super::{ErrorLayout, NavigationLayout};
// full 客户端专属页面组件：reader 构建（无 client-full）随变体一起剥离。
#[cfg(not(ygg_client_reader))]
use crate::components::admin_layout::AdminLayout;
use crate::components::frontend_layout::FrontendLayout;
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
            /// 笔记阅读页；`?book=` 指明在哪个笔记本里读
            #[route("/notes/:slug?:book")]
            NoteDetail { slug: String, book: String },
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
            /// 搜索页；`?q=` 带入初始查询（搜索面板的「全文搜索」入口）
            #[route("/search?:q")]
            Search { q: String },
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
