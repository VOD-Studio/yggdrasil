//! 评论审核状态批量查询。
//!
//! 用于前端轮询刚刚提交的评论是否已通过审核，支持传入多个 id。
//! 仅在 `feature = "server"` 启用的服务端构建中查询数据库。

use dioxus::prelude::*;

/// 单个评论的待审核状态结果。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingStatusItem {
    /// 评论 id。
    pub id: i64,
    /// 当前状态：approved / pending / rejected / spam / trash / gone。
    pub status: String,
}

/// 查询一组评论的当前审核状态。
///
/// 如果某 id 不存在，则返回状态 `"gone"`。
/// Dioxus server function，注册在 `/api` 路径下。
#[server(CheckPendingStatus, "/api")]
pub async fn check_pending_status(ids: Vec<i64>) -> Result<Vec<PendingStatusItem>, ServerFnError> {
    // 仅在服务端构建中执行 SQL 查询。
    #[cfg(feature = "server")]
    {
        use crate::api::error::AppError;
        use crate::db::pool::get_conn;

        // 空列表直接返回空结果，避免无意义的数据库查询。
        if ids.is_empty() {
            return Ok(vec![]);
        }
        // 上限 100 条，防止超大 IN 列表拖慢查询或被滥用枚举评论状态。
        if ids.len() > 100 {
            return Ok(vec![]);
        }

        // 限流防高速遍历枚举评论状态（L3）。本接口供访客轮询自己刚提交的评论
        // 审核状态，故不加 admin 鉴权；但 strict 限流（对 unknown IP 降级宽松桶）
        // 足以阻止批量枚举。
        if let Some(ctx) = dioxus::fullstack::FullstackContext::current() {
            let headers = ctx.parts_mut().headers.clone();
            let ip = crate::api::rate_limit::get_client_ip(&headers).await;
            if let Err(_msg) = crate::api::rate_limit::check_strict_limit(&ip) {
                return Err(ServerFnError::new("请求过于频繁，请稍后再试"));
            }
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;

        let rows = client
            .query(
                "SELECT id, status FROM comments WHERE id = ANY($1)",
                &[&ids],
            )
            .await
            .map_err(AppError::query)?;

        // 将查询结果收集为 HashMap，便于后续按传入顺序补齐缺失项。
        let found: std::collections::HashMap<i64, String> = rows
            .iter()
            .map(|r| (r.get::<_, i64>(0), r.get::<_, String>(1)))
            .collect();

        let result: Vec<PendingStatusItem> = ids
            .into_iter()
            .map(|id| {
                let status = found
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| "gone".to_string());
                PendingStatusItem { id, status }
            })
            .collect();

        Ok(result)
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}
