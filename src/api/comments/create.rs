//! 发表评论接口。
//!
//! 校验作者信息、父评论与目标文章，生成内容哈希防止重复提交。
//! 匿名评论默认进入 pending 状态等待审核；**登录用户**（有效会话）发表的评论
//! 以账号身份直发 approved——身份字段取自 users 表（显示名回退用户名），
//! 跳过昵称/邮箱/网址校验与蜜罐，记录 `user_id` 供读取侧 JOIN 实时展示。
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
        use crate::api::auth::get_user_by_token;
        use crate::api::comments::helpers::{
            compute_content_hash, validate_comment_content, validate_comment_email,
            validate_comment_honeypot, validate_comment_name, validate_comment_url,
        };
        use crate::api::error::AppError;
        use crate::auth::session::get_session_from_ctx;
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

        // 登录用户识别：有效会话 → 以账号身份发表（直发 approved）；
        // 匿名/会话失效 → 原审核流程。会话校验失败不报错，按匿名兜底。
        let session_user = match get_session_from_ctx() {
            Some(token) => get_user_by_token(&token).await.map_err(AppError::query)?,
            None => None,
        };

        if session_user.is_none() {
            // 蜜罐字段二次校验：禁用 JS 的机器人可能绕过前端拦截，这里作为服务端防线。
            // 登录表单不渲染蜜罐，登录用户跳过。
            if let Err(e) = validate_comment_honeypot(&honeypot) {
                return Ok(CommentResponse::error("spam_detected", e));
            }

            // 依次校验昵称、邮箱与网址（登录用户的身份字段取自账号，无需校验）。
            if let Err(e) = validate_comment_name(&author_name) {
                return Ok(CommentResponse::error("invalid_input", e));
            }
            if let Err(e) = validate_comment_email(&author_email) {
                return Ok(CommentResponse::error("invalid_input", e));
            }
            if let Some(url) = &author_url {
                if let Err(e) = validate_comment_url(url) {
                    return Ok(CommentResponse::error("invalid_input", e));
                }
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

        // 身份字段定稿：登录用户取账号值（显示名回退用户名、无个人网址），
        // 匿名取表单值。查重哈希的作者键对用户用稳定 "user:<id>"（改名不影响查重）。
        let (final_name, final_email, final_url, user_id) = match &session_user {
            Some(u) => (
                u.display_name.clone().unwrap_or_else(|| u.username.clone()),
                u.email.clone(),
                None,
                Some(u.id),
            ),
            None => (
                author_name.clone(),
                author_email.clone(),
                author_url.clone(),
                None,
            ),
        };
        let author_key = match user_id {
            Some(id) => format!("user:{id}"),
            None => final_name.clone(),
        };

        // 基于文章、父评论、作者与内容计算哈希，防止短时间重复提交。
        let content_hash = compute_content_hash(post_id, parent_id, &author_key, &content_md);

        // Markdown 渲染（含 syntect 高亮 + KaTeX）是 CPU 密集任务，移到阻塞线程池执行，
        // 避免阻塞 async runtime（M4）。content_md 下游 INSERT 仍需使用，故先克隆再移入闭包。
        let md_for_render = content_md.clone();
        let content_html = tokio::task::spawn_blocking(move || {
            crate::api::comments::markdown::render_comment_markdown(&md_for_render)
        })
        .await
        .map_err(|_| AppError::Internal("Markdown 渲染任务失败"))?;
        let author_name_safe = crate::utils::html::escape_html(final_name.trim());
        let author_url_safe = final_url
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

        // 登录用户直发 approved（记录 approved_at）；匿名进入 pending 等待审核。
        let initial_status: &str = if session_user.is_some() {
            "approved"
        } else {
            "pending"
        };
        let approved_at: Option<chrono::DateTime<chrono::Utc>> =
            session_user.is_some().then(chrono::Utc::now);

        let row = tx
            .query_one(
                "INSERT INTO comments \
                 (post_id, parent_id, depth, author_name, author_email, author_url, \
                  content_md, content_html, content_hash, status, ip_address, user_agent, \
                  user_id, approved_at) \
                  VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
                  RETURNING id",
                &[
                    &post_id,
                    &parent_id,
                    &depth,
                    &author_name_safe,
                    &final_email.trim(),
                    &author_url_safe,
                    &content_md,
                    &content_html,
                    &content_hash,
                    &initial_status,
                    &ip_address,
                    &user_agent,
                    &user_id,
                    &approved_at,
                ],
            )
            .await
            .map_err(AppError::query)?;

        tx.commit().await.map_err(AppError::query)?;

        let comment_id: i64 = row.get(0);

        // 响应头像：登录用户用账号头像（未设置回退邮箱 Gravatar），匿名用邮箱 Gravatar。
        let avatar_url = match &session_user {
            Some(u) => u
                .avatar_url
                .clone()
                .unwrap_or_else(|| crate::api::comments::helpers::gravatar_url(&u.email)),
            None => crate::api::comments::helpers::gravatar_url(&final_email),
        };

        // 新评论影响文章评论列表缓存；仅匿名 pending 评论才影响待审核计数。
        cache::invalidate_comments_by_post(post_id).await;
        if session_user.is_none() {
            cache::invalidate_pending_count().await;
        }

        let message = if session_user.is_some() {
            "评论已发布"
        } else {
            "评论已提交，等待审核"
        };

        Ok(CommentResponse::created(
            message.to_string(),
            comment_id,
            avatar_url,
            depth,
        ))
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}
