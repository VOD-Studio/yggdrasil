//! 站内地址枚举：只负责生成 href，不参与路由匹配。
//!
//! 路由匹配由 `crate::app` 里的 `<Routes>` 表完成；这里的 [`Route`] 让调用方用
//! 类型安全的变体拼地址（`<a href=Route::PostDetail { slug }.to_string()>`、
//! `crate::navigation::push(&route.to_string())`），避免散落的字符串拼接。
//!
//! 编码规则沿用 dioxus-router 0.7.10：动态路径段与查询值分别使用 [`PATH_ASCII_SET`]、
//! [`QUERY_ASCII_SET`] 做百分号编码，查询参数值为空时整个 `?key=` 省略。

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use std::fmt;

/// 查询值需要编码的字符。
const QUERY_ASCII_SET: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>');

/// 路径段需要编码的字符（在查询集合上追加 `? ^ ` { }`）。
const PATH_ASCII_SET: &AsciiSet = &QUERY_ASCII_SET
    .add(b'?')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// 全站路由，每个变体对应一个页面路径。
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    /// 首页
    Home,
    /// 首页分页
    HomePage { page: i32 },
    /// 笔记索引
    Notes,
    /// 笔记本详情
    NotebookDetail { id: i32 },
    /// 笔记阅读页；`book` 指明在哪个笔记本里读（空串省略）
    NoteDetail { slug: String, book: String },
    /// 文章归档与标签索引
    Archives,
    /// 单个标签下的文章列表
    TagDetail { tag: String },
    /// 文章详情页，按 slug 匹配
    PostDetail { slug: String },
    /// 搜索页；`q` 带入初始查询（空串省略）
    Search { q: String },
    /// 关于页面
    About,
    /// Agent 写作技能与 MCP 接入指南
    WritingGuide,
    /// 项目自写组件的公开图鉴
    ComponentShowcase,
    /// 单枚组件的交互示例与用法
    ComponentDetail { component: String },
    /// 友链页
    Friends,
    /// 更新日志页
    Changelog,
    /// 后台仪表盘
    Admin,
    /// 写文章页
    Write,
    /// 编辑文章页
    WriteEdit { id: i32 },
    /// 草稿/文章预览（管理员只读）
    PostPreview { slug: String },
    /// 文章管理
    Posts,
    /// 笔记管理
    AdminNotes,
    /// 新建笔记
    NewNote,
    /// 编辑笔记
    EditNote { id: i32 },
    /// 笔记本管理
    AdminNotebooks,
    /// 文章回收站
    PostsTrash,
    /// 素材管理
    Assets,
    /// 友链管理
    FriendsAdmin,
    /// 评论管理
    AdminComments,
    /// 评论管理分页
    AdminCommentsPage { page: i32 },
    /// 系统管理
    System,
    /// 代码试运行沙箱
    Runner,
    /// MCP 令牌管理
    Mcp,
    /// 运行日志
    Logs,
    /// 站点配置
    SiteSettingsPage,
    /// 个人信息
    Profile,
    /// 登录页
    Login,
    /// 注册页
    Register,
}

/// 编码一个动态路径段。
fn seg(value: &str) -> impl fmt::Display + '_ {
    utf8_percent_encode(value, PATH_ASCII_SET)
}

