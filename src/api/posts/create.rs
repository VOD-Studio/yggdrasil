//! 创建文章接口。
//!
//! 校验标题、内容与 slug，生成唯一 slug 并渲染 Markdown，
//! 在事务中写入 posts 表并同步标签关联，最后失效相关缓存。
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

/// 创建一篇新文章。
///
/// 校验标题与内容非空、slug 格式合法；未提供 slug 时从标题自动生成。
/// 首次发布时填充 published_at，并同步标签与缓存失效。
#[server(CreatePost, endpoint = "create_post", input = leptos::server_fn::codec::Json)]
pub async fn create_post(
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
    write_response(service::create(user.id, draft, None).await, "创建成功")
}
