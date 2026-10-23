//! 把 server function 错误整理成给人看的失败提示。
//!
//! Leptos 版 server fn 直接返回 [`AppError`]（JSON 编码跨端），业务错误本身已是
//! 可读中文；框架层错误（`AppError::ServerFn`）的文本带英文细节，不能直接拼进界面。

use crate::error::AppError;
use leptos::prelude::ServerFnErrorErr;

/// 服务端没给具体原因时的说法（`AppError` 对查询与事务错误统一返回它），不再重复展示。
const GENERIC_REASON: &str = "操作失败";

/// 可以展示给用户的失败原因；网络中断给固定说法，拿不到具体原因时为 `None`。
pub fn error_reason(err: &AppError) -> Option<String> {
    match err {
        AppError::BadRequest(message) => {
            let message = message.trim();
            (!message.is_empty() && message != GENERIC_REASON).then(|| message.to_string())
        }
        AppError::Unauthorized | AppError::Forbidden | AppError::NotFound => Some(err.to_string()),
        // 内部错误只有固定文案，按「请稍后再试」处理。
        AppError::Internal => None,
        AppError::ServerFn(inner) => match inner {
            // 服务端经框架回传的消息：与 Dioxus 版 `ServerError` 分支一致，非空且非通用说法才展示。
            ServerFnErrorErr::ServerError(message) => {
                let message = message.trim();
                (!message.is_empty() && message != GENERIC_REASON).then(|| message.to_string())
            }
            ServerFnErrorErr::Request(_) | ServerFnErrorErr::Response(_) => {
                Some("网络连接中断，请检查网络后重试".to_string())
            }
            // 反序列化等客户端失败（含反向代理返回的非 JSON 错误页）不泄露内部文本。
            _ => None,
        },
    }
}

/// 失败提示：有具体原因时为「保存失败：标题不能为空」，否则为「保存失败，请稍后再试」。
///
/// 签名：`failure_message(action: &str, err: &AppError) -> String`。
pub fn failure_message(action: &str, err: &AppError) -> String {
    match error_reason(err) {
        Some(reason) => format!("{action}：{reason}"),
        None => format!("{action}，请稍后再试"),
    }
}
