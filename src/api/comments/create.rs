//! 发表评论接口。
//!
//! 校验作者信息、父评论与目标文章，生成内容哈希防止重复提交，
//! 新评论默认进入 pending 状态等待审核。
//! Dioxus server function，注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中写入数据库。

use crate::api::comments::types::*;
use dioxus::prelude::*;

/// 创建一条新评论。
///
/// 对作者昵称、邮箱、网址与内容进行基础校验；
/// 若目标文章未发布或父评论未通过审核，则拒绝提交；
/// 成功后将评论置为 pending，并清空相关缓存。
#[server(CreateComment, "/api")]
pub async fn create_comment(
    post_id: i32,
    parent_id: Option<i64>,
    author_name: String,
    author_email: String,
    author_url: Option<String>,
    content_md: String,
    honeypot: String,
) -> Result<CommentResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::comments::helpers::{
            compute_content_hash, validate_comment_content, validate_comment_email,
            validate_comment_honeypot, validate_comment_name, validate_comment_url,
        };
        use crate::api::error::AppError;
        use crate::cache;
        use crate::db::pool::get_conn;

        // 从 FullstackContext 获取客户端 IP，并进行评论频率限流。
        if let Some(ctx) = dioxus::fullstack::FullstackContext::current() {
            let headers = ctx.parts_mut().headers.clone();
            let ip = crate::api::rate_limit::get_client_ip(&headers).await;
            if let Err(msg) = crate::api::rate_limit::check_comment_limit(&ip) {
                return Ok(CommentResponse::error("rate_limited", msg));
            }
        }

        // 蜜罐字段二次校验：禁用 JS 的机器人可能绕过前端拦截，这里作为服务端防线。
        if let Err(e) = validate_comment_honeypot(&honeypot) {
            return Ok(CommentResponse::error("spam_detected", e));
        }

        // 依次校验昵称、邮箱、网址与评论内容。
        if let Err(e) = validate_comment_name(&author_name) {
            return Ok(CommentResponse::error("invalid_input", e));
        }
        if let Err(e) = validate_comment_email(&author_email) {
            return Ok(CommentResponse::error("invalid_input", e));
        }
        if let Some(ref url) = author_url {
            if let Err(e) = validate_comment_url(url) {
                return Ok(CommentResponse::error("invalid_input", e));
            }
        }
        if let Err(e) = validate_comment_content(&content_md) {
            return Ok(CommentResponse::error("invalid_input", e));
        }

        let mut client = get_conn().await.map_err(AppError::db_conn)?;

        // 确认目标文章存在且处于已发布状态。
        let post_row = client
            .query_opt(
                "SELECT status, deleted_at FROM posts WHERE id = $1",
                &[&post_id],
            )
            .await
            .map_err(AppError::query)?;

        match post_row {
            None => {
                return Ok(CommentResponse::error(
                    "post_not_found",
                    "文章不存在".to_string(),
                ));
            }
            Some(row) => {
                let status: String = row.get("status");
                let deleted_at: Option<chrono::DateTime<chrono::Utc>> = row.get("deleted_at");
                if status != "published" || deleted_at.is_some() {
                    return Ok(CommentResponse::error(
                        "post_not_found",
                        "文章不存在".to_string(),
                    ));
                }
            }
        }

        // 若存在父评论，校验其归属文章与审核状态，并计算当前评论的嵌套深度。
        let mut depth: i32 = 0;
        if let Some(pid) = parent_id {
            let parent_row = client
                .query_opt(
                    "SELECT post_id, status, depth FROM comments WHERE id = $1 AND deleted_at IS NULL",
                    &[&pid],
                )
                .await
                .map_err(AppError::query)?;

            match parent_row {
                None => {
                    return Ok(CommentResponse::error(
                        "parent_not_found",
                        "父评论不存在".to_string(),
                    ));
                }
                Some(row) => {
                    let parent_post_id: i32 = row.get("post_id");
                    let parent_status: String = row.get("status");
                    let parent_depth: i32 = row.get("depth");

                    if parent_post_id != post_id {
                        return Ok(CommentResponse::error(
                            "parent_not_found",
                            "父评论不存在".to_string(),
                        ));
                    }
                    if parent_status != "approved" {
                        return Ok(CommentResponse::error(
                            "parent_not_approved",
                            "父评论未通过审核".to_string(),
                        ));
                    }

                    depth = parent_depth + 1;
                    if depth > 20 {
                        return Ok(CommentResponse::error(
                            "too_deep",
                            "评论嵌套层级过深".to_string(),
                        ));
                    }
                }
            }
        }

        // 基于文章、父评论、作者与内容计算哈希，防止短时间重复提交。
        let content_hash = compute_content_hash(post_id, parent_id, &author_name, &content_md);

        // Markdown 渲染（含 syntect 高亮 + KaTeX）是 CPU 密集任务，移到阻塞线程池执行，
        // 避免阻塞 async runtime（M4）。content_md 下游 INSERT 仍需使用，故先克隆再移入闭包。
        let md_for_render = content_md.clone();
        let content_html = tokio::task::spawn_blocking(move || {
            crate::api::comments::markdown::render_comment_markdown(&md_for_render)
        })
        .await
        .map_err(|_| AppError::Internal("Markdown 渲染任务失败"))?;
        let author_name_safe = crate::utils::html::escape_html(author_name.trim());
        let author_url_safe = author_url
            .as_ref()
            .map(|u| crate::utils::html::escape_html(u.trim()))
            .filter(|u| !u.is_empty());
        let ip_address = if let Some(ctx) = dioxus::fullstack::FullstackContext::current() {
            let headers = ctx.parts_mut().headers.clone();
            Some(crate::api::rate_limit::get_client_ip(&headers).await)
        } else {
            None
        };
        let user_agent = if let Some(ctx) = dioxus::fullstack::FullstackContext::current() {
            let parts = ctx.parts_mut();
            parts
                .headers
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        } else {
            None
        };

        // 查重与插入在同一事务内，并用 advisory lock 串行化相同内容的并发提交（M4）。
        // 仅靠普通 SELECT+事务在 Read Committed 下无法阻止并发重复（两个事务都看不到
        // 对方未提交的 INSERT）；pg_advisory_xact_lock 以内容哈希派生的 key 加事务级
        // 排他锁，使相同内容的并发请求在锁上排队，第二个提交时查重必然命中前一个。
        // key 取 content_hash 前 16 个 hex 字符（8 字节）解析为 i64。
        let lock_key: i64 = i64::from_str_radix(&content_hash[..16], 16).unwrap_or(0);

        let tx = client.transaction().await.map_err(AppError::query)?;
        // 事务级 advisory 锁：随事务结束自动释放，无需显式 unlock。
        tx.execute("SELECT pg_advisory_xact_lock($1)", &[&lock_key])
            .await
            .map_err(AppError::query)?;

        let dup: Option<i64> = tx
            .query_opt(
                "SELECT id FROM comments WHERE post_id = $1 AND content_hash = $2 AND created_at > NOW() - INTERVAL '5 minutes'",
                &[&post_id, &content_hash],
            )
            .await
            .map_err(AppError::query)?
            .map(|r| r.get(0));

        if dup.is_some() {
            // 重复：回滚（释放 advisory 锁）后返回。
            tx.rollback().await.ok();
            return Ok(CommentResponse::error(
                "duplicate",
                "请勿重复提交".to_string(),
            ));
        }

        // 插入评论，默认状态为 pending，等待管理员审核。
        let row = tx
            .query_one(
                "INSERT INTO comments \
                 (post_id, parent_id, depth, author_name, author_email, author_url, \
                  content_md, content_html, content_hash, status, ip_address, user_agent) \
                  VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'pending', $10, $11) \
                  RETURNING id",
                &[
                    &post_id,
                    &parent_id,
                    &depth,
                    &author_name_safe,
                    &author_email.trim(),
                    &author_url_safe,
                    &content_md,
                    &content_html,
                    &content_hash,
                    &ip_address,
                    &user_agent,
                ],
            )
            .await
            .map_err(AppError::query)?;

        tx.commit().await.map_err(AppError::query)?;

        let comment_id: i64 = row.get(0);

        // 根据邮箱生成 Gravatar 头像链接。
        let avatar_url = crate::api::comments::helpers::gravatar_url(&author_email);

        // 新评论可能影响文章评论列表与待审核计数，清空相关缓存。
        cache::invalidate_comments_by_post(post_id).await;
        cache::invalidate_pending_count().await;

        Ok(CommentResponse::created(
            "评论已提交，等待审核".to_string(),
            comment_id,
            avatar_url,
            depth,
        ))
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}
