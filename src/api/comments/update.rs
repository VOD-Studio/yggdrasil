//! 评论审核状态更新接口：通过、垃圾、删除与批量更新。
//!
//! 所有接口均需管理员身份，Leptos server function 注册在 `/api` 路径下。
//! 状态变更后需要清空文章评论缓存、计数缓存与待审核计数缓存。
//! 仅在 `feature = "server"` 启用的服务端构建中写入数据库。

use crate::api::comments::types::*;
use leptos::prelude::*;

/// 通过指定评论。
///
/// 同时递归将该评论的所有 pending 父评论一并通过，确保嵌套链可见。
#[server(ApproveComment, endpoint = "approve_comment", input = leptos::server_fn::codec::Json)]
pub async fn approve_comment(id: i64) -> Result<CommentResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;

        let _admin = get_current_admin_user().await?;
        if !approve_comment_impl(id).await? {
            return Ok(CommentResponse::error(
                "not_found",
                "评论不存在".to_string(),
            ));
        }

        Ok(CommentResponse::ok("已通过".to_string()))
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 将指定评论标记为垃圾评论。
///
/// 若原状态为 approved，则需要清空该文章相关缓存。
#[server(SpamComment, endpoint = "spam_comment", input = leptos::server_fn::codec::Json)]
pub async fn spam_comment(id: i64) -> Result<CommentResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;

        let _admin = get_current_admin_user().await?;
        spam_comment_impl(id).await?;

        Ok(CommentResponse::ok("已标记为垃圾".to_string()))
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 将指定评论移入回收站（软删除）。
///
/// 软删除会设置 deleted_at 与状态为 trash，并清空相关缓存。
#[server(TrashComment, endpoint = "trash_comment", input = leptos::server_fn::codec::Json)]
pub async fn trash_comment(id: i64) -> Result<CommentResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;

        let _admin = get_current_admin_user().await?;
        trash_comment_impl(id).await?;

        Ok(CommentResponse::ok("已删除".to_string()))
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 批量更新评论状态。
///
/// 仅接受 approved / spam / trash 三种状态；trash 会软删除并设置 deleted_at，
/// approved 会设置 approved_at。
#[server(BatchUpdateCommentStatus, endpoint = "batch_update_comment_status", input = leptos::server_fn::codec::Json)]
pub async fn batch_update_comment_status(
    ids: Vec<i64>,
    status: String,
) -> Result<BatchStatusResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;
        use crate::api::error::AppError;
        use crate::cache;
        use crate::db::pool::get_conn;

        let _admin = get_current_admin_user().await?;

        // 限制可批量操作的状态，防止非法状态写入数据库。
        if !matches!(status.as_str(), "approved" | "spam" | "trash") {
            return Ok(BatchStatusResponse {
                success: false,
                updated_count: 0,
                message: "无效的状态".to_string(),
            });
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;

        // 收集受影响的文章 id，用于后续批量失效缓存。
        let post_ids: Vec<i32> = client
            .query(
                "SELECT DISTINCT post_id FROM comments WHERE id = ANY($1)",
                &[&ids],
            )
            .await
            .map_err(AppError::query)?
            .iter()
            .map(|r| r.get("post_id"))
            .collect();

        // 根据目标状态设置不同的附加字段：trash 软删除，approved 记录通过时间。
        let result = if status == "trash" {
            client
                .execute(
                    "UPDATE comments SET status = $1, deleted_at = NOW() WHERE id = ANY($2)",
                    &[&status, &ids],
                )
                .await
                .map_err(AppError::query)?
        } else if status == "approved" {
            client
                .execute(
                    "UPDATE comments SET status = $1, approved_at = NOW() WHERE id = ANY($2)",
                    &[&status, &ids],
                )
                .await
                .map_err(AppError::query)?
        } else {
            client
                .execute(
                    "UPDATE comments SET status = $1 WHERE id = ANY($2)",
                    &[&status, &ids],
                )
                .await
                .map_err(AppError::query)?
        };

        cache::invalidate_pending_count().await;
        for pid in post_ids {
            cache::invalidate_comments_by_post(pid).await;
        }

        Ok(BatchStatusResponse {
            success: true,
            updated_count: result as i64,
            message: format!("已更新 {} 条评论", result),
        })
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 从回收站恢复评论：清除软删除并回到待审，由管理员重新决定是否通过。
#[server(RestoreComments, endpoint = "restore_comments", input = leptos::server_fn::codec::Json)]
pub async fn restore_comments(ids: Vec<i64>) -> Result<BatchStatusResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;
        use crate::api::error::AppError;
        use crate::cache;
        use crate::db::pool::get_conn;

        let _admin = get_current_admin_user().await?;
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let rows = client
            .query(
                "UPDATE comments SET deleted_at = NULL, status = 'pending' \
                 WHERE id = ANY($1) AND deleted_at IS NOT NULL RETURNING post_id",
                &[&ids],
            )
            .await
            .map_err(AppError::query)?;
        let mut post_ids: Vec<i32> = rows.iter().map(|r| r.get("post_id")).collect();
        post_ids.sort_unstable();
        post_ids.dedup();
        cache::invalidate_pending_count().await;
        for pid in post_ids {
            cache::invalidate_comments_by_post(pid).await;
        }
        Ok(BatchStatusResponse {
            success: true,
            updated_count: rows.len() as i64,
            message: format!("已恢复 {} 条评论，等待审核", rows.len()),
        })
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = ids;
        unreachable!()
    }
}

