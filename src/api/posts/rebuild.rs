//! 批量重建文章 HTML 与目录。
//!
//! 用于数据迁移或修复：遍历符合条件的文章，将 Markdown 重新渲染为 HTML，
//! 并更新 content_html 与 toc_html 字段。
//! Leptos server function，注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中执行数据库更新。

use crate::api::error::ApiError;
use leptos::prelude::*;

#[cfg(feature = "server")]
use super::helpers::{get_current_admin_user, render_post_fields_minimal};
#[cfg(feature = "server")]
use crate::api::error::AppError;
use crate::api::posts::{CreatePostResponse, RebuildResult};
#[cfg(feature = "server")]
use crate::db::pool::get_conn;

/// 单次重建批处理数量。每批 100 篇，保证单次调用在 30 s 请求超时内完成。
#[cfg(feature = "server")]
const REBUILD_BATCH_LIMIT: i64 = 100;
/// 返回给前端展示的最大错误条数。
#[cfg(feature = "server")]
const MAX_DISPLAY_ERRORS: usize = 5;

/// 批量重建文章 content_html 与 toc_html（单批，客户端按游标续传）。
///
/// 当 `rebuild_all` 为 true 时重建所有未删除文章；否则仅重建 content_html 为空的文章。
/// 只处理 `id > after_id` 的前 [`REBUILD_BATCH_LIMIT`] 篇；批满时 `next_after_id` 给出续传游标，
/// 为 None 表示已处理完。渲染在事务外进行，写回用 `updated_at` 乐观校验：渲染期间被编辑的文章
/// 其写路径已自行渲染，这里跳过。渲染异常或写入失败会被捕获并汇总。
#[server(RebuildContentHtml, endpoint = "rebuild_content_html", input = leptos::server_fn::codec::Json)]
pub async fn rebuild_content_html(
    rebuild_all: bool,
    after_id: i32,
) -> Result<RebuildResult, ApiError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        // 两种模式都必须按游标分页：渲染失败的行 content_html 仍为 NULL，无游标会被反复选中。
        let missing_only = if rebuild_all {
            ""
        } else {
            "AND content_html IS NULL"
        };
        let query = format!(
            "SELECT id, content_md, updated_at FROM posts \
             WHERE deleted_at IS NULL {missing_only} AND id > $1 ORDER BY id LIMIT {REBUILD_BATCH_LIMIT}"
        );

        let mut rebuilt: u64 = 0;
        let mut failed: u64 = 0;
        let mut errors: Vec<String> = Vec::new();
        let mut aborted = false;

        let rows = client
            .query(&query, &[&after_id])
            .await
            .map_err(AppError::query)?;

        for row in &rows {
            let id: i32 = row.get(0);
            let content_md: String = row.get(1);
            let updated_at: chrono::DateTime<chrono::Utc> = row.get(2);

            // Markdown 渲染 + 度量派生收敛到 helper（R4）。
            let (content_html, toc_html, word_count, reading_time) =
                match render_post_fields_minimal(&content_md).await {
                    Ok(f) => f,
                    Err(_) => {
                        failed += 1;
                        if errors.len() < MAX_DISPLAY_ERRORS {
                            errors.push(format!("文章 #{id}: 渲染异常"));
                        }
                        continue;
                    }
                };

            match client
                .execute(
                    "UPDATE posts SET content_html = $1, toc_html = $2, word_count = $3, reading_time = $4 \
                     WHERE id = $5 AND updated_at = $6 AND deleted_at IS NULL",
                    &[
                        &content_html,
                        &toc_html,
                        &word_count,
                        &reading_time,
                        &id,
                        &updated_at,
                    ],
                )
                .await
            {
                Ok(1) => rebuilt += 1,
                // 渲染期间文章被编辑或删除：其写路径已自行渲染，跳过不计数。
                Ok(_) => {}
                Err(e) => {
                    failed += 1;
                    if errors.len() < MAX_DISPLAY_ERRORS {
                        errors.push(format!("文章 #{id}: DB 写入失败，已中止"));
                    }
                    tracing::error!("rebuild UPDATE 失败，中止本批: {:?}", e);
                    aborted = true;
                    break;
                }
            }
        }

        let next_after_id = if !aborted && rows.len() as i64 == REBUILD_BATCH_LIMIT {
            rows.last().map(|r| r.get::<_, i32>(0))
        } else {
            None
        };

        // 重建会修改 word_count / reading_time 等列表项字段，批量影响列表、标签云、
        // 标签文章及单篇缓存；这里使用全量失效作为务实的回退策略。
        if rebuilt > 0 {
            crate::cache::invalidate_all_post_caches();
            crate::cache::invalidate_search_results();
            // 批量重建影响所有公开页（列表/标签/归档/各文章），全量失效 SSR 磁盘缓存。
            crate::ssr_cache::invalidate_ssr_all_public();
            crate::ssr_cache::bump_global_generation();
        }

        Ok(RebuildResult {
            rebuilt,
            failed,
            errors,
            next_after_id,
        })
    }

    #[cfg(not(feature = "server"))]
    {
        let _ = (rebuild_all, after_id);
        Ok(RebuildResult {
            rebuilt: 0,
            failed: 0,
            errors: vec![],
            next_after_id: None,
        })
    }
}

