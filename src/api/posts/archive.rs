//! 固定大小的公开归档分页，与首页列表的 50 条上限独立。
use leptos::prelude::*;

use crate::models::archive::ArchivePost;

pub const ARCHIVE_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveResponse {
    pub posts: Vec<ArchivePost>,
    pub total: i64,
}

#[server(ListArchivePosts, endpoint = "list_archive_posts", input = leptos::server_fn::codec::Json)]
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

/// 某一年发布的篇数。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct YearCount {
    pub year: i32,
    pub count: i64,
}

/// 每年发布的篇数，按年份升序；中间没有文章的年份补 0。关于页的柱状图使用，
/// 在服务端聚合并随文章列表缓存一起失效。
#[server(PostsPerYear, endpoint = "posts_per_year", input = leptos::server_fn::codec::Json)]
pub async fn posts_per_year() -> Result<Vec<YearCount>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::error::AppError;
        if let Some(cached) = crate::cache::get_year_counts().await {
            return Ok(cached);
        }
        let client = crate::db::pool::get_conn()
            .await
            .map_err(AppError::db_conn)?;
        let rows = client
            .query(
                "SELECT EXTRACT(YEAR FROM COALESCE(published_at, created_at))::int AS year, COUNT(*)
                 FROM posts WHERE status = 'published' AND deleted_at IS NULL
                 GROUP BY 1 ORDER BY 1",
                &[],
            )
            .await
            .map_err(AppError::query)?;
        let counts = fill_years(rows.iter().map(|r| (r.get(0), r.get(1))).collect());
        crate::cache::set_year_counts(counts.clone()).await;
        Ok(counts)
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(Vec::new())
    }
}

/// 补齐首尾之间缺失的年份（篇数为 0），柱状图才不会跳年。
#[cfg(any(feature = "server", test))]
fn fill_years(counts: Vec<(i32, i64)>) -> Vec<YearCount> {
    let (Some(&(first, _)), Some(&(last, _))) = (counts.first(), counts.last()) else {
        return Vec::new();
    };
    (first..=last)
        .map(|year| YearCount {
            year,
            count: counts
                .iter()
                .find(|(y, _)| *y == year)
                .map_or(0, |(_, n)| *n),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_years_are_filled_with_zero() {
        let years = fill_years(vec![(2020, 1), (2022, 12), (2023, 4)]);
        let shape: Vec<_> = years.iter().map(|y| (y.year, y.count)).collect();
        assert_eq!(shape, [(2020, 1), (2021, 0), (2022, 12), (2023, 4)]);
        assert!(fill_years(Vec::new()).is_empty());
    }
}
