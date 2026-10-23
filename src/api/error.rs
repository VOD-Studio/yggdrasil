//! 应用错误类型与 `ServerFnError` 转换。
//!
//! `AppError` 封装认证、权限、数据库、内部错误等场景，
//! 并转换为对外友好的 `ServerFnError` 消息，避免泄露 SQL 细节。

use leptos::prelude::ServerFnError;

/// `AppError::NotFound` 转成 `ServerFnError` 时消息的固定前缀。
pub const NOT_FOUND_PREFIX: &str = "not_found:";

/// 应用层统一错误类型。
#[derive(Debug)]
pub enum AppError {
    /// 未认证（401）。
    Unauthorized(&'static str),
    /// 无权限（403）。
    Forbidden(&'static str),
    /// 资源不存在（404）。
    NotFound(&'static str),
    /// 客户端请求错误（400）——业务规则拒绝，消息原样透传给用户。
    BadRequest(String),
    /// 数据库连接失败。
    DbConn(String),
    /// SQL 查询执行失败。
    Query(String),
    /// 数据库事务失败。
    Transaction(String),
    /// 内部通用错误。
    Internal(&'static str),
}

#[cfg(feature = "server")]
impl AppError {
    /// 记录并包装数据库连接错误。
    ///
    /// 对外只暴露通用提示（脱敏），但服务端日志会展开 source 链，记录完整的
    /// 失败原因（如 `db error: connection refused`、IO 错误等）。传入的错误类型
    /// 必须实现 `std::error::Error` —— DB 相关错误（`tokio_postgres::Error`、
    /// `deadpool_postgres::PoolError`）均满足，这一点是上次"日志只显示 `db error`"
    /// 问题的根治前提。
    pub fn db_conn(e: impl std::error::Error) -> Self {
        tracing::error!(
            "DB connection failed: {}",
            crate::db::format_with_sources(&e)
        );
        AppError::DbConn("connection error".to_string())
    }

    /// 记录并包装 SQL 查询错误。
    pub fn query(e: impl std::error::Error) -> Self {
        tracing::error!("Query failed: {}", crate::db::format_with_sources(&e));
        AppError::Query("query error".to_string())
    }

    /// 记录并包装数据库事务错误。
    pub fn tx(e: impl std::error::Error) -> Self {
        tracing::error!("Transaction failed: {}", crate::db::format_with_sources(&e));
        AppError::Transaction("transaction error".to_string())
    }
}

/// 转换为 `ServerFnError`，对数据库类错误返回通用中文提示。
///
/// 全部映射为 `ServerError(消息)`；`NotFound` 额外带 [`NOT_FOUND_PREFIX`] 前缀，
/// 前端用 `utils::error::is_not_found` 识别后渲染 404 页面。
/// 注意：`AppError` 不能实现 `std::error::Error`，否则与 `ServerFnError` 的
/// 泛型 `From<E: Error>` 冲突。
impl From<AppError> for ServerFnError {
    fn from(err: AppError) -> ServerFnError {
        let msg = match &err {
            AppError::NotFound(m) => format!("{NOT_FOUND_PREFIX}{m}"),
            AppError::Unauthorized(m) | AppError::Forbidden(m) | AppError::Internal(m) => {
                m.to_string()
            }
            // BadRequest 是业务规则拒绝（如 SQL 护栏拦截），消息原样透传给用户。
            AppError::BadRequest(m) => m.clone(),
            AppError::DbConn(_) => "服务暂时不可用".to_string(),
            AppError::Query(_) | AppError::Transaction(_) => "操作失败".to_string(),
        };
        ServerFnError::ServerError(msg)
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    /// 取出 `ServerError` 的消息；其他变体视为测试失败。
    fn server_message(err: ServerFnError) -> String {
        match err {
            ServerFnError::ServerError(message) => message,
            other => panic!("expected ServerError, got {other:?}"),
        }
    }

    #[test]
    fn unauthorized_message_passthrough() {
        let err: ServerFnError = AppError::Unauthorized("未登录").into();
        assert_eq!(server_message(err), "未登录");
    }

    #[test]
    fn db_conn_hides_internal_details() {
        // 入参是实现 Error 的类型；构造函数内部会把 source 链写进日志，
        // 但对外（ServerFnError）必须只暴露通用提示。
        let src = std::io::Error::other("connection refused on port 5432");
        let err: ServerFnError = AppError::db_conn(src).into();
        let msg = server_message(err);
        assert!(
            !msg.contains("5432"),
            "should not leak internal details: {msg}"
        );
        assert_eq!(msg, "服务暂时不可用");
    }

    #[test]
    fn query_hides_sql_details() {
        let src = std::io::Error::other("syntax error at SELECT * FROM");
        let err: ServerFnError = AppError::query(src).into();
        let msg = server_message(err);
        assert!(!msg.contains("SELECT"), "should not leak SQL: {msg}");
    }

    #[test]
    fn forbidden_message_passthrough() {
        let err: ServerFnError = AppError::Forbidden("权限不足").into();
        assert_eq!(server_message(err), "权限不足");
    }

    #[test]
    fn not_found_message_carries_prefix() {
        // NotFound 带固定前缀，前端据此区分 404 页面与普通失败。
        let err: ServerFnError = AppError::NotFound("文章不存在").into();
        let msg = server_message(err);
        assert_eq!(msg, format!("{NOT_FOUND_PREFIX}文章不存在"));
    }

    #[test]
    fn internal_message_passthrough() {
        // Internal 错误的消息原样透传，便于向用户展示可读的内部错误描述。
        let err: ServerFnError = AppError::Internal("内部错误").into();
        assert_eq!(server_message(err), "内部错误");
    }

    #[test]
    fn bad_request_message_passthrough() {
        let err: ServerFnError = AppError::BadRequest("标题不能为空".to_string()).into();
        assert_eq!(server_message(err), "标题不能为空");
    }

    #[test]
    fn transaction_hides_sql_details() {
        // 事务错误同样返回通用提示，不泄露 SQL 细节。
        let src = std::io::Error::other("deadlock detected on UPDATE posts");
        let err: ServerFnError = AppError::tx(src).into();
        let msg = server_message(err);
        assert!(!msg.contains("UPDATE"), "should not leak SQL: {msg}");
        assert!(
            !msg.contains("deadlock"),
            "should not leak error detail: {msg}"
        );
        assert_eq!(msg, "操作失败");
    }

    #[test]
    fn db_conn_query_transaction_all_return_generic_message() {
        // 三类数据库错误对外均返回固定中文提示，避免泄露实现细节。
        let db_conn: ServerFnError = AppError::DbConn("x".into()).into();
        let query: ServerFnError = AppError::Query("x".into()).into();
        let tx: ServerFnError = AppError::Transaction("x".into()).into();

        assert_eq!(server_message(db_conn), "服务暂时不可用");
        assert_eq!(server_message(query), "操作失败");
        assert_eq!(server_message(tx), "操作失败");
    }
}
