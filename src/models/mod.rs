//! 数据模型模块。
//!
//! 定义博客系统使用的核心领域模型，包括文章（Post）、用户（User）与评论（Comment）。
//! 这些结构体通过 serde 在服务端与客户端之间共享序列化。

/// 素材（图片）模型：assets 注册表与引用关联的 serde DTO。
pub mod asset;
/// 评论模型及其状态枚举。
pub mod comment;
/// 友链模型。
pub mod friend_link;
/// 运行日志查看器的共享 DTO。
pub mod log;
/// MCP 服务器访问令牌模型与作用域枚举。
/// allow(dead_code)：T1 仅定义类型；T2 的 token 管理服务端函数才构造这些 DTO。
#[allow(dead_code)]
pub mod mcp_token;
/// 文章模型、文章状态、标签与统计信息。
pub mod post;
/// 回收站与站点配置模型。
pub mod settings;
/// 主机指标快照模型（server 状态聚合用，两端共享序列化）。
pub mod system;
/// 用户模型、用户角色与可公开用户信息。
pub mod user;
