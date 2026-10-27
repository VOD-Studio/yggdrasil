//! 将类型化的 API 错误整理成给人看的失败提示。
//!
//! 消息只用于展示，错误分类仅检查 [`ApiError`] 的枚举变体。
//! 框架错误可能带原始响应或内部细节，不能直接展示。

use leptos::prelude::ServerFnError;

use crate::api::error::ApiError;

/// 服务端没给具体原因时的说法（`AppError` 对查询与事务错误统一返回它），不再重复展示。
const GENERIC_REASON: &str = "操作失败";

/// 无法判断具体原因时的说法。
const UNAVAILABLE_REASON: &str = "服务暂时不可用，请稍后再试";

/// 可以展示给用户的失败原因；网络中断给固定说法，拿不到具体原因时为 `None`。
pub fn error_reason(err: &ApiError) -> Option<String> {
    match err {
        ApiError::Unauthorized(message)
        | ApiError::Forbidden(message)
        | ApiError::NotFound(message)
        | ApiError::BadRequest(message)
        | ApiError::Internal(message) => {
            let message = message.trim();
            (!message.is_empty() && message != GENERIC_REASON).then(|| message.to_string())
        }
        ApiError::Unavailable => Some("服务暂时不可用".to_string()),
        ApiError::Framework(ServerFnError::Request(_)) => {
            Some("网络连接中断，请检查网络后重试".to_string())
        }
        // 响应体不是 server_fn 的错误编码时（反向代理返回 HTML 错误页等），解码失败，
        // 错误消息里带着原始响应体。正文不能展示，只按其中出现的状态码说明。
        ApiError::Framework(
            ServerFnError::Deserialization(message) | ServerFnError::Response(message),
        ) => {
            let reason = ["413", "429", "502", "503", "504"]
                .iter()
                .find(|code| message.contains(**code))
                .and_then(|code| code.parse().ok().and_then(status_reason))
                .unwrap_or(UNAVAILABLE_REASON);
            Some(reason.to_string())
        }
        _ => None,
    }
}

fn status_reason(code: u16) -> Option<&'static str> {
    match code {
        413 => Some("内容超过服务器允许的大小"),
        429 => Some("请求过于频繁，请稍后再试"),
        502..=504 => Some(UNAVAILABLE_REASON),
        _ => None,
    }
}

/// 失败提示：有具体原因时为「保存失败：标题不能为空」，否则为「保存失败，请稍后再试」。
pub fn failure_message(action: &str, err: &ApiError) -> String {
    match error_reason(err) {
        Some(reason) => format!("{action}：{reason}"),
        None => format!("{action}，请稍后再试"),
    }
}

/// 只识别公开契约中的「资源不存在」，消息不参与判断。
pub fn is_not_found(err: &ApiError) -> bool {
    matches!(err, ApiError::NotFound(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn business_reason_and_generic_failure_are_presented_separately() {
        let error = ApiError::BadRequest("标题不能为空".into());
        assert_eq!(
            failure_message("保存失败", &error),
            "保存失败：标题不能为空"
        );
        let missing = ApiError::NotFound("文章不存在".into());
        assert_eq!(
            failure_message("加载失败", &missing),
            "加载失败：文章不存在"
        );
        for message in ["操作失败", "  "] {
            let error = ApiError::Internal(message.into());
            assert_eq!(failure_message("删除失败", &error), "删除失败，请稍后再试");
        }
    }

    #[test]
    fn unparseable_client_failures_never_leak_internal_text() {
        let err = ApiError::Framework(ServerFnError::Deserialization(
            "expected value at line 1".to_string(),
        ));
        let text = failure_message("发布失败", &err);
        assert_eq!(text, "发布失败：服务暂时不可用，请稍后再试");
        assert!(!text.contains("error"));
        assert!(!text.contains("line 1"));
    }

    #[test]
    fn non_json_error_pages_are_described_by_status() {
        let too_large = ApiError::Framework(ServerFnError::Deserialization(
            "Invalid format: missing delimiter in \"<html><title>413 Request Entity Too Large</title>\""
                .to_string(),
        ));
        assert_eq!(
            error_reason(&too_large).as_deref(),
            Some("内容超过服务器允许的大小")
        );
        let proxy = ApiError::Framework(ServerFnError::Response(
            "<h1>502 Bad Gateway</h1>".to_string(),
        ));
        assert_eq!(
            failure_message("登录失败", &proxy),
            "登录失败：服务暂时不可用，请稍后再试"
        );
        let busy = ApiError::Framework(ServerFnError::Deserialization(
            "<h1>429 Too Many Requests</h1>".to_string(),
        ));
        assert_eq!(
            failure_message("登录失败", &busy),
            "登录失败：请求过于频繁，请稍后再试"
        );
        let unknown = ApiError::Framework(ServerFnError::Deserialization(
            "<p>418 teapot</p>".to_string(),
        ));
        assert_eq!(
            failure_message("登录失败", &unknown),
            "登录失败：服务暂时不可用，请稍后再试"
        );
    }

    #[test]
    fn request_errors_read_as_network_problems() {
        let err = ApiError::Framework(ServerFnError::Request("connection reset".to_string()));
        assert_eq!(
            failure_message("回复失败", &err),
            "回复失败：网络连接中断，请检查网络后重试"
        );
    }

    #[test]
    fn other_variants_have_no_reason() {
        let err = ApiError::Framework(ServerFnError::Args("missing field".to_string()));
        assert_eq!(error_reason(&err), None);
        assert_eq!(failure_message("保存失败", &err), "保存失败，请稍后再试");
    }

    #[test]
    fn not_found_requires_the_typed_category() {
        assert!(is_not_found(&ApiError::NotFound("文章不存在".into())));
        for error in [
            ApiError::BadRequest("not_found:文章不存在".into()),
            ApiError::Internal("not_found".into()),
            ApiError::Framework(ServerFnError::Request("not_found:x".into())),
            ApiError::Framework(ServerFnError::ServerError("not_found".into())),
        ] {
            assert!(!is_not_found(&error));
        }
    }
}
