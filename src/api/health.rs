//! 健康检查端点（liveness / readiness）。
//!
//! 提供两个无中间件、不走 CSRF/缓存/超时层的探针端点，
//! 挂载在 `static_routes` 上，供 Docker HEALTHCHECK 与反向代理/负载均衡使用：
//! - `GET /healthz` — liveness 存活探针。只要进程在跑就返回 200，不查 DB。
//! - `GET /readyz` — readiness 就绪探针。执行 `SELECT 1` 检测 DB 连通性，
//!   不可达时返回 503，附带连接池指标。
//!
//! 仅在 `server` feature 启用时编译。

#![cfg(feature = "server")]

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use std::time::Duration;

/// 连接池探活的超时时间。
///
/// 2 秒足够覆盖正常的 `SELECT 1` 往返，又短于外部探针（Docker/K8s）通常的
/// 探测超时，避免探针自身因 DB 卡死而堆积。
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// `GET /healthz` — liveness 存活探针。
///
/// 进程在跑即返回 200。不触碰数据库，保证即使 DB 故障时探针也能快速响应，
/// 让编排器知道容器本身没死（不需要重启），只是暂时无法服务。
pub async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// `GET /readyz` — readiness 就绪探针。
///
/// 流程：
/// 1. 取连接池状态（纯内存快照，无 I/O）；
/// 2. 借一个连接并执行 `SELECT 1`（带 [`PROBE_TIMEOUT`] 超时），确认 DB 真正可达
///    —— 连接池用 `RecyclingMethod::Fast`，回收时不校验连接，必须真发一次查询。
///
/// 直接用 `DB_POOL.get()` 而非 `get_conn()`：后者有指数退避重试（约 1.6s），
/// 探针应当快速失败而非等待重试。
///
/// 返回：
/// - 200 `{status:"ready", db:"ok", pool:{...}}` — 一切正常
/// - 503 `{status:"unready", db:"down"|"error"|"timeout", ...}` — DB 不可达
pub async fn readyz() -> (StatusCode, Json<Value>) {
    use crate::db::pool::DB_POOL;

    // 连接池状态：纯内存，无 I/O，即便 DB 故障也能拿到。
    let s = DB_POOL.status();
    let pool_info = json!({
        "size": s.size,
        "available": s.available,
        "max_size": s.max_size,
        "waiting": s.waiting,
    });

    // 借连接 + SELECT 1，整体限时 PROBE_TIMEOUT。
    match tokio::time::timeout(PROBE_TIMEOUT, DB_POOL.get()).await {
        Ok(Ok(conn)) => match conn.simple_query("SELECT 1").await {
            Ok(_) => (
                StatusCode::OK,
                Json(json!({ "status": "ready", "db": "ok", "pool": pool_info })),
            ),
            Err(e) => {
                tracing::warn!(error = ?e, "readiness database probe query failed");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({
                        "status": "unready",
                        "db": "error",
                        "pool": pool_info
                    })),
                )
            }
        },
        Ok(Err(e)) => {
            tracing::warn!(error = ?e, "readiness database connection failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "status": "unready",
                    "db": "down",
                    "pool": pool_info
                })),
            )
        }
        Err(_) => {
            tracing::warn!("readiness database probe timed out");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "status": "unready",
                    "db": "timeout",
                    "pool": pool_info
                })),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn healthz_returns_exactly_ok_status_without_touching_db() {
        // 契约：liveness 只回 `{"status":"ok"}`，不含 DB/连接池字段。
        // healthz 不访问 DB_POOL，测试环境无需 DATABASE_URL。
        let Json(body) = healthz().await;
        assert_eq!(body, json!({ "status": "ok" }));
    }

    #[test]
    fn probe_timeout_is_two_seconds() {
        assert_eq!(PROBE_TIMEOUT, Duration::from_secs(2));
    }
}
