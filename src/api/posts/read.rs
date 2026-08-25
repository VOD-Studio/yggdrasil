//! 文章详情查询接口。
//!
//! 提供按 id（管理员）与按 slug（公开）两种方式获取文章，
//! 其中按 slug 查询包含上下篇导航并启用缓存。
//! Dioxus server function，注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中查询数据库。

use dioxus::prelude::*;

#[cfg(feature = "server")]
use super::helpers::{get_current_admin_user, row_to_post_full};
use super::types::SinglePostResponse;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;

/// 根据文章 id 获取详情。
///
/// 需要 admin 权限；不缓存，用于管理后台编辑等场景。
#[server(GetPostById, "/api")]
pub async fn get_post_by_id(post_id: i32) -> Result<SinglePostResponse, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        let row = client
            .query_opt(
                "SELECT
                    p.id, p.author_id, p.title, p.slug, p.summary, p.content_md, p.content_html, p.toc_html,
                    p.status, p.published_at, p.created_at, p.updated_at, p.cover_image,
                    p.word_count, p.reading_time,
                    COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as tags
                 FROM posts p
                 LEFT JOIN post_tags pt ON p.id = pt.post_id
                 LEFT JOIN tags t ON pt.tag_id = t.id
                 WHERE p.id = $1 AND p.deleted_at IS NULL
                 GROUP BY p.id",
                &[&post_id],
            )
            .await
            .map_err(AppError::query)?;

        let post = match row {
            Some(row) => Some(row_to_post_full(&row).await?),
            None => None,
        };

        Ok(SinglePostResponse { post })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(SinglePostResponse { post: None })
    }
}

/// 根据 slug 获取公开文章详情。
///
/// 优先命中缓存；未命中时查询数据库，并附带基于 published_at 的上一篇/下一篇导航。
#[server(GetPostBySlug, "/api")]
pub async fn get_post_by_slug(slug: String) -> Result<SinglePostResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        if let Some(cached) = crate::cache::get_post_by_slug(&slug).await {
            return Ok(SinglePostResponse { post: cached });
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;

        // 使用 LATERAL JOIN 查询按 published_at 排序的相邻文章。
        let row = client
            .query_opt(
                "SELECT
                    p.id, p.author_id, p.title, p.slug, p.summary, p.content_md, p.content_html, p.toc_html,
                    p.status, p.published_at, p.created_at, p.updated_at, p.cover_image,
                    p.word_count, p.reading_time,
                    COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as tags,
                    prev.title as prev_title, prev.slug as prev_slug,
                    next.title as next_title, next.slug as next_slug
                 FROM posts p
                 LEFT JOIN post_tags pt ON p.id = pt.post_id
                 LEFT JOIN tags t ON pt.tag_id = t.id
                 LEFT JOIN LATERAL (
                     SELECT title, slug FROM posts 
                     WHERE published_at < p.published_at 
                       AND status = 'published' 
                       AND deleted_at IS NULL
                     ORDER BY published_at DESC
                     LIMIT 1
                 ) prev ON true
                 LEFT JOIN LATERAL (
                     SELECT title, slug FROM posts 
                     WHERE published_at > p.published_at 
                       AND status = 'published' 
                       AND deleted_at IS NULL
                     ORDER BY published_at ASC
                     LIMIT 1
                 ) next ON true
                 WHERE p.slug = $1 AND p.status = 'published' AND p.deleted_at IS NULL
                 GROUP BY p.id, prev.title, prev.slug, next.title, next.slug",
                &[&slug],
            )
            .await
            .map_err(AppError::query)?;

        let post = match row {
            Some(row) => Some(row_to_post_full(&row).await?),
            None => None,
        };

        if post.is_some() {
            crate::cache::set_post_by_slug(&slug, post.clone()).await;
        }
        Ok(SinglePostResponse { post })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(SinglePostResponse { post: None })
    }
}

/// 管理员按 slug 预览文章（含草稿），不缓存、不带上下篇导航。
///
/// 镜像 [`get_post_by_id`] 的查询，但按 slug 取、不做 `status = 'published'`
/// 过滤，故草稿可被预览。回收站（软删除）文章返回 `None`。草稿绝不进公开缓存
/// （`cache::get/set_post_by_slug`），因此预览路径只走一次直查。
#[server(GetPostPreview, "/api")]
pub async fn get_post_preview(slug: String) -> Result<SinglePostResponse, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        let row = client
            .query_opt(
                "SELECT
                    p.id, p.author_id, p.title, p.slug, p.summary, p.content_md, p.content_html, p.toc_html,
                    p.status, p.published_at, p.created_at, p.updated_at, p.cover_image,
                    p.word_count, p.reading_time,
                    COALESCE(array_agg(t.name) FILTER (WHERE t.name IS NOT NULL), '{}') as tags
                 FROM posts p
                 LEFT JOIN post_tags pt ON p.id = pt.post_id
                 LEFT JOIN tags t ON pt.tag_id = t.id
                 WHERE p.slug = $1 AND p.deleted_at IS NULL
                 GROUP BY p.id",
                &[&slug],
            )
            .await
            .map_err(AppError::query)?;

        let post = match row {
            Some(row) => Some(row_to_post_full(&row).await?),
            None => None,
        };

        Ok(SinglePostResponse { post })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(SinglePostResponse { post: None })
    }
}