/// 通过评论并递归通过其 pending 祖先；评论不存在返回 `Ok(false)`。
///
/// 不做鉴权：调用方（web server fn 与 MCP 工具）各自鉴权。缓存失效在此完成。
#[cfg(feature = "server")]
pub(crate) async fn approve_comment_impl(id: i64) -> Result<bool, crate::api::error::AppError> {
    use crate::api::error::AppError;
    use crate::cache;
    use crate::db::pool::get_conn;

    let client = get_conn().await.map_err(AppError::db_conn)?;

    let row = client
        .query_opt(
            "SELECT post_id FROM comments WHERE id = $1 AND deleted_at IS NULL",
            &[&id],
        )
        .await
        .map_err(AppError::query)?;

    let post_id: i32 = match row {
        Some(r) => r.get("post_id"),
        None => return Ok(false),
    };

    // 直接通过目标评论并记录通过时间。
    client
        .execute(
            "UPDATE comments SET status = 'approved', approved_at = NOW() WHERE id = $1",
            &[&id],
        )
        .await
        .map_err(AppError::query)?;

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
        .map_err(AppError::query)?;

    cache::invalidate_comments_by_post(post_id).await;
    cache::invalidate_pending_count().await;
    Ok(true)
}

/// 将评论标记为垃圾；评论不存在返回 `Ok(false)`。仅旧状态为 approved 时失效评论缓存。
///
/// 不做鉴权：调用方各自鉴权。
#[cfg(feature = "server")]
pub(crate) async fn spam_comment_impl(id: i64) -> Result<bool, crate::api::error::AppError> {
    use crate::api::error::AppError;
    use crate::cache;
    use crate::db::pool::get_conn;

    let client = get_conn().await.map_err(AppError::db_conn)?;

    let row = client
        .query_opt(
            "SELECT post_id, status FROM comments WHERE id = $1 AND deleted_at IS NULL",
            &[&id],
        )
        .await
        .map_err(AppError::query)?;

    let Some(r) = row else {
        return Ok(false);
    };
    let post_id: i32 = r.get("post_id");
    let old_status: String = r.get("status");

    client
        .execute(
            "UPDATE comments SET status = 'spam' WHERE id = $1 AND deleted_at IS NULL",
            &[&id],
        )
        .await
        .map_err(AppError::query)?;

    if old_status == "approved" {
        cache::invalidate_comments_by_post(post_id).await;
    }
    cache::invalidate_pending_count().await;
    Ok(true)
}

/// 将评论移入回收站（软删除）；评论不存在返回 `Ok(false)`。
///
/// 不做鉴权：调用方各自鉴权。
#[cfg(feature = "server")]
pub(crate) async fn trash_comment_impl(id: i64) -> Result<bool, crate::api::error::AppError> {
    use crate::api::error::AppError;
    use crate::cache;
    use crate::db::pool::get_conn;

    let client = get_conn().await.map_err(AppError::db_conn)?;

    let row = client
        .query_opt(
            "SELECT post_id FROM comments WHERE id = $1 AND deleted_at IS NULL",
            &[&id],
        )
        .await
        .map_err(AppError::query)?;

    let Some(r) = row else {
        return Ok(false);
    };
    let post_id: i32 = r.get("post_id");

    client
        .execute(
            "UPDATE comments SET status = 'trash', deleted_at = NOW() WHERE id = $1",
            &[&id],
        )
        .await
        .map_err(AppError::query)?;

    cache::invalidate_comments_by_post(post_id).await;
    cache::invalidate_pending_count().await;
    Ok(true)
}
