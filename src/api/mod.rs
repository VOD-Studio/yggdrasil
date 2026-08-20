//! API 层根模块。
//!
//! 按职责划分子模块，包含两类接口：
//! - Dioxus server function（`#[server(Name, "/api")]`），如 `auth`、`posts`；
//! - Axum 手动路由处理器，如 `upload`、`image`。

/// 素材管理（assets/asset_refs）的 Dioxus server function。
pub mod assets;
/// 认证相关的 Dioxus server function。
pub mod auth;
/// 更新日志（内嵌 CHANGELOG.md 渲染）的 Dioxus server function。
pub mod changelog;
/// 代码运行接口与数据结构。
pub mod code_runner;
/// 评论相关接口。
pub mod comments;
/// CSRF 防护中间件。
pub mod csrf;
/// 数据库管理接口（运行状态 / SQL 控制台 / 导出 / 备份恢复）。
pub mod database;
/// 应用错误类型与转换。
pub mod error;
/// RSS 2.0 与 JSON Feed 1.1 订阅端点（server-only）。
pub mod feed;
/// 友链 CRUD 与前台列表的 Dioxus server function。
pub mod friends;
/// 健康检查端点（liveness / readiness）。
pub mod health;
/// 图片服务的 Axum 处理器。
pub mod image;
/// KaTeX 服务端数学公式渲染（server-only）。
#[cfg(feature = "server")]
pub mod katex;
/// 运行日志查看器（查询 / 导出 / target 列表 / 设置 / SSE 实时流）。
pub mod logs;
/// Markdown 渲染与 HTML 清理。
pub mod markdown;
/// MCP 访问令牌管理（签发 / 列表 / 重查 / 撤销）的 Dioxus server function。
pub mod mcp_tokens;
/// mhchem 化学公式转译器（\ce/\pu → LaTeX，server-only）。
#[cfg(feature = "server")]
pub mod mhchem;
/// 文章 CRUD 相关接口。
pub mod posts;
/// 个人信息（当前账号资料与密码）的 Dioxus server function。
pub mod profile;
/// 限流工具。
pub mod rate_limit;
/// HTML 消毒器。
pub mod sanitizer;
/// 回收站与站点配置接口。
pub mod settings;
/// URL slug 生成与校验。
pub mod slug;
/// 图片上传的 Axum 处理器。
pub mod upload;
/// SSRF 防护的 URL 抓取（服务端按图，供 MCP upload_media 工具）。
#[cfg(feature = "server")]
pub mod url_fetch;
