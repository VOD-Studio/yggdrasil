//! 固定大小的公开归档分页，与首页列表的 50 条上限独立。
use dioxus::prelude::*;

use crate::models::archive::ArchivePost;

pub const ARCHIVE_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveResponse {
    pub posts: Vec<ArchivePost>,
    pub total: i64,
}

#[server(ListArchivePosts, "/api")]
pub async fn list_archive_posts(page: i32) -> Result<ArchiveResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::error::AppError;
        let page = page.clamp(1, 10_000);
        if let Some(response) = crate::cache::get_archive(page).await {
            return Ok(response);
        }
        let client = crate::db::pool::get_conn()
            .await
            .map_err(AppError::db_conn)?;
        let total: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM posts WHERE status = 'published' AND deleted_at IS NULL",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .get(0);
        let rows = client.query(
            "SELECT p.id, p.title, p.slug, COALESCE(p.published_at, p.created_at) AS published_at,
                    p.reading_time, COALESCE(array_agg(t.name ORDER BY t.name) FILTER (WHERE t.name IS NOT NULL), '{}') AS tags
             FROM posts p LEFT JOIN post_tags pt ON pt.post_id = p.id LEFT JOIN tags t ON t.id = pt.tag_id
             WHERE p.status = 'published' AND p.deleted_at IS NULL
             GROUP BY p.id ORDER BY COALESCE(p.published_at, p.created_at) DESC, p.id DESC LIMIT $1 OFFSET $2",
            &[&(ARCHIVE_PAGE_SIZE as i64), &((i64::from(page) - 1) * ARCHIVE_PAGE_SIZE as i64)],
        ).await.map_err(AppError::query)?;
        let posts = rows
            .iter()
            .map(|row| ArchivePost {
                id: row.get("id"),
                title: row.get("title"),
                slug: row.get("slug"),
                published_at: row.get("published_at"),
                reading_time: row.get::<_, i32>("reading_time").max(1) as u32,
                tags: row.get("tags"),
            })
            .collect();
        let response = ArchiveResponse { posts, total };
        crate::cache::set_archive(page, response.clone()).await;
        Ok(response)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = page;
        Ok(ArchiveResponse {
            posts: vec![],
            total: 0,
        })
    }
}
