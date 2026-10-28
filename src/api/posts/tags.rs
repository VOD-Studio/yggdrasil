//! 标签列表接口。
//!
//! 返回所有标签及其关联的已发布文章数量，用于标签云与侧边栏。
//! Leptos server function，注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中查询数据库。

use crate::api::error::ApiError;
use leptos::prelude::*;

use super::types::TagListResponse;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;
#[cfg(feature = "server")]
use crate::models::post::Tag;

/// 获取全部标签列表。
///
/// 优先命中缓存；未命中时聚合每个标签关联的已发布文章数量，并按标签名升序排列。
#[server(ListTags, endpoint = "list_tags", input = leptos::server_fn::codec::Json)]
pub async fn list_tags() -> Result<TagListResponse, ApiError> {
    #[cfg(feature = "server")]
    {
        if let Some(cached) = crate::cache::get_tag_list().await {
            return Ok(TagListResponse { tags: cached });
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;

        // 聚合标签对应的已发布、未删除文章数量。
        let rows = client
            .query(
                "SELECT t.id, t.name, COUNT(p.id) as post_count
                 FROM tags t
                 LEFT JOIN post_tags pt ON t.id = pt.tag_id
                 LEFT JOIN posts p ON pt.post_id = p.id AND p.deleted_at IS NULL AND p.status = 'published'
                 GROUP BY t.id, t.name
                 ORDER BY t.name",
                &[],
            )
            .await
            .map_err(AppError::query)?;

        let tags: Vec<Tag> = rows
            .iter()
            .map(|r| Tag {
                id: r.get("id"),
                name: r.get("name"),
                post_count: r.get("post_count"),
            })
            .collect();

        crate::cache::set_tag_list(tags.clone()).await;
        Ok(TagListResponse { tags })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(TagListResponse { tags: Vec::new() })
    }
}
