//! 首页「最新一篇」的目录预览接口。
//!
//! 读取最新已发布文章已存储的 `toc_html`，在服务端解析一级小节，不重新渲染
//! Markdown。结果随文章列表缓存一起失效。
//! Leptos server function，注册在 `/api` 路径下。

use leptos::prelude::*;

use super::types::PostOutline;
#[cfg(feature = "server")]
use super::types::OUTLINE_PREVIEW_LIMIT;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;

/// 获取最新一篇已发布文章的目录预览；没有文章或文章没有小节时返回 `None`。
///
/// 排序与 `list_published_posts` 一致，前台再用 slug 核对首篇，防止两次读取之间
/// 有新文章发布造成错配。
#[server(LatestPostOutline, endpoint = "latest_post_outline", input = leptos::server_fn::codec::Json)]
pub async fn latest_post_outline() -> Result<Option<PostOutline>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        if let Some(cached) = crate::cache::get_latest_outline().await {
            return Ok(cached);
        }
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let row = client
            .query_opt(
                "SELECT slug, toc_html FROM posts
                 WHERE status = 'published' AND deleted_at IS NULL
                 ORDER BY published_at DESC
                 LIMIT 1",
                &[],
            )
            .await
            .map_err(AppError::query)?;
        let outline = row.and_then(|row| {
            let toc: Option<String> = row.get("toc_html");
            outline_from_toc(row.get("slug"), toc.as_deref().unwrap_or_default())
        });
        crate::cache::set_latest_outline(outline.clone()).await;
        Ok(outline)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(None)
    }
}

#[cfg(feature = "server")]
fn outline_from_toc(slug: String, toc_html: &str) -> Option<PostOutline> {
    let sections = crate::utils::toc::toc_sections(toc_html, OUTLINE_PREVIEW_LIMIT);
    (sections.total > 0).then(|| PostOutline {
        slug,
        total: u32::try_from(sections.total).unwrap_or(u32::MAX),
        headings: sections.headings,
    })
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn article_without_headings_has_no_outline() {
        assert_eq!(outline_from_toc("a".into(), ""), None);
    }

    #[test]
    fn outline_keeps_six_headings_and_total() {
        let md: String = (1..=8)
            .map(|i| format!("## 第 {i} 节\n\n正文\n\n"))
            .collect();
        let toc = crate::api::markdown::render_markdown_enhanced(&md).toc_html;
        let outline = outline_from_toc("post".into(), &toc).expect("有小节");
        assert_eq!(outline.slug, "post");
        assert_eq!(outline.total, 8);
        assert_eq!(outline.headings.len(), OUTLINE_PREVIEW_LIMIT);
        assert_eq!(outline.headings[0], "第 1 节");
    }
}
