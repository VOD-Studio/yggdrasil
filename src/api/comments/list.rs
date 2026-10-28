//! 评论列表查询接口：后台管理用的评论列表与各状态条数。
//!
//! 所有接口均需管理员身份，Leptos server function 注册在 `/api` 路径下。
//! 仅在 `feature = "server"` 启用的服务端构建中查询数据库。

use crate::api::comments::types::*;
use crate::api::error::ApiError;
use leptos::prelude::*;

/// 获取全部评论分页列表。
///
/// 支持按状态筛选（`trash` 为软删除的评论）；未指定状态时返回所有未删除评论。
/// 同时返回各状态条数。
#[server(GetAllComments, endpoint = "get_all_comments", input = leptos::server_fn::codec::Json)]
pub async fn get_all_comments(
    page: i32,
    status: Option<String>,
) -> Result<AllCommentsResponse, ApiError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;

        let _admin = get_current_admin_user().await?;
        Ok(list_admin_comments_impl(page, status).await?)
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 后台评论分页列表与各状态条数。
///
/// 不做鉴权：调用方（web server fn 与 MCP 工具）各自鉴权。
#[cfg(feature = "server")]
pub(crate) async fn list_admin_comments_impl(
    page: i32,
    status: Option<String>,
) -> Result<AllCommentsResponse, crate::api::error::AppError> {
    use crate::api::comments::helpers::row_to_admin_comment;
    use crate::api::error::AppError;
    use crate::db::pool::get_conn;

    let page = page.max(1);
    let per_page: i64 = 20;
    let offset: i64 = (page as i64 - 1) * per_page;

    let client = get_conn().await.map_err(AppError::db_conn)?;

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
        .map_err(AppError::query)?
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
        .map_err(AppError::query)?;
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
        .map_err(AppError::query)?;
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
