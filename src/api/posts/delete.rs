//! 删除文章接口。
//!
//! 采用软删除方式，将 posts.deleted_at 设置为当前时间，
//! 并按影响范围失效相关缓存。
//! Dioxus server function，注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中执行删除与缓存失效。

use dioxus::prelude::*;

#[cfg(feature = "server")]
use super::helpers::get_current_admin_user;
use super::types::CreatePostResponse;
#[cfg(feature = "server")]
use super::{service, types::write_response};

/// 删除指定文章。
///
/// 仅 admin 可调用；通过设置 deleted_at 实现软删除，
/// 成功后按影响范围失效文章列表、标签云、统计、slug 及相关标签文章缓存。
#[server(DeletePost, "/api")]
pub async fn delete_post(post_id: i32) -> Result<CreatePostResponse, ServerFnError> {
    let _user = get_current_admin_user().await?;
    write_response(
        service::act(post_id, None, service::PostAction::Trash).await,
        "删除成功",
    )
}