/// 写出可选查询参数：值为空时整个 `?key=` 省略。
fn query(f: &mut fmt::Formatter<'_>, key: &str, value: &str) -> fmt::Result {
    if value.is_empty() {
        Ok(())
    } else {
        write!(f, "?{key}={}", utf8_percent_encode(value, QUERY_ASCII_SET))
    }
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Route::Home => f.write_str("/"),
            Route::HomePage { page } => write!(f, "/page/{page}"),
            Route::Notes => f.write_str("/notes"),
            Route::NotebookDetail { id } => write!(f, "/notes/book/{id}"),
            Route::NoteDetail { slug, book } => {
                write!(f, "/notes/{}", seg(slug))?;
                query(f, "book", book)
            }
            Route::Archives => f.write_str("/archives"),
            Route::TagDetail { tag } => write!(f, "/tags/{}", seg(tag)),
            Route::PostDetail { slug } => write!(f, "/post/{}", seg(slug)),
            Route::Search { q } => {
                f.write_str("/search")?;
                query(f, "q", q)
            }
            Route::About => f.write_str("/about"),
            Route::WritingGuide => f.write_str("/about/writing"),
            Route::ComponentShowcase => f.write_str("/about/components"),
            Route::ComponentDetail { component } => {
                write!(f, "/about/components/{}", seg(component))
            }
            Route::Friends => f.write_str("/friends"),
            Route::Changelog => f.write_str("/changelog"),
            Route::Admin => f.write_str("/admin"),
            Route::Write => f.write_str("/admin/write"),
            Route::WriteEdit { id } => write!(f, "/admin/write/{id}"),
            Route::PostPreview { slug } => write!(f, "/admin/preview/{}", seg(slug)),
            Route::Posts => f.write_str("/admin/posts"),
            Route::AdminNotes => f.write_str("/admin/notes"),
            Route::NewNote => f.write_str("/admin/notes/new"),
            Route::EditNote { id } => write!(f, "/admin/notes/edit/{id}"),
            Route::AdminNotebooks => f.write_str("/admin/notebooks"),
            Route::PostsTrash => f.write_str("/admin/posts/trash"),
            Route::Assets => f.write_str("/admin/assets"),
            Route::FriendsAdmin => f.write_str("/admin/friends"),
            Route::AdminComments => f.write_str("/admin/comments"),
            Route::AdminCommentsPage { page } => write!(f, "/admin/comments/{page}"),
            Route::System => f.write_str("/admin/system"),
            Route::Runner => f.write_str("/admin/runner"),
            Route::Mcp => f.write_str("/admin/mcp"),
            Route::Logs => f.write_str("/admin/logs"),
            Route::SiteSettingsPage => f.write_str("/admin/settings"),
            Route::Profile => f.write_str("/admin/profile"),
            Route::Login => f.write_str("/login"),
            Route::Register => f.write_str("/register"),
        }
    }
}

/// 是否为文章详情路径（`/post/` 前缀）：文章页主区放宽，分享元信息由页面自己写。
pub fn is_post_path(path: &str) -> bool {
    path.starts_with("/post/")
}

/// 是否为组件图鉴路径（`/about/components` 或其子路径）。
pub fn is_showcase_path(path: &str) -> bool {
    path == "/about/components" || path.starts_with("/about/components/")
}

/// 是否为后台路径（`/admin` 或其子路径）。按路径段判断：同前缀的静态文件 `/admin.css`
/// 是公开资源（组件图鉴也加载它），不能被后台登录守卫拦截。
pub fn is_admin_path(path: &str) -> bool {
    path == "/admin" || path.starts_with("/admin/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_segments_are_percent_encoded() {
        assert_eq!(
            Route::TagDetail {
                tag: "Rust 编程".to_owned()
            }
            .to_string(),
            "/tags/Rust%20%E7%BC%96%E7%A8%8B"
        );
        assert_eq!(
            Route::PostDetail {
                slug: "a?b".to_owned()
            }
            .to_string(),
            "/post/a%3Fb"
        );
    }

    #[test]
    fn query_values_are_encoded_and_empty_values_omitted() {
        assert_eq!(
            Route::Search {
                q: "a b".to_owned()
            }
            .to_string(),
            "/search?q=a%20b"
        );
        assert_eq!(Route::Search { q: String::new() }.to_string(), "/search");
        assert_eq!(
            Route::NoteDetail {
                slug: "x".to_owned(),
                book: "3".to_owned()
            }
            .to_string(),
            "/notes/x?book=3"
        );
        assert_eq!(
            Route::NoteDetail {
                slug: "x".to_owned(),
                book: String::new()
            }
            .to_string(),
            "/notes/x"
        );
    }

    #[test]
    fn numeric_segments_and_admin_paths() {
        assert_eq!(
            Route::AdminCommentsPage { page: 2 }.to_string(),
            "/admin/comments/2"
        );
        assert_eq!(Route::HomePage { page: 3 }.to_string(), "/page/3");
        assert_eq!(Route::Admin.to_string(), "/admin");
        assert_eq!(Route::EditNote { id: 7 }.to_string(), "/admin/notes/edit/7");
    }

    #[test]
    fn path_classifiers() {
        assert!(is_post_path("/post/hello"));
        assert!(!is_post_path("/posts"));
        assert!(is_admin_path("/admin"));
        assert!(is_admin_path("/admin/posts"));
        assert!(!is_admin_path("/admin.css"));
        assert!(!is_admin_path("/administrator"));
        assert!(is_showcase_path("/about/components"));
        assert!(is_showcase_path("/about/components/button"));
        assert!(!is_showcase_path("/about/componentsx"));
        assert!(!is_showcase_path("/about"));
    }
}
