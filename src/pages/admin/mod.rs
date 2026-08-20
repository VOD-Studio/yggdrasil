//! 管理后台页面模块。
//!
//! 汇总并重新导出后台管理相关的页面组件，供路由与其他模块使用。

/// 素材管理页面模块。
pub mod assets;
/// 评论管理页面模块。
pub mod comments;
/// 管理后台仪表盘页面模块。
pub mod dashboard;
/// 友链管理页面模块。
pub mod friends;
/// 运行日志查看页面模块（/admin/logs）。
pub mod logs;
/// MCP 令牌管理 + 客户端配置生成页面模块。
pub mod mcp;
/// 文章管理列表页面模块。
pub mod posts;
/// 回收站页面模块（/admin/posts/trash 独立路由）。
pub mod posts_trash;
/// 草稿/文章预览页面模块（管理员只读）。
pub mod preview;
/// 个人信息页面模块（当前账号资料与密码）。
pub mod profile;
/// 代码试运行沙箱页面模块。
pub mod runner;
/// 站点配置页面模块（页脚 GitHub 链接等公开配置）。
pub mod settings;
/// 系统管理页面模块（数据库 + 服务器状态 + SQL 控制台 + 导出 + 备份）。
pub mod system;
/// 文章编辑器页面模块（基于 Tiptap 富文本编辑器）。
pub mod write;

/// 素材管理页面组件。
pub use assets::Assets;
/// 评论管理入口组件（带默认分页）。
pub use comments::{AdminComments, AdminCommentsPage};
/// 管理后台仪表盘组件。
pub use dashboard::Admin;
/// 友链管理页面组件。
pub use friends::FriendsAdmin;
/// 运行日志查看页面组件。
pub use logs::Logs;
/// MCP 令牌管理 + 客户端配置生成页面组件。
pub use mcp::Mcp;
/// 文章管理入口组件（全部文章列表）。
pub use posts::Posts;
/// 回收站页面组件。
pub use posts_trash::PostsTrash;
/// 草稿/文章预览页面组件（管理员只读）。
pub use preview::PostPreview;
/// 个人信息页面组件。
pub use profile::Profile;
/// 代码试运行沙箱组件。
pub use runner::Runner;
/// 站点配置页面组件。
pub use settings::SiteSettingsPage;
/// 系统管理入口组件。
pub use system::System;
/// 文章编辑器组件（新建与编辑模式）。
pub use write::{Write, WriteEdit};