/// 重新渲染指定文章的 content_html 与 toc_html。
///
/// 从数据库读取 content_md（`FOR UPDATE` 锁行，避免并发编辑产生非可重复读），
/// 在阻塞线程池渲染 HTML，并更新 content_html / toc_html / word_count / reading_time。
/// 仅 admin 可调用；成功后按影响范围失效文章列表、slug 单篇与搜索缓存。
#[server(RebuildPostContentHtml, endpoint = "rebuild_post_content_html", input = leptos::server_fn::codec::Json)]
pub async fn rebuild_post_content_html(post_id: i32) -> Result<CreatePostResponse, ApiError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let mut client = get_conn().await.map_err(AppError::db_conn)?;
        let tx = client.transaction().await.map_err(AppError::tx)?;

        // 在事务内锁行并读取 content_md / slug，避免并发更新导致用旧内容覆盖新内容
        // 或缓存失效目标过期（与 delete_post 一致的事务策略）。
        let row = tx
            .query_opt(
                "SELECT content_md, slug FROM posts WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
                &[&post_id],
            )
            .await
            .map_err(AppError::query)?;

        let Some(row) = row else {
            return Ok(CreatePostResponse::err("文章不存在".to_string()));
        };

        let content_md: String = row.get(0);
        let slug: String = row.get(1);

        // Markdown 渲染 + 度量派生收敛到 helper（R4）。
        let (content_html, toc_html, word_count, reading_time) =
            render_post_fields_minimal(&content_md).await?;

        tx.execute(
            "UPDATE posts SET content_html = $1, toc_html = $2, word_count = $3, reading_time = $4 WHERE id = $5",
            &[
                &content_html,
                &toc_html,
                &word_count,
                &reading_time,
                &post_id,
            ],
        )
        .await
        .map_err(AppError::query)?;

        tx.commit().await.map_err(AppError::tx)?;

        // 重建修改了 word_count / reading_time 等列表项字段以及单篇 content_html，
        // 按影响范围精准失效（标签与统计不受影响）。
        crate::cache::invalidate_post_lists();
        crate::cache::invalidate_search_results();
        crate::cache::invalidate_post_by_slug(&slug).await;
        // SSR：单篇详情页 + 列表页（首页/分页/归档/标签都含列表项摘要）。
        crate::ssr_cache::invalidate_ssr_route(&format!("/post/{slug}"));
        crate::ssr_cache::invalidate_post_preview(&slug);
        crate::ssr_cache::invalidate_ssr_all_public();
        crate::ssr_cache::bump_global_generation();

        Ok(CreatePostResponse::ok(
            "重建成功".to_string(),
            post_id,
            slug,
        ))
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(CreatePostResponse::err("server only".to_string()))
    }
}
