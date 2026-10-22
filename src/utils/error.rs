//! 把 server function 错误整理成给人看的失败提示。
//!
//! `ServerFnError` 的 `Display` 带英文前缀与 `(details: None)`，不能直接拼进界面；
//! 服务端消息（`AppError` 转换而来）本身已是可读中文，取出来即可。

use dioxus::prelude::ServerFnError;

/// 服务端没给具体原因时的说法（`AppError` 对查询与事务错误统一返回它），不再重复展示。
const GENERIC_REASON: &str = "操作失败";

/// 可以展示给用户的失败原因；网络中断给固定说法，拿不到具体原因时为 `None`。
pub fn error_reason(err: &ServerFnError) -> Option<String> {
    match err {
        // 响应体不是 JSON 时（反向代理的错误页等），Dioxus 拼成 `HTTP {code}: {body}`，
        // 正文不能展示，只按状态码说明。
        ServerFnError::ServerError { message, code, .. }
            if message.starts_with(&format!("HTTP {code}:")) =>
        {
            status_reason(*code).map(str::to_string)
        }
        ServerFnError::ServerError { message, .. } => {
            let message = message.trim();
            (!message.is_empty() && message != GENERIC_REASON).then(|| message.to_string())
        }
        ServerFnError::Request(_) | ServerFnError::StreamError(_) => {
            Some("网络连接中断，请检查网络后重试".to_string())
        }
        _ => None,
    }
}

fn status_reason(code: u16) -> Option<&'static str> {
    match code {
        413 => Some("内容超过服务器允许的大小"),
        429 => Some("请求过于频繁，请稍后再试"),
        502..=504 => Some("服务暂时不可用，请稍后再试"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_message_is_shown_without_prefix_or_details() {
        let err = ServerFnError::new("标题不能为空");
        assert_eq!(failure_message("保存失败", &err), "保存失败：标题不能为空");
    }

    #[test]
    fn generic_or_empty_server_message_asks_to_retry() {
        for message in ["操作失败", "  "] {
            let err = ServerFnError::new(message);
            assert_eq!(failure_message("删除失败", &err), "删除失败，请稍后再试");
        }
    }

    #[test]
    fn client_side_failures_never_leak_internal_text() {
        let err = ServerFnError::Deserialization("expected value at line 1".to_string());
        let text = failure_message("发布失败", &err);
        assert_eq!(text, "发布失败，请稍后再试");
        assert!(!text.contains("error"));
    }

    #[test]
    fn non_json_error_pages_are_described_by_status() {
        let proxy = ServerFnError::ServerError {
            message: "HTTP 502: <h1>Bad Gateway</h1>".to_string(),
            code: 502,
            details: None,
        };
        assert_eq!(
            failure_message("登录失败", &proxy),
            "登录失败：服务暂时不可用，请稍后再试"
        );
        let unknown = ServerFnError::ServerError {
            message: "HTTP 418: <p>teapot</p>".to_string(),
            code: 418,
            details: None,
        };
        assert_eq!(
            failure_message("登录失败", &unknown),
            "登录失败，请稍后再试"
        );
    }

    #[test]
    fn stream_errors_read_as_network_problems() {
        let err = ServerFnError::StreamError("connection reset".to_string());
        assert_eq!(
            failure_message("回复失败", &err),
            "回复失败：网络连接中断，请检查网络后重试"
        );
    }
}
