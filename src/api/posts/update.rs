//! 更新文章接口。
//!
//! 校验管理员权限与文章归属，重新生成唯一 slug、渲染 Markdown，
//! 在事务中更新 posts 表并同步标签，最后失效相关缓存。
//! Leptos server function，注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中写入数据库。

#![allow(clippy::too_many_arguments)]

use crate::api::error::ApiError;
use leptos::prelude::*;

#[cfg(feature = "server")]
use super::helpers::get_current_admin_user;
use super::types::CreatePostResponse;
#[cfg(feature = "server")]
use super::{service, types::write_response};

/// 更新指定文章。
///
/// 校验文章存在且属于当前 admin；处理 slug 变更、发布状态转换、标签同步，
/// 并失效文章详情、列表、标签与统计缓存。
#[server(UpdatePost, endpoint = "update_post", input = leptos::server_fn::codec::Json)]
pub async fn update_post(
    post_id: i32,
    title: String,
    slug: Option<String>,
    summary: Option<String>,
    content_md: String,
    status: String,
    tags: Vec<String>,
    cover_image: Option<String>,
) -> Result<CreatePostResponse, ApiError> {
    let user = get_current_admin_user().await?;
    let draft = service::PostDraft {
        title,
        slug,
        summary,
        content_md,
        status,
        tags,
        cover_image,
    };
    write_response(
        service::update(user.id, post_id, service::UpdatePost::Replace(draft)).await,
        "更新成功",
    )
}
