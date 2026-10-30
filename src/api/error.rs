//! 应用内部错误与 server function 的公开错误契约。
//!
//! [`AppError`](crate::api::error::AppError) 保留服务端上下文，只有脱敏后的
//! [`ApiError`](crate::api::error::ApiError) 可以跨越网络或 SSR 资源边界。
//! 业务类别由枚举标签传输，消息只用于展示，不能参与错误分类。

use std::fmt;

use leptos::prelude::ServerFnError;
use leptos::server_fn::{
    codec::JsonEncoding,
    error::{FromServerFnError, ServerFnErrorErr},
};
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Serialize};

/// server function、SSR 资源与界面共用的公开错误。
///
/// `Framework` 保留网络、编解码等基础设施错误；其原始文本不应直接显示给用户。
///
/// 线格式是相邻标签 `{"code": "...", "message": ...}`，序列化用派生实现；
/// 反序列化手写（见下方 `impl Deserialize`），避免派生代码在 WASM 里引入
/// `Content` 缓冲和 `ServerFnError` 的整套派生解码。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

const CODES: &[&str] = &[
    "unauthorized",
    "forbidden",
    "not_found",
    "bad_request",
    "unavailable",
    "internal",
    "framework",
];

const FRAMEWORK_VARIANTS: &[&str] = &[
    "Registration",
    "Request",
    "Response",
    "ServerError",
    "MiddlewareError",
    "Deserialization",
    "Serialization",
    "Args",
    "MissingArg",
];

/// `message` 字段的三种线形态：业务文本、`Framework` 的 `{变体: 文本}`、`Unavailable` 的 null。
enum Message {
    Text(String),
    Tagged(String, String),
    Null,
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct MessageVisitor;

        impl<'de> Visitor<'de> for MessageVisitor {
            type Value = Message;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("错误消息文本或 {变体: 文本}")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Message, E> {
                Ok(Message::Text(v.to_owned()))
            }

            fn visit_string<E: de::Error>(self, v: String) -> Result<Message, E> {
                Ok(Message::Text(v))
            }

            fn visit_unit<E: de::Error>(self) -> Result<Message, E> {
                Ok(Message::Null)
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Message, A::Error> {
                let Some((name, text)) = map.next_entry::<String, String>()? else {
                    return Err(de::Error::invalid_length(0, &self));
                };
                if map.next_key::<IgnoredAny>()?.is_some() {
                    return Err(de::Error::invalid_length(2, &self));
                }
                Ok(Message::Tagged(name, text))
            }
        }

        d.deserialize_any(MessageVisitor)
    }
}

impl<'de> Deserialize<'de> for ApiError {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct ApiErrorVisitor;

        impl<'de> Visitor<'de> for ApiErrorVisitor {
            type Value = ApiError;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("带 code 与 message 的错误对象")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ApiError, A::Error> {
                let mut code: Option<String> = None;
                let mut message: Option<Message> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "code" => {
                            if code.is_some() {
                                return Err(de::Error::duplicate_field("code"));
                            }
                            code = Some(map.next_value()?);
                        }
                        "message" => {
                            if message.is_some() {
                                return Err(de::Error::duplicate_field("message"));
                            }
                            message = Some(map.next_value()?);
                        }
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                let code = code.ok_or_else(|| de::Error::missing_field("code"))?;
                api_error_from_parts(&code, message)
            }
        }

        d.deserialize_struct("ApiError", &["code", "message"], ApiErrorVisitor)
    }
}

/// 按 `code` 组装 [`ApiError`]；`message` 形态与派生实现的要求一致。
fn api_error_from_parts<E: de::Error>(code: &str, message: Option<Message>) -> Result<ApiError, E> {
    let text = |message: Option<Message>| match message {
        Some(Message::Text(text)) => Ok(text),
        None => Err(E::missing_field("message")),
        Some(_) => Err(E::custom("message 应为字符串")),
    };
    Ok(match code {
        "unauthorized" => ApiError::Unauthorized(text(message)?),
        "forbidden" => ApiError::Forbidden(text(message)?),
        "not_found" => ApiError::NotFound(text(message)?),
        "bad_request" => ApiError::BadRequest(text(message)?),
        "internal" => ApiError::Internal(text(message)?),
        "unavailable" => match message {
            None | Some(Message::Null) => ApiError::Unavailable,
            Some(_) => return Err(E::custom("unavailable 不带 message")),
        },
        "framework" => match message {
            Some(Message::Tagged(name, text)) => ApiError::Framework(framework_error(&name, text)?),
            None => return Err(E::missing_field("message")),
            Some(_) => return Err(E::custom("framework 的 message 应为 {变体: 文本}")),
        },
        _ => return Err(E::unknown_variant(code, CODES)),
    })
}

/// `{变体: 文本}` → 同名 [`ServerFnError`] 变体；已废弃的 `WrappedServerError` 按未知变体处理。
fn framework_error<E: de::Error>(name: &str, text: String) -> Result<ServerFnError, E> {
    Ok(match name {
        "Registration" => ServerFnError::Registration(text),
        "Request" => ServerFnError::Request(text),
        "Response" => ServerFnError::Response(text),
        "ServerError" => ServerFnError::ServerError(text),
        "MiddlewareError" => ServerFnError::MiddlewareError(text),
        "Deserialization" => ServerFnError::Deserialization(text),
        "Serialization" => ServerFnError::Serialization(text),
        "Args" => ServerFnError::Args(text),
        "MissingArg" => ServerFnError::MissingArg(text),
        _ => return Err(E::unknown_variant(name, FRAMEWORK_VARIANTS)),
    })
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

    #[test]
    fn wire_format_decodes_independent_of_field_order() {
        let not_found = ApiError::NotFound("文章不存在".into());
        assert_eq!(
            ApiError::de(r#"{"code":"not_found","message":"文章不存在"}"#.into()),
            not_found
        );
        assert_eq!(
            ApiError::de(r#"{"message":"文章不存在","code":"not_found"}"#.into()),
            not_found
        );
        assert_eq!(
            ApiError::de(r#"{"code":"unavailable"}"#.into()),
            ApiError::Unavailable
        );
        assert_eq!(
            ApiError::de(r#"{"code":"unavailable","message":null}"#.into()),
            ApiError::Unavailable
        );
        assert_eq!(
            ApiError::de(r#"{"code":"framework","message":{"Request":"x"}}"#.into()),
            ApiError::Framework(ServerFnError::Request("x".into()))
        );
        assert!(matches!(
            ApiError::de(r#"{"code":"teapot","message":"x"}"#.into()),
            ApiError::Framework(ServerFnError::Deserialization(_))
        ));
    }

    #[test]
    fn framework_variants_round_trip() {
        let variants: [fn(String) -> ServerFnError; 9] = [
            ServerFnError::Registration,
            ServerFnError::Request,
            ServerFnError::Response,
            ServerFnError::ServerError,
            ServerFnError::MiddlewareError,
            ServerFnError::Deserialization,
            ServerFnError::Serialization,
            ServerFnError::Args,
            ServerFnError::MissingArg,
        ];
        for variant in variants {
            let error = ApiError::Framework(variant("m".into()));
            assert_eq!(ApiError::de(error.ser()), error);
        }
    }
}
