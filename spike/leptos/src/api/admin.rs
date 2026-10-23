//! 后台评论管理接口：评论列表与各状态条数、单条审核与批量更新、回收站恢复。
//!
//! 所有接口均需管理员身份（`require_admin`）；端点挂在 `/api/admin/comments/...`。
//! 改变公开页面的写操作成功后清空 SSR 页面缓存（对应主应用的评论缓存失效）。

use leptos::prelude::*;
use leptos::server_fn::codec::Json;
use serde::{Deserialize, Serialize};

use crate::api::comments::CommentResponse;
use crate::error::AppError;
use crate::models::comment::AdminComment;

/// 全部评论列表响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllCommentsResponse {
    /// 评论列表。
    pub comments: Vec<AdminComment>,
    /// 当前筛选下的总数（分页用）。
    pub total: i64,
    /// 各状态的条数（选项卡数量）。
    #[serde(default)]
    pub counts: CommentCounts,
}

/// 各状态的评论条数；回收站按软删除计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentCounts {
    pub pending: i64,
    pub approved: i64,
    pub spam: i64,
    pub trash: i64,
}

/// 批量更新状态响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStatusResponse {
    /// 操作是否成功。
    pub success: bool,
    /// 实际更新的行数。
    pub updated_count: i64,
    /// 提示信息。
    pub message: String,
}

/// 将带作者 JOIN 的评论行转换为后台评论结构。
#[cfg(feature = "ssr")]
fn row_to_admin_comment(row: &tokio_postgres::Row) -> AdminComment {
    use crate::api::comments::resolve_author_display;
    use crate::models::comment::CommentStatus;

    let status_str: String = row.get("status");
    let email: String = row.get("author_email");
    let snapshot_name: String = row.get("author_name");
    let user_id: Option<i32> = row.get("user_id");
    let user_display_name: Option<String> = row.get("user_display_name");
    let user_avatar: Option<String> = row.get("user_avatar");
    let (author_name, avatar_url, _) = resolve_author_display(
        &snapshot_name,
        &email,
        user_id,
        user_display_name.as_deref(),
        user_avatar.as_deref(),
    );

    AdminComment {
        id: row.get("id"),
        post_id: row.get("post_id"),
        post_title: row.get("post_title"),
        post_slug: row.get("post_slug"),
        parent_id: row.get("parent_id"),
        reply_to: row.try_get("parent_author").ok().flatten(),
        depth: row.get("depth"),
        author_name,
        author_email: email,
        author_url: row.get("author_url"),
        avatar_url,
        content_md: row.get("content_md"),
        status: CommentStatus::from_str(&status_str),
        created_at: row.get("created_at"),
    }
}

