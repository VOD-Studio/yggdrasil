//! 评论列表查询接口：后台管理用的全部评论列表与待审核计数。
//!
//! 所有接口均需管理员身份，Dioxus server function 注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中查询数据库。

use crate::api::comments::types::*;
use dioxus::prelude::*;

/// 获取待审核评论总数。
///
/// 优先从缓存读取，未命中时查询数据库并写入缓存。
#[server(GetPendingCount, "/api")]
pub async fn get_pending_count() -> Result<PendingCountResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;
        use crate::api::error::AppError;
        use crate::cache;
        use crate::db::pool::get_conn;

        let _admin = get_current_admin_user().await?;

        if let Some(cached) = cache::get_pending_count().await {
            return Ok(PendingCountResponse { count: cached });
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;

        let count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM comments WHERE status = 'pending' AND deleted_at IS NULL",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .get(0);

        cache::set_pending_count(count).await;

        Ok(PendingCountResponse { count })
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 获取全部评论分页列表。
///
/// 支持按状态筛选；未指定状态时返回所有未删除评论。
#[server(GetAllComments, "/api")]
pub async fn get_all_comments(
    page: i32,
    status: Option<String>,
) -> Result<AllCommentsResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;
        use crate::api::comments::helpers::row_to_admin_comment;
        use crate::api::error::AppError;
        use crate::db::pool::get_conn;

        let _admin = get_current_admin_user().await?;

        let page = page.max(1);
        let per_page: i64 = 20;
        let offset: i64 = (page as i64 - 1) * per_page;

        let client = get_conn().await.map_err(AppError::db_conn)?;

        // 根据是否传入状态参数，分别构造 SQL 与查询条件。
        let (total, rows) = match status.as_deref() {
            Some(s) if !s.is_empty() => {
                let total: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM comments WHERE status = $1 AND deleted_at IS NULL",
                        &[&s],
                    )
                    .await
                    .map_err(AppError::query)?
                    .get(0);

                let rows = client
                    .query(
                        "SELECT c.id, c.post_id, c.parent_id, c.depth, c.author_name, c.author_email, \
                                c.author_url, c.content_md, c.status, c.created_at, \
                                c.user_id, u.display_name AS user_display_name, u.avatar_url AS user_avatar, \
                                p.title as post_title, p.slug as post_slug \
                         FROM comments c JOIN posts p ON c.post_id = p.id \
                         LEFT JOIN users u ON c.user_id = u.id \
                         WHERE c.status = $1 AND c.deleted_at IS NULL \
                         ORDER BY c.created_at DESC LIMIT $2 OFFSET $3",
                        &[&s, &per_page, &offset],
                    )
                    .await
                    .map_err(AppError::query)?;

                (total, rows)
            }
            _ => {
                let total: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM comments WHERE deleted_at IS NULL",
                        &[],
                    )
                    .await
                    .map_err(AppError::query)?
                    .get(0);

                let rows = client
                    .query(
                        "SELECT c.id, c.post_id, c.parent_id, c.depth, c.author_name, c.author_email, \
                                c.author_url, c.content_md, c.status, c.created_at, \
                                c.user_id, u.display_name AS user_display_name, u.avatar_url AS user_avatar, \
                                p.title as post_title, p.slug as post_slug \
                         FROM comments c JOIN posts p ON c.post_id = p.id \
                         LEFT JOIN users u ON c.user_id = u.id \
                         WHERE c.deleted_at IS NULL \
                         ORDER BY c.created_at DESC LIMIT $1 OFFSET $2",
                        &[&per_page, &offset],
                    )
                    .await
                    .map_err(AppError::query)?;

                (total, rows)
            }
        };

        let comments = rows.iter().map(row_to_admin_comment).collect();

        Ok(AllCommentsResponse { comments, total })
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}
