//! 运行日志实时流 SSE 端点：`GET /api/logs/stream?levels=ERROR,WARN&target=X&q=Y`。
//!
//! 鉴权镜像 [`crate::api::database::export`]：从 cookie 取 session，校验管理员。
//! 事件类型：
//! - `log`：JSON [`LogEntry`]（实时事件尚未落库，`id` 恒为 0，不可用作游标）；
//! - `gap`：broadcast 通道满导致本连接丢事件（`Lagged`），data 为丢失条数文本。
//!
//! 过滤按连接参数在服务端完成：levels 逗号分隔（白名单校验，非法级别静默忽略，
//! 空 = 全部）；target 精确匹配；q 为 message 大小写不敏感子串。
//! keep-alive comment 每 15s 一次，防反向代理超时关闭空闲连接。

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::Query;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::StreamExt;
use serde::Deserialize;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;

use crate::api::logs::capture::{self, LogRecord};
use crate::auth::session::parse_session_token;
use crate::models::log::LogEntry;

/// SSE 查询参数：`?levels=ERROR,WARN&target=X&q=Y`（全部可选）。
#[derive(Deserialize)]
pub struct LogStreamQuery {
    pub levels: Option<String>,
    pub target: Option<String>,
    pub q: Option<String>,
}

/// SSE handler：cookie admin 鉴权后 subscribe 广播通道，按连接参数过滤推送。
pub async fn log_stream(
    headers: HeaderMap,
    Query(q): Query<LogStreamQuery>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    // 鉴权：cookie → session → admin（镜像 export.rs）
    let cookie_header = headers
        .get("cookie")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let token = parse_session_token(cookie_header).map(str::to_string);
    let token = match token {
        Some(t) => t,
        None => return Err((StatusCode::UNAUTHORIZED, "未登录".to_string())),
    };
    let user = match crate::api::auth::get_user_by_token(&token).await {
        Ok(Some(u)) => u,
        _ => return Err((StatusCode::UNAUTHORIZED, "会话已过期".to_string())),
    };
    if user.role != crate::models::user::UserRole::Admin {
        return Err((StatusCode::FORBIDDEN, "权限不足".to_string()));
    }

    // levels：逗号分隔、trim 大写、白名单过滤（非法级别静默忽略——EventSource
    // 读不到 400 响应体，容错优于报错）；空 vec = 全部级别。
    let levels: Vec<String> = q
        .levels
        .unwrap_or_default()
        .split(',')
        .map(|l| l.trim().to_uppercase())
        .filter(|l| super::VALID_LEVELS.contains(&l.as_str()))
        .collect();
    let target = q
        .target
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    // q 预转小写，匹配时对 message 做大小写不敏感子串（语义对齐 SQL ILIKE）。
    let needle =
        q.q.map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());

    let stream = BroadcastStream::new(capture::subscribe_live()).filter_map(move |item| {
        std::future::ready(
            match item {
                Ok(record) => {
                    if !matches_filters(&record, &levels, &target, &needle) {
                        None
                    } else {
                        let entry = LogEntry {
                            // 实时事件尚未落库，无数据库 id；前端勿用它做游标。
                            id: 0,
                            ts: record.ts,
                            level: record.level,
                            target: record.target,
                            message: record.message,
                        };
                        Some(
                            Event::default()
                                .event("log")
                                .json_data(entry)
                                .unwrap_or_else(|_| Event::default().event("log").data("{}")),
                        )
                    }
                }
                // 消费太慢被广播通道覆盖：通知前端有缺口（应重新拉历史页兜底）。
                Err(BroadcastStreamRecvError::Lagged(n)) => Some(
                    Event::default()
                        .event("gap")
                        .data(format!("missed {n} log events")),
                ),
            }
            .map(Ok),
        )
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

/// 按连接参数过滤单条记录。
fn matches_filters(
    record: &LogRecord,
    levels: &[String],
    target: &Option<String>,
    needle: &Option<String>,
) -> bool {
    if !levels.is_empty() && !levels.iter().any(|l| l == &record.level) {
        return false;
    }
    if let Some(t) = target {
        if &record.target != t {
            return false;
        }
    }
    if let Some(n) = needle {
        if !record.message.to_lowercase().contains(n.as_str()) {
            return false;
        }
    }
    true
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    fn record(level: &str, target: &str, message: &str) -> LogRecord {
        LogRecord {
            ts: chrono::Utc::now(),
            level: level.to_string(),
            target: target.to_string(),
            message: message.to_string(),
        }
    }

    #[test]
    fn empty_filters_match_everything() {
        let r = record("INFO", "yggdrasil::api::posts", "hello");
        assert!(matches_filters(&r, &[], &None, &None));
    }

    #[test]
    fn level_filter_matches_whitelist() {
        let r = record("WARN", "t", "m");
        let levels = vec!["ERROR".to_string(), "WARN".to_string()];
        assert!(matches_filters(&r, &levels, &None, &None));
        let levels = vec!["ERROR".to_string()];
        assert!(!matches_filters(&r, &levels, &None, &None));
    }

    #[test]
    fn target_filter_is_exact() {
        let r = record("INFO", "yggdrasil::api::posts", "m");
        assert!(matches_filters(
            &r,
            &[],
            &Some("yggdrasil::api::posts".to_string()),
            &None
        ));
        assert!(!matches_filters(
            &r,
            &[],
            &Some("yggdrasil::api".to_string()),
            &None
        ));
    }

    #[test]
    fn query_filter_is_case_insensitive_substring() {
        let r = record("INFO", "t", "Database Migration Failed");
        assert!(matches_filters(
            &r,
            &[],
            &None,
            &Some("migration failed".to_string())
        ));
        assert!(!matches_filters(
            &r,
            &[],
            &None,
            &Some("backup".to_string())
        ));
    }
}