/// 获取全部评论分页列表。
///
/// 支持按状态筛选（`trash` 为软删除的评论）；未指定状态时返回所有未删除评论。
/// 同时返回各状态条数。
#[server(endpoint = "admin/comments/list", input = Json)]
pub async fn get_all_comments(
    page: i32,
    status: Option<String>,
) -> Result<AllCommentsResponse, AppError> {
    use crate::api::auth::require_admin;
    use crate::server::db::{get_conn, query_err};

    let _admin = require_admin().await?;

    let page = page.max(1);
    let per_page: i64 = 20;
    let offset: i64 = (page as i64 - 1) * per_page;

    let client = get_conn().await?;

    // 回收站按软删除筛选；其余状态只看未删除的评论；不传状态时为全部未删除评论。
    let status = status.filter(|s| !s.is_empty());
    let trash = status.as_deref() == Some("trash");
    // 两种条件都引用 $1，参数个数保持一致；回收站传 NULL。
    let where_clause = if trash {
        "c.deleted_at IS NOT NULL AND ($1::text IS NULL OR c.status = $1)"
    } else {
        "c.deleted_at IS NULL AND ($1::text IS NULL OR c.status = $1)"
    };
    let status_param: Option<&str> = if trash { None } else { status.as_deref() };
    let total: i64 = client
        .query_one(
            &format!("SELECT COUNT(*) FROM comments c WHERE {where_clause}"),
            &[&status_param],
        )
        .await
        .map_err(query_err)?
        .get(0);
    let rows = client
        .query(
            &format!(
                "SELECT c.id, c.post_id, c.parent_id, c.depth, c.author_name, c.author_email, \
                        c.author_url, c.content_md, c.status, c.created_at, \
                        c.user_id, u.display_name AS user_display_name, u.avatar_url AS user_avatar, \
                        COALESCE(pu.display_name, pc.author_name) AS parent_author, \
                        p.title as post_title, p.slug as post_slug \
                 FROM comments c JOIN posts p ON c.post_id = p.id \
                 LEFT JOIN users u ON c.user_id = u.id \
                 LEFT JOIN comments pc ON pc.id = c.parent_id \
                 LEFT JOIN users pu ON pc.user_id = pu.id \
                 WHERE {where_clause} \
                 ORDER BY c.created_at DESC LIMIT $2 OFFSET $3"
            ),
            &[&status_param, &per_page, &offset],
        )
        .await
        .map_err(query_err)?;
    let counts = client
        .query_one(
            "SELECT COUNT(*) FILTER (WHERE deleted_at IS NULL AND status = 'pending'), \
                    COUNT(*) FILTER (WHERE deleted_at IS NULL AND status = 'approved'), \
                    COUNT(*) FILTER (WHERE deleted_at IS NULL AND status = 'spam'), \
                    COUNT(*) FILTER (WHERE deleted_at IS NOT NULL) \
             FROM comments",
            &[],
        )
        .await
        .map_err(query_err)?;
    let counts = CommentCounts {
        pending: counts.get(0),
        approved: counts.get(1),
        spam: counts.get(2),
        trash: counts.get(3),
    };

    let comments = rows.iter().map(row_to_admin_comment).collect();

    Ok(AllCommentsResponse {
        comments,
        total,
        counts,
    })
}

/// 通过指定评论。
///
/// 同时递归将该评论的所有 pending 父评论一并通过，确保嵌套链可见。
#[server(endpoint = "admin/comments/approve")]
pub async fn approve_comment(id: i64) -> Result<CommentResponse, AppError> {
    use crate::api::auth::require_admin;
    use crate::server::db::{get_conn, query_err};

    let _admin = require_admin().await?;
    let client = get_conn().await?;

    let row = client
        .query_opt(
            "SELECT post_id FROM comments WHERE id = $1 AND deleted_at IS NULL",
            &[&id],
        )
        .await
        .map_err(query_err)?;
    if row.is_none() {
        return Ok(CommentResponse::error(
            "not_found",
            "评论不存在".to_string(),
        ));
    }

    // 直接通过目标评论并记录通过时间。
    client
        .execute(
            "UPDATE comments SET status = 'approved', approved_at = NOW() WHERE id = $1",
            &[&id],
        )
        .await
        .map_err(query_err)?;

    // 递归向上查找所有 pending 父评论并同步通过，避免子评论可见但父评论被隐藏。
    client
        .execute(
            "WITH RECURSIVE ancestors AS ( \
                 SELECT parent_id FROM comments WHERE id = $1 \
                 UNION ALL \
                 SELECT c.parent_id FROM comments c JOIN ancestors a ON c.id = a.parent_id WHERE a.parent_id IS NOT NULL \
             ) \
             UPDATE comments SET status = 'approved', approved_at = NOW() \
             WHERE id IN (SELECT parent_id FROM ancestors WHERE parent_id IS NOT NULL) AND status = 'pending'",
            &[&id],
        )
        .await
        .map_err(query_err)?;

    crate::server::ssr_cache::invalidate_all();
    Ok(CommentResponse::ok("已通过".to_string()))
}

