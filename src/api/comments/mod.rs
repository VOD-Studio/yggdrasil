//! 评论模块：提供评论的 CRUD、Markdown 渲染、审核状态流转与分页查询。
//!
//! 所有 Dioxus server function 均注册在 `/api` 路径下，供前端与服务端调用。
//! 仅在 `feature = "server"` 启用的服务端构建中执行数据库操作与缓存失效。

#![allow(clippy::unused_unit, deprecated, unused_imports)]

mod check;
mod create;
mod helpers;
mod list;
mod markdown;
mod read;
mod types;
mod update;

/// 查询一组评论的当前审核状态。
pub use check::check_pending_status;
/// 创建一条新评论。
pub use create::create_comment;
/// 获取全部评论分页列表。
pub use list::get_all_comments;
/// 获取指定文章的已审核评论列表。
pub use read::get_comments;
/// 评论 API 的请求与响应数据结构。
pub use types::*;
/// 通过指定评论。
pub use update::approve_comment;
/// 批量更新评论状态。
pub use update::batch_update_comment_status;
/// 将指定评论标记为垃圾评论。
pub use update::spam_comment;
/// 将指定评论移入回收站。
pub use update::trash_comment;
