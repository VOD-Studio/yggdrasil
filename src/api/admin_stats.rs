//! 后台统计接口：侧栏导航项右侧的数量。
//!
//! 文章数读文章统计缓存，待审评论数读待审计数缓存；素材与评论总数是两次
//! 计数查询，侧栏每次换页刷新一次。Leptos server function，注册在 `/api` 路径下。

// 与 settings 等模块一致：Leptos `#[server]` 宏触发 deprecated/unit 提示，按项目惯例放行。
#![allow(clippy::unused_unit, deprecated)]

use crate::api::error::ApiError;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// 侧栏数量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminNavCounts {
    /// 未删除的文章（含草稿）。
    pub posts: i64,
    /// 已登记的素材。
    pub assets: i64,
    /// 待审与已通过的评论，不含垃圾与回收站。
    pub comments: i64,
    /// 待审评论。
    pub pending_comments: i64,
}

/// 获取侧栏数量。需要 admin 权限。
#[server(GetAdminNavCounts, endpoint = "get_admin_nav_counts", input = leptos::server_fn::codec::Json)]
pub async fn get_admin_nav_counts() -> Result<AdminNavCounts, ApiError> {
    #[cfg(feature = "server")]
    {
        use crate::api::error::AppError;
        use crate::db::pool::get_conn;

        crate::api::auth::get_current_admin_user().await?;
        let stats = crate::api::posts::load_post_stats().await?;

        let client = get_conn().await.map_err(AppError::db_conn)?;
        let row = client
            .query_one(
                "SELECT
                    (SELECT COUNT(*) FROM assets) AS assets,
                    (SELECT COUNT(*) FROM comments
                      WHERE deleted_at IS NULL AND status IN ('pending', 'approved')) AS comments",
                &[],
            )
            .await
            .map_err(AppError::query)?;

        let pending_comments = match crate::cache::get_pending_count().await {
            Some(count) => count,
            None => {
                let count: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM comments WHERE status = 'pending' AND deleted_at IS NULL",
                        &[],
                    )
                    .await
                    .map_err(AppError::query)?
                    .get(0);
                crate::cache::set_pending_count(count).await;
                count
            }
        };

        Ok(AdminNavCounts {
            posts: stats.total,
            assets: row.get("assets"),
            comments: row.get("comments"),
            pending_comments,
        })
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}
