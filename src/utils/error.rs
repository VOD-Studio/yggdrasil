//! 把 server function 错误整理成给人看的失败提示。
//!
//! `ServerFnError` 的 `Display` 带英文前缀，不能直接拼进界面；
//! 服务端消息（`AppError` 转换而来）本身已是可读中文，取出来即可。
//! UI 一律经 [`failure_message`][crate::utils::error::failure_message] / [`error_reason`][crate::utils::error::error_reason] 展示错误，不直接展示 `err.to_string()`。

use leptos::prelude::ServerFnError;

use crate::api::error::NOT_FOUND_PREFIX;

/// 服务端没给具体原因时的说法（`AppError` 对查询与事务错误统一返回它），不再重复展示。
const GENERIC_REASON: &str = "操作失败";

/// 无法判断具体原因时的说法。
const UNAVAILABLE_REASON: &str = "服务暂时不可用，请稍后再试";

/// 可以展示给用户的失败原因；网络中断给固定说法，拿不到具体原因时为 `None`。
pub fn error_reason(err: &ServerFnError) -> Option<String> {
    match err {
        ServerFnError::ServerError(message) => {
            let message = message.strip_prefix(NOT_FOUND_PREFIX).unwrap_or(message);
            let message = message.trim();
            (!message.is_empty() && message != GENERIC_REASON).then(|| message.to_string())
        }
        ServerFnError::Request(_) => Some("网络连接中断，请检查网络后重试".to_string()),
        // 响应体不是 server_fn 的错误编码时（反向代理返回 HTML 错误页等），解码失败，
        // 错误消息里带着原始响应体。正文不能展示，只按其中出现的状态码说明。
        ServerFnError::Deserialization(message) | ServerFnError::Response(message) => {
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
pub fn failure_message(action: &str, err: &ServerFnError) -> String {
    match error_reason(err) {
        Some(reason) => format!("{action}：{reason}"),
        None => format!("{action}，请稍后再试"),
    }
}

/// 错误是否表示「资源不存在」（`AppError::NotFound` 带前缀转换而来）。
pub fn is_not_found(err: &ServerFnError) -> bool {
    matches!(err, ServerFnError::ServerError(m) if m.starts_with(NOT_FOUND_PREFIX) || m == "not_found")
}

/// 错误文本是否表示「资源不存在」，用于 `ErrorBoundary` 里只拿到错误文本的场景。
pub fn is_not_found_text(text: &str) -> bool {
    text.contains(NOT_FOUND_PREFIX) || text.contains("not_found")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_message_is_shown_without_prefix_or_details() {
        let err = ServerFnError::new("标题不能为空");
        assert_eq!(failure_message("保存失败", &err), "保存失败：标题不能为空");
    }

    #[test]
    fn not_found_prefix_is_stripped_from_reason() {
        let err = ServerFnError::ServerError(format!("{NOT_FOUND_PREFIX}文章不存在"));
        assert_eq!(failure_message("加载失败", &err), "加载失败：文章不存在");
    }

    #[test]
    fn generic_or_empty_server_message_asks_to_retry() {
        for message in ["操作失败", "  ", NOT_FOUND_PREFIX] {
            let err = ServerFnError::new(message);
            assert_eq!(failure_message("删除失败", &err), "删除失败，请稍后再试");
        }
    }

    #[test]
    fn unparseable_client_failures_never_leak_internal_text() {
        let err = ServerFnError::Deserialization("expected value at line 1".to_string());
        let text = failure_message("发布失败", &err);
        assert_eq!(text, "发布失败：服务暂时不可用，请稍后再试");
        assert!(!text.contains("error"));
        assert!(!text.contains("line 1"));
    }

    #[test]
    fn non_json_error_pages_are_described_by_status() {
        let too_large = ServerFnError::Deserialization(
            "Invalid format: missing delimiter in \"<html><title>413 Request Entity Too Large</title>\""
                .to_string(),
        );
        assert_eq!(
            error_reason(&too_large).as_deref(),
            Some("内容超过服务器允许的大小")
        );
        let proxy = ServerFnError::Response("<h1>502 Bad Gateway</h1>".to_string());
        assert_eq!(
            failure_message("登录失败", &proxy),
            "登录失败：服务暂时不可用，请稍后再试"
        );
        let busy = ServerFnError::Deserialization("<h1>429 Too Many Requests</h1>".to_string());
        assert_eq!(
            failure_message("登录失败", &busy),
            "登录失败：请求过于频繁，请稍后再试"
        );
        let unknown = ServerFnError::Deserialization("<p>418 teapot</p>".to_string());
        assert_eq!(
            failure_message("登录失败", &unknown),
            "登录失败：服务暂时不可用，请稍后再试"
        );
    }

    #[test]
    fn request_errors_read_as_network_problems() {
        let err = ServerFnError::Request("connection reset".to_string());
        assert_eq!(
            failure_message("回复失败", &err),
            "回复失败：网络连接中断，请检查网络后重试"
        );
    }

    #[test]
    fn other_variants_have_no_reason() {
        let err = ServerFnError::Args("missing field".to_string());
        assert_eq!(error_reason(&err), None);
        assert_eq!(failure_message("保存失败", &err), "保存失败，请稍后再试");
    }

    #[test]
    fn not_found_is_recognised_from_error_and_text() {
        let prefixed = ServerFnError::ServerError(format!("{NOT_FOUND_PREFIX}文章不存在"));
        assert!(is_not_found(&prefixed));
        assert!(is_not_found(&ServerFnError::ServerError(
            "not_found".into()
        )));
        assert!(!is_not_found(&ServerFnError::new("操作失败")));
        assert!(!is_not_found(&ServerFnError::Request("not_found:x".into())));
        assert!(is_not_found_text(
            "error running server function: not_found:文章不存在"
        ));
        assert!(is_not_found_text("not_found"));
        assert!(!is_not_found_text("页面加载失败"));
    }
}
