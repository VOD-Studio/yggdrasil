//! 与主 crate `api::error::AppError` 对应的错误类型。
//!
//! Leptos 0.8 允许 server fn 直接返回自定义错误（`FromServerFnError`），
//! 错误以 JSON 编码跨端传输：客户端拿到的仍是枚举本身，不必像 Dioxus 版
//! 那样在水合时从错误文本里猜 "not_found"。

use leptos::prelude::*;
use leptos::server_fn::codec::JsonEncoding;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
pub enum AppError {
    #[error("请先登录")]
    Unauthorized,
    #[error("没有权限")]
    Forbidden,
    #[error("内容不存在")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    /// 数据库与内部错误对客户端只暴露固定文案，细节只进服务端日志。
    #[error("服务暂时不可用，请稍后再试")]
    Internal,
    #[error("请求失败：{0}")]
    ServerFn(ServerFnErrorErr),
}

impl FromServerFnError for AppError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        AppError::ServerFn(value)
    }
}

impl AppError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, AppError::NotFound)
    }

    /// 记录内部错误并返回脱敏变体。
    #[cfg(feature = "ssr")]
    pub fn internal(context: &str, err: impl std::fmt::Display) -> Self {
        eprintln!("[spike] {context}: {err}");
        AppError::Internal
    }

    /// SSR 时把错误映射到 HTTP 状态码（对应主 crate `commit_error_status`）。
    pub fn commit_status(&self) {
        #[cfg(feature = "ssr")]
        if let Some(resp) = use_context::<leptos_axum::ResponseOptions>() {
            let code = match self {
                AppError::Unauthorized => http::StatusCode::UNAUTHORIZED,
                AppError::Forbidden => http::StatusCode::FORBIDDEN,
                AppError::NotFound => http::StatusCode::NOT_FOUND,
                AppError::BadRequest(_) => http::StatusCode::BAD_REQUEST,
                _ => http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            resp.set_status(code);
        }
    }
}
