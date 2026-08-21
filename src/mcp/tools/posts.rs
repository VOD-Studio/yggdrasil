//! MCP 写作用域工具：文章 CRUD。
//!
//! 镜像 `src/api/posts/{create,update,trash,delete}.rs` 的 server-fn 逻辑，
//! 但用 bearer-token 鉴权（`principal.user_id` 作 author_id），不走 cookie。
//! 每个写操作后执行与 web 后台完全一致的缓存失效（moka + SSR）。
//!
//! 本模块仅 `feature = "server"` 编译；`server.rs` 在最终装配时把 `posts_router`
//! 组合进单一 `ServerHandler`。

#![cfg(feature = "server")]
#![allow(clippy::too_many_arguments)]

use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{schemars, tool, tool_router, ErrorData as McpError};
use serde::Deserialize;

use super::common::{internal, ok_json, require_scope};
use crate::cache;
use crate::db::pool::get_conn;
use crate::models::mcp_token::TokenScope;
use crate::models::post::PostStatus;
use crate::ssr_cache;

// ---------------------------------------------------------------------------
// 结构体
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 工具
// ---------------------------------------------------------------------------

#[tool_router(router = posts_router, vis = "pub")]
impl crate::mcp::server::YggMcpServer {
    /// 创建一篇新文章（草稿或直接发布）。要求 write 作用域。
    #[tool(
        description = "创建一篇新文章。渲染 Markdown 为 HTML，同步标签与素材引用。返回 post_id/slug。"
    )]
    async fn create_post(
        &self,
        Parameters(p): Parameters<CreatePostParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "create_post", TokenScope::Write)?;

        if p.title.trim().is_empty() {
            return Err(McpError::invalid_request("title must not be empty", None));
        }
        if p.content_md.trim().is_empty() {
            return Err(McpError::invalid_request(
                "content_md must not be empty",
                None,
            ));
        }

        // 确定基础 slug。
        let base_slug = match &p.slug {
            Some(s) if !s.trim().is_empty() => {
                let s = s.trim();
                if !crate::api::slug::is_valid_slug(s) {
                    return Err(McpError::invalid_request(
                        "slug 格式无效，只能包含字母、数字、连字符和下划线",
                        None,
                    ));
                }
                s.to_string()
            }
            _ => crate::api::slug::slugify(&p.title),
        };

        // Markdown 渲染 + 度量派生收敛到 helper（R4）。
        let fields = crate::api::posts::helpers::render_post_fields(
            &p.content_md,
            &p.status,
            p.cover_image.as_deref(),
        )
        .await
        .map_err(|_| internal("markdown render", "render_post_fields"))?;
        let summary = p
            .summary
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or(fields.auto_summary);
        let explicit_published_at = match &p.published_at {
            Some(s) if !s.trim().is_empty() => {
                let Some(dt) = parse_date_opt(s) else {
                    return Err(McpError::invalid_request(
                        "published_at 格式无效，支持 YYYY-MM-DD 或 ISO 8601",
                        None,
                    ));
                };
                Some(dt)
            }
            _ => None,
        };
        let published_at = if fields.status == PostStatus::Published {
            explicit_published_at.or_else(|| Some(chrono::Utc::now()))
        } else {
            None
        };
        let created_at = explicit_published_at.unwrap_or_else(chrono::Utc::now);

        let mut client = get_conn().await.map_err(|e| internal(e, "db connection"))?;
        let tx = client
            .transaction()
            .await
            .map_err(|e| internal(e, "begin txn"))?;

        let final_slug = crate::api::slug::ensure_unique_slug(&tx, &base_slug, None)
            .await
            .map_err(|e| internal(e, "ensure_unique_slug"))?;

        let row = tx
            .query_one(
                "INSERT INTO posts (author_id, title, slug, summary, content_md, content_html, toc_html, status, published_at, cover_image, word_count, reading_time, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 RETURNING id",
                &[
                    &principal.user_id,
                    &p.title.trim(),
                    &final_slug,
                    &summary,
                    &p.content_md,
                    &fields.content_html,
                    &fields.toc_html,
                    &fields.status.as_str(),
                    &published_at,
                    &fields.cover_image,
                    &fields.word_count,
                    &fields.reading_time,
                    &created_at,
                ],
            )
            .await
            .map_err(|e| internal(e, "insert post"))?;
        let post_id: i32 = row.get(0);

        let tags_cleaned = crate::api::posts::helpers::clean_tags(&p.tags);
        crate::api::posts::helpers::sync_tags(&tx, post_id, &tags_cleaned)
            .await
            .map_err(|_| internal("tag sync", "sync_tags"))?;
        crate::api::posts::helpers::sync_asset_refs(
            &tx,
            post_id,
            &fields.content_html,
            fields.cover_image.as_deref(),
        )
        .await
        .map_err(|_| internal("asset_refs sync", "sync_asset_refs"))?;

        tx.commit().await.map_err(|e| internal(e, "commit"))?;

        // 与 web 后台一致的缓存失效（moka + SSR）。
        cache::invalidate_for_post_write(std::slice::from_ref(&final_slug), &tags_cleaned).await;

        ok_json(PostResult {
            success: true,
            message: "创建成功".into(),
            post_id: Some(post_id),
            slug: Some(final_slug),
        })
    }

    /// 更新指定文章（PATCH 语义：仅更新提供的字段）。要求 write 作用域。
    /// 仅文章原作者可更新。
    #[tool(
        description = "部分更新一篇已有文章（PATCH 语义）。仅更新提供的字段：未提供 content_md 时跳过重新渲染；未提供 summary 时随 content_md 联动（自动提取或保留旧值）。仅文章原作者可更新。"
    )]
    async fn update_post(
        &self,
        Parameters(p): Parameters<UpdatePostParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "update_post", TokenScope::Write)?;

        use tokio_postgres::types::ToSql;

        // 至少一个可更新字段。
        let any_change = p.title.is_some()
            || p.content_md.is_some()
            || p.summary.is_some()
            || p.slug.is_some()
            || p.tags.is_some()
            || p.status.is_some()
            || p.cover_image.is_some()
            || p.published_at.is_some();
        if !any_change {
            return Err(McpError::invalid_request("至少提供一个可更新字段", None));
        }
        // 提供时的非空校验。
        if matches!(&p.title, Some(t) if t.trim().is_empty()) {
            return Err(McpError::invalid_request("title must not be empty", None));
        }
        if matches!(&p.content_md, Some(c) if c.trim().is_empty()) {
            return Err(McpError::invalid_request(
                "content_md must not be empty",
                None,
            ));
        }

        let mut client = get_conn().await.map_err(|e| internal(e, "db connection"))?;
        let tx = client
            .transaction()
            .await
            .map_err(|e| internal(e, "begin txn"))?;

        // 校验存在、未删除、归属，并取旧值（slug/status/published_at/cover）。
        let old_row = tx
            .query_opt(
                "SELECT slug, status, published_at, cover_image FROM posts \
                 WHERE id = $1 AND author_id = $2 AND deleted_at IS NULL",
                &[&p.post_id, &principal.user_id],
            )
            .await
            .map_err(|e| internal(e, "select post"))?;
        let Some(old_row) = old_row else {
            return Err(McpError::invalid_request("文章不存在或无权限", None));
        };
        let old_slug: String = old_row.get(0);
        let old_status: String = old_row.get(1);
        let old_published_at: Option<chrono::DateTime<chrono::Utc>> = old_row.get(2);
        let old_cover: Option<String> = old_row.get(3);

        // 渲染（仅当 content_md 提供时）。status/cover 用新值或回退旧值。
        let rendered: Option<crate::api::posts::helpers::RenderedFields> = match &p.content_md {
            Some(md) => {
                let status_for_render = p.status.as_deref().unwrap_or(&old_status);
                let cover_for_render: Option<&str> = p
                    .cover_image
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .or(old_cover.as_deref());
                Some(
                    crate::api::posts::helpers::render_post_fields(
                        md,
                        status_for_render,
                        cover_for_render,
                    )
                    .await
                    .map_err(|_| internal("markdown render", "render_post_fields"))?,
                )
            }
            None => None,
        };

        // summary 决策（随 content_md 联动）：
        //  提供了 summary → 用之（空则回退自动提取）；未提供但 content_md 变了 → 自动提取；都未变 → 不动。
        let summary_value: Option<String> = match (&p.summary, &rendered) {
            (Some(s), r) => {
                let t = s.trim();
                if t.is_empty() {
                    r.as_ref().map(|f| f.auto_summary.clone())
                } else {
                    Some(t.to_string())
                }
            }
            (None, Some(f)) => Some(f.auto_summary.clone()),
            (None, None) => None,
        };

        // slug 决策（仅显式提供才动；空串视为未提供）。
        let new_slug: Option<String> = match &p.slug {
            Some(s) => {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else if !crate::api::slug::is_valid_slug(t) {
                    return Err(McpError::invalid_request("slug 格式无效", None));
                } else {
                    Some(t.to_string())
                }
            }
            None => None,
        };
        let final_slug: Option<String> = match new_slug {
            Some(base) => Some(
                crate::api::slug::ensure_unique_slug(&tx, &base, Some(p.post_id))
                    .await
                    .map_err(|e| internal(e, "ensure_unique_slug"))?,
            ),
            None => None,
        };

        // published_at 显式指定或依据 status 转换联动。
        let explicit_published_at = match &p.published_at {
            Some(s) if !s.trim().is_empty() => {
                let Some(dt) = parse_date_opt(s) else {
                    return Err(McpError::invalid_request(
                        "published_at 格式无效，支持 YYYY-MM-DD 或 ISO 8601",
                        None,
                    ));
                };
                Some(Some(dt))
            }
            Some(_) => Some(None), // 空串清空 published_at
            None => None,
        };

        // status + published_at 决策（首发 published 填 published_at；转 draft 保留旧值）。
        let new_status: Option<PostStatus> = p
            .status
            .as_deref()
            .map(|s| PostStatus::from_str(s).unwrap_or(PostStatus::Draft));
        let published_at: Option<Option<chrono::DateTime<chrono::Utc>>> =
            if let Some(epa) = explicit_published_at {
                Some(epa)
            } else {
                match &new_status {
                    Some(PostStatus::Published) => Some(if old_status == "published" {
                        old_published_at
                    } else {
                        Some(chrono::Utc::now())
                    }),
                    Some(PostStatus::Draft) => Some(old_published_at),
                    None => None,
                }
            };

        // cover 决策（空串 → 清空 None）。
        let new_cover: Option<String> = p
            .cover_image
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let cover_changed = p.cover_image.is_some();

        // 动态构建 UPDATE（仅 SET 提供的字段）。
        let mut sets: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn ToSql + Sync + Send>> = Vec::new();
        let mut idx = 1usize;
        macro_rules! push {
            ($col:expr, $val:expr) => {{
                sets.push(format!("{} = ${}", $col, idx));
                params.push(Box::new($val));
                idx += 1;
            }};
        }
        if let Some(t) = &p.title {
            push!("title", t.trim().to_string());
        }
        if let Some(c) = &p.content_md {
            push!("content_md", c.clone());
        }
        if let Some(f) = &rendered {
            push!("content_html", f.content_html.clone());
            push!("toc_html", f.toc_html.clone());
            push!("word_count", f.word_count);
            push!("reading_time", f.reading_time);
        }
        if let Some(s) = &summary_value {
            push!("summary", s.clone());
        }
        if let Some(s) = &final_slug {
            push!("slug", s.clone());
        }
        if let Some(st) = &new_status {
            push!("status", st.as_str().to_string());
        }
        if let Some(pa) = published_at {
            push!("published_at", pa);
            if let Some(dt) = pa {
                push!("created_at", dt);
            }
        }
        if cover_changed {
            push!("cover_image", new_cover.clone());
        }
        sets.push("updated_at = NOW()".to_string());

        let sql = format!("UPDATE posts SET {} WHERE id = ${}", sets.join(", "), idx);
        params.push(Box::new(p.post_id));
        let refs: Vec<&(dyn ToSql + Sync)> = params
            .iter()
            .map(|b| b.as_ref() as &(dyn ToSql + Sync))
            .collect();
        let updated = tx
            .execute(&sql, &refs)
            .await
            .map_err(|e| internal(e, "update post"))?;
        if updated == 0 {
            return Err(McpError::invalid_request("文章不存在或无权限", None));
        }

        // 标签同步（仅当 tags 提供时）。先取旧标签供缓存失效，再完全替换。
        let tags_changed = p.tags.is_some();
        let mut old_tags: Vec<String> = Vec::new();
        if tags_changed {
            old_tags = crate::api::posts::helpers::fetch_post_tags(&tx, p.post_id)
                .await
                .map_err(|_| internal("select old tags", "fetch_post_tags"))?;
            let tags_cleaned = crate::api::posts::helpers::clean_tags(p.tags.as_ref().unwrap());
            tx.execute("DELETE FROM post_tags WHERE post_id = $1", &[&p.post_id])
                .await
                .map_err(|e| internal(e, "delete old post_tags"))?;
            crate::api::posts::helpers::sync_tags(&tx, p.post_id, &tags_cleaned)
                .await
                .map_err(|_| internal("tag sync", "sync_tags"))?;
        }

        // 素材引用同步（content_html 或 cover 变了）。
        if rendered.is_some() || cover_changed {
            let content_html: String = match &rendered {
                Some(f) => f.content_html.clone(),
                None => tx
                    .query_one(
                        "SELECT content_html FROM posts WHERE id = $1",
                        &[&p.post_id],
                    )
                    .await
                    .map_err(|e| internal(e, "select content_html"))?
                    .get::<_, String>(0),
            };
            let cover_for_sync = new_cover.as_deref().or(old_cover.as_deref());
            crate::api::posts::helpers::sync_asset_refs(
                &tx,
                p.post_id,
                &content_html,
                cover_for_sync,
            )
            .await
            .map_err(|_| internal("asset_refs sync", "sync_asset_refs"))?;
        }

        tx.commit().await.map_err(|e| internal(e, "commit"))?;

        // 缓存失效（moka + SSR）。
        let effective_slug = final_slug.clone().unwrap_or_else(|| old_slug.clone());

        cache::invalidate_post_metadata();
        cache::invalidate_post_by_slug(&effective_slug).await;

        if let Some(new) = &final_slug {
            if new != &old_slug {
                cache::invalidate_post_by_slug(&old_slug).await;
                ssr_cache::invalidate_ssr_route(&format!("/post/{old_slug}"));
                ssr_cache::invalidate_post_preview(&old_slug);
            }
        }
        ssr_cache::invalidate_ssr_route(&format!("/post/{effective_slug}"));
        ssr_cache::invalidate_post_preview(&effective_slug);
        ssr_cache::invalidate_ssr_all_public();
        ssr_cache::bump_global_generation();

        if tags_changed {
            let new_tags = crate::api::posts::helpers::clean_tags(p.tags.as_ref().unwrap());
            let mut all: std::collections::HashSet<String> = old_tags.into_iter().collect();
            all.extend(new_tags);
            let all_tags: Vec<String> = all.into_iter().collect();
            cache::invalidate_tag_posts_for(&all_tags).await;
        }

        ok_json(PostResult {
            success: true,
            message: "更新成功".into(),
            post_id: Some(p.post_id),
            slug: Some(effective_slug),
        })
    }

    /// 发布指定文章（设置 status=published 与 published_at）。要求 write 作用域。
    #[tool(
        description = "发布一篇草稿文章。设置 status=published，若首次发布则填充 published_at。"
    )]
    async fn publish_post(
        &self,
        Parameters(p): Parameters<PostIdParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "publish_post", TokenScope::Write)?;

        let client = get_conn().await.map_err(|e| internal(e, "db connection"))?;

        // 校验存在、未删除、归属当前用户，并取 slug 用于缓存失效。
        let row = client
            .query_opt(
                "SELECT slug FROM posts WHERE id = $1 AND author_id = $2 AND deleted_at IS NULL",
                &[&p.post_id, &principal.user_id],
            )
            .await
            .map_err(|e| internal(e, "select post"))?;
        let slug: String = match row {
            Some(r) => r.get(0),
            None => {
                return Err(McpError::invalid_request("文章不存在或无权限", None));
            }
        };

        // M6 修复：发布后文章出现在公开标签列表页，须失效标签缓存（api update.rs:202
        // 会失效，此 MCP 路径此前漏掉 → 标签页新发文陈旧 ≤120s）。
        let tags = crate::api::posts::helpers::fetch_post_tags(&client, p.post_id)
            .await
            .map_err(|_| internal("select tags", "select tags"))?;

        let result = client
            .execute(
                "UPDATE posts SET status = 'published', \
                 published_at = COALESCE(published_at, NOW()), updated_at = NOW() \
                 WHERE id = $1 AND deleted_at IS NULL",
                &[&p.post_id],
            )
            .await
            .map_err(|e| internal(e, "publish post"))?;
        if result == 0 {
            return Err(McpError::invalid_request("文章不存在", None));
        }

        // 发布后失效文章详情、列表、标签与 SSR 缓存（moka + SSR）。
        cache::invalidate_for_post_write(std::slice::from_ref(&slug), &tags).await;

        ok_json(PostResult {
            success: true,
            message: "发布成功".into(),
            post_id: Some(p.post_id),
            slug: Some(slug),
        })
    }

    /// 将指定文章移入回收站（软删除：设置 deleted_at）。要求 write 作用域。
    #[tool(description = "将文章移入回收站（软删除）。可通过恢复操作还原。")]
    async fn trash_post(
        &self,
        Parameters(p): Parameters<PostIdParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "trash_post", TokenScope::Write)?;

        let mut client = get_conn().await.map_err(|e| internal(e, "db connection"))?;
        let tx = client
            .transaction()
            .await
            .map_err(|e| internal(e, "begin txn"))?;

        let slug_row = tx
            .query_opt(
                "SELECT slug FROM posts WHERE id = $1 AND author_id = $2 AND deleted_at IS NULL FOR UPDATE",
                &[&p.post_id, &principal.user_id],
            )
            .await
            .map_err(|e| internal(e, "select post"))?;
        let Some(slug_row) = slug_row else {
            return Err(McpError::invalid_request("文章不存在", None));
        };
        let slug: String = slug_row.get(0);

        let tags = crate::api::posts::helpers::fetch_post_tags(&tx, p.post_id)
            .await
            .map_err(|_| internal("select tags", "select tags"))?;

        let result = tx
            .execute(
                "UPDATE posts SET deleted_at = NOW() WHERE id = $1 AND deleted_at IS NULL",
                &[&p.post_id],
            )
            .await
            .map_err(|e| internal(e, "soft delete"))?;
        if result == 0 {
            return Err(McpError::invalid_request("文章不存在", None));
        }

        tx.commit().await.map_err(|e| internal(e, "commit"))?;

        // 移入回收站后失效相关缓存（moka + SSR）。
        cache::invalidate_for_post_write(std::slice::from_ref(&slug), &tags).await;

        ok_json(PostResult {
            success: true,
            message: "已移入回收站".into(),
            post_id: Some(p.post_id),
            slug: Some(slug),
        })
    }

    /// 彻底删除指定文章（物理删除，不可恢复）。要求 write 作用域。
    #[tool(
        description = "彻底删除文章（物理删除，不可恢复）。post_tags 关联因外键 CASCADE 自动清理。"
    )]
    async fn delete_post(
        &self,
        Parameters(p): Parameters<PostIdParams>,
        Extension(parts): Extension<http::request::Parts>,
    ) -> Result<CallToolResult, McpError> {
        let principal = require_scope(&parts, "delete_post", TokenScope::Write)?;

        let mut client = get_conn().await.map_err(|e| internal(e, "db connection"))?;
        let tx = client
            .transaction()
            .await
            .map_err(|e| internal(e, "begin txn"))?;

        let slug_row = tx
            .query_opt(
                "SELECT slug FROM posts WHERE id = $1 AND author_id = $2 FOR UPDATE",
                &[&p.post_id, &principal.user_id],
            )
            .await
            .map_err(|e| internal(e, "select post"))?;
        let Some(slug_row) = slug_row else {
            return Err(McpError::invalid_request("文章不存在", None));
        };
        let slug: String = slug_row.get(0);

        let tags = crate::api::posts::helpers::fetch_post_tags(&tx, p.post_id)
            .await
            .map_err(|_| internal("select tags", "select tags"))?;

        let result = tx
            .execute("DELETE FROM posts WHERE id = $1", &[&p.post_id])
            .await
            .map_err(|e| internal(e, "hard delete"))?;
        if result == 0 {
            return Err(McpError::invalid_request("文章不存在", None));
        }

        tx.commit().await.map_err(|e| internal(e, "commit"))?;

        // 彻底删除后失效相关缓存（moka + SSR）。
        cache::invalidate_for_post_write(std::slice::from_ref(&slug), &tags).await;

        ok_json(PostResult {
            success: true,
            message: "已彻底删除".into(),
            post_id: Some(p.post_id),
            slug: Some(slug),
        })
    }
}