/// 将指定评论标记为垃圾评论。
///
/// 若原状态为 approved，则需要清空公开页面缓存。
#[server(endpoint = "admin/comments/spam")]
pub async fn spam_comment(id: i64) -> Result<CommentResponse, AppError> {
    use crate::api::auth::require_admin;
    use crate::server::db::{get_conn, query_err};

    let _admin = require_admin().await?;
    let client = get_conn().await?;

    let row = client
        .query_opt(
            "SELECT post_id, status FROM comments WHERE id = $1 AND deleted_at IS NULL",
            &[&id],
        )
        .await
        .map_err(query_err)?;
    // 与主应用一致：评论不存在时同样返回成功。
    if let Some(r) = row {
        let old_status: String = r.get("status");
        client
            .execute(
                "UPDATE comments SET status = 'spam' WHERE id = $1 AND deleted_at IS NULL",
                &[&id],
            )
            .await
            .map_err(query_err)?;
        if old_status == "approved" {
            crate::server::ssr_cache::invalidate_all();
        }
    }

    Ok(CommentResponse::ok("已标记为垃圾".to_string()))
}

/// 将指定评论移入回收站（软删除）。
///
/// 软删除会设置 deleted_at 与状态为 trash，并清空相关缓存。
#[server(endpoint = "admin/comments/trash")]
pub async fn trash_comment(id: i64) -> Result<CommentResponse, AppError> {
    use crate::api::auth::require_admin;
    use crate::server::db::{get_conn, query_err};

    let _admin = require_admin().await?;
    let client = get_conn().await?;

    let row = client
        .query_opt(
            "SELECT post_id FROM comments WHERE id = $1 AND deleted_at IS NULL",
            &[&id],
        )
        .await
        .map_err(query_err)?;
    if row.is_some() {
        client
            .execute(
                "UPDATE comments SET status = 'trash', deleted_at = NOW() WHERE id = $1",
                &[&id],
            )
            .await
            .map_err(query_err)?;
        crate::server::ssr_cache::invalidate_all();
    }

    Ok(CommentResponse::ok("已删除".to_string()))
}

/// 批量更新评论状态。
///
/// 仅接受 approved / spam / trash 三种状态；trash 会软删除并设置 deleted_at，
/// approved 会设置 approved_at。
#[server(endpoint = "admin/comments/batch_status", input = Json)]
pub async fn batch_update_comment_status(
    ids: Vec<i64>,
    status: String,
) -> Result<BatchStatusResponse, AppError> {
    use crate::api::auth::require_admin;
    use crate::server::db::{get_conn, query_err};

    let _admin = require_admin().await?;

    // 限制可批量操作的状态，防止非法状态写入数据库。
    if !matches!(status.as_str(), "approved" | "spam" | "trash") {
        return Ok(BatchStatusResponse {
            success: false,
            updated_count: 0,
            message: "无效的状态".to_string(),
        });
    }

    let client = get_conn().await?;

    // 根据目标状态设置不同的附加字段：trash 软删除，approved 记录通过时间。
    let sql = if status == "trash" {
        "UPDATE comments SET status = $1, deleted_at = NOW() WHERE id = ANY($2)"
    } else if status == "approved" {
        "UPDATE comments SET status = $1, approved_at = NOW() WHERE id = ANY($2)"
    } else {
        "UPDATE comments SET status = $1 WHERE id = ANY($2)"
    };
    let result = client
        .execute(sql, &[&status, &ids])
        .await
        .map_err(query_err)?;

    crate::server::ssr_cache::invalidate_all();

    Ok(BatchStatusResponse {
        success: true,
        updated_count: result as i64,
        message: format!("已更新 {} 条评论", result),
    })
}

/// 从回收站恢复评论：清除软删除并回到待审，由管理员重新决定是否通过。
#[server(endpoint = "admin/comments/restore", input = Json)]
pub async fn restore_comments(ids: Vec<i64>) -> Result<BatchStatusResponse, AppError> {
    use crate::api::auth::require_admin;
    use crate::server::db::{get_conn, query_err};

    let _admin = require_admin().await?;
    let client = get_conn().await?;
    let rows = client
        .query(
            "UPDATE comments SET deleted_at = NULL, status = 'pending' \
             WHERE id = ANY($1) AND deleted_at IS NOT NULL RETURNING post_id",
            &[&ids],
        )
        .await
        .map_err(query_err)?;
    crate::server::ssr_cache::invalidate_all();
    Ok(BatchStatusResponse {
        success: true,
        updated_count: rows.len() as i64,
        message: format!("已恢复 {} 条评论，等待审核", rows.len()),
    })
}
