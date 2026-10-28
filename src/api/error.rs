//! 应用内部错误与 server function 的公开错误契约。
//!
//! [`AppError`](crate::api::error::AppError) 保留服务端上下文，只有脱敏后的
//! [`ApiError`](crate::api::error::ApiError) 可以跨越网络或 SSR 资源边界。
//! 业务类别由枚举标签传输，消息只用于展示，不能参与错误分类。

use leptos::prelude::ServerFnError;
use leptos::server_fn::{
    codec::JsonEncoding,
    error::{FromServerFnError, ServerFnErrorErr},
};
use serde::{Deserialize, Serialize};

/// server function、SSR 资源与界面共用的公开错误。
///
/// `Framework` 保留网络、编解码等基础设施错误；其原始文本不应直接显示给用户。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "message", rename_all = "snake_case")]
pub enum ApiError {
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    BadRequest(String),
    Unavailable,
    Internal(String),
    Framework(ServerFnError),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized(message)
            | Self::Forbidden(message)
            | Self::NotFound(message)
            | Self::BadRequest(message)
            | Self::Internal(message) => f.write_str(message),
            Self::Unavailable | Self::Framework(_) => f.write_str("服务暂时不可用"),
        }
    }
}

impl std::error::Error for ApiError {}

impl FromServerFnError for ApiError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        Self::Framework(ServerFnError::from_server_fn_error(value))
    }
}

/// 应用层内部错误；数据库细节只写日志，不进入公开契约。
#[derive(Debug)]
pub enum AppError {
    /// 未认证（401）。
    Unauthorized(&'static str),
    /// 无权限（403）。
    Forbidden(&'static str),
    /// 资源不存在（404）。
    NotFound(&'static str),
    /// 业务规则拒绝，消息原样透传给用户。
    BadRequest(String),
    DbConn(String),
    Query(String),
    Transaction(String),
    /// 已脱敏、可展示的内部错误提示。
    Internal(&'static str),
}

#[cfg(feature = "server")]
impl AppError {
    /// 记录完整 source 链，对外仅保留错误类别。
    pub fn db_conn(e: impl std::error::Error) -> Self {
        tracing::error!(
            "DB connection failed: {}",
            crate::db::format_with_sources(&e)
        );
        AppError::DbConn("connection error".to_string())
    }

    /// 记录 SQL 错误，查询细节不得传给客户端。
    pub fn query(e: impl std::error::Error) -> Self {
        tracing::error!("Query failed: {}", crate::db::format_with_sources(&e));
        AppError::Query("query error".to_string())
    }

    /// 记录事务错误，事务细节不得传给客户端。
    pub fn tx(e: impl std::error::Error) -> Self {
        tracing::error!("Transaction failed: {}", crate::db::format_with_sources(&e));
        AppError::Transaction("transaction error".to_string())
    }
}

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::Unauthorized(message) => Self::Unauthorized(message.into()),
            AppError::Forbidden(message) => Self::Forbidden(message.into()),
            AppError::NotFound(message) => Self::NotFound(message.into()),
            AppError::BadRequest(message) => Self::BadRequest(message),
            AppError::DbConn(_) => Self::Unavailable,
            AppError::Query(_) | AppError::Transaction(_) => Self::Internal("操作失败".into()),
            AppError::Internal(message) => Self::Internal(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn business_categories_survive_server_function_encoding() {
        // 消息刻意相同：分类不能依赖展示文本。
        for error in [
            ApiError::from(AppError::Unauthorized("not_found:相同消息")),
            ApiError::from(AppError::Forbidden("not_found:相同消息")),
            ApiError::from(AppError::NotFound("not_found:相同消息")),
            ApiError::from(AppError::BadRequest("not_found:相同消息".into())),
            ApiError::from(AppError::Internal("not_found:相同消息")),
            ApiError::Unavailable,
        ] {
            assert_eq!(ApiError::de(error.ser()), error);
        }
    }

    #[test]
    fn database_details_never_cross_the_wire() {
        for (internal, expected) in [
            (
                AppError::DbConn("postgres://admin:secret@localhost:5432/private".into()),
                ApiError::Unavailable,
            ),
            (
                AppError::Query("SELECT password FROM users".into()),
                ApiError::Internal("操作失败".into()),
            ),
            (
                AppError::Transaction("deadlock detected on UPDATE posts".into()),
                ApiError::Internal("操作失败".into()),
            ),
        ] {
            let error = ApiError::from(internal);
            assert_eq!(error, expected);
            assert_eq!(ApiError::de(error.ser()), expected);
        }
    }

    #[test]
    fn malformed_responses_remain_framework_errors() {
        assert!(matches!(
            ApiError::de("not_found:代理错误页".into()),
            ApiError::Framework(ServerFnError::Deserialization(_))
        ));
        let error = ApiError::from_server_fn_error(ServerFnErrorErr::Request("not_found".into()));
        assert!(matches!(
            ApiError::de(error.ser()),
            ApiError::Framework(ServerFnError::Request(_))
        ));
    }
}