// ---------------------------------------------------------------------------
// 参数与输出结构
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreatePostParams {
    /// 文章标题（必填，非空）。
    pub title: String,
    /// Markdown 正文（必填，非空）。
    pub content_md: String,
    /// 摘要；未提供时自动从正文提取。
    #[serde(default)]
    pub summary: Option<String>,
    /// URL slug；未提供时从标题自动生成。
    #[serde(default)]
    pub slug: Option<String>,
    /// 标签列表。
    #[serde(default)]
    pub tags: Vec<String>,
    /// 状态：`draft`（默认）或 `published`。
    #[serde(default = "default_status")]
    pub status: String,
    /// 封面图 URL。
    #[serde(default)]
    pub cover_image: Option<String>,
    /// 发布时间（ISO 8601 或 YYYY-MM-DD）；仅在 status=published 时生效，未提供则使用当前时间。
    #[serde(default)]
    pub published_at: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdatePostParams {
    /// 要更新的文章 id。
    pub post_id: i32,
    /// 新标题。未提供则不修改（且不联动 slug）。
    #[serde(default)]
    pub title: Option<String>,
    /// 新 Markdown 正文。未提供则不重新渲染（content_html/toc/度量保持不变）。
    #[serde(default)]
    pub content_md: Option<String>,
    /// 新摘要。content_md 变化且未提供时自动从正文提取；content_md 未变且未提供则保留旧值。
    #[serde(default)]
    pub summary: Option<String>,
    /// 新 slug。未提供则不修改；提供则校验格式并自动去重。
    #[serde(default)]
    pub slug: Option<String>,
    /// 新标签列表。未提供则不修改；提供空列表则清空标签（完全替换旧标签）。
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// 新状态：`draft` / `published`。未提供则不修改。
    #[serde(default)]
    pub status: Option<String>,
    /// 新封面图 URL。未提供则不修改；空字符串清空封面。
    #[serde(default)]
    pub cover_image: Option<String>,
    /// 发布时间（ISO 8601 或 YYYY-MM-DD）；未提供则不修改。
    #[serde(default)]
    pub published_at: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PostIdParams {
    /// 文章 id。
    pub post_id: i32,
}

#[derive(Debug, serde::Serialize)]
struct PostResult {
    success: bool,
    message: String,
    post_id: Option<i32>,
    slug: Option<String>,
}

fn default_status() -> String {
    "draft".to_string()
}

fn parse_date_opt(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let s = s.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(nd) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(ndt) = nd.and_hms_opt(0, 0, 0) {
            return Some(chrono::DateTime::from_naive_utc_and_offset(
                ndt,
                chrono::Utc,
            ));
        }
    }
    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(chrono::DateTime::from_naive_utc_and_offset(
            ndt,
            chrono::Utc,
        ));
    }
    None
}
