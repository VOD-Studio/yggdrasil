//! 会话 token 生成、哈希与 Cookie 处理。
//!
//! token 使用 UUID，存储时使用 SHA-256 哈希，
//! Cookie 包含 HttpOnly、SameSite=Lax 与可选 Secure 标志。
//! 服务端上下文解析函数仅在 `feature = "server"` 时可用。

#[cfg(feature = "server")]
use chrono::{DateTime, Duration, Utc};
#[cfg(feature = "server")]
use uuid::Uuid;

#[cfg(feature = "server")]
/// 生成新的随机会话 token（UUID 格式）。
pub fn generate_token() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(feature = "server")]
/// 使用 SHA-256 对 token 进行哈希，用于数据库存储。
pub fn hash_token(token: &str) -> String {
    crate::utils::server::sha256_hex(token)
}

#[cfg(feature = "server")]
/// 会话默认生命周期（秒）：30 天。同时驱动 DB `expires_at`（见 [`default_expiry`]）
/// 与登录时下发的 Cookie `Max-Age`（`api/auth.rs::login`），避免两处独立硬编码导致漂移。
pub const SESSION_MAX_AGE_SECS: i32 = 30 * 24 * 60 * 60;

#[cfg(feature = "server")]
/// 返回默认会话过期时间（当前时间 + [`SESSION_MAX_AGE_SECS`]）。
pub fn default_expiry() -> DateTime<Utc> {
    Utc::now() + Duration::seconds(SESSION_MAX_AGE_SECS as i64)
}

#[cfg(feature = "server")]
/// 构造会话 Cookie 字符串，包含 HttpOnly、Path、Max-Age 与 SameSite。
pub fn session_cookie(token: &str, max_age_seconds: i32, secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!(
        "session={token}; HttpOnly; Path=/; Max-Age={max_age_seconds}; SameSite=Lax{secure_flag}"
    )
}

#[cfg(feature = "server")]
/// 从 `Cookie` 头中解析名为 `session` 的 token 值。
pub fn parse_session_token(cookie_header: &str) -> Option<&str> {
    cookie_header.split(';').map(|s| s.trim()).find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next()?.trim();
        let value = parts.next()?.trim();
        if name == "session" {
            Some(value)
        } else {
            None
        }
    })
}

#[cfg(feature = "server")]
/// 从 Dioxus `FullstackContext` 中读取 Cookie 并返回会话 token。
pub fn get_session_from_ctx() -> Option<String> {
    use dioxus::fullstack::FullstackContext;

    FullstackContext::current().and_then(|ctx| {
        let parts = ctx.parts_mut();
        parts
            .headers
            .get("cookie")
            .and_then(|h| h.to_str().ok())
            .and_then(parse_session_token)
            .map(|s| s.to_string())
    })
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use sha2::Digest; // hash_token_known_value 用 Sha256::digest 校验已知值

    #[test]
    fn parse_session_found() {
        let header = "session=abc123; path=/";
        assert_eq!(parse_session_token(header), Some("abc123"));
    }

    #[test]
    fn parse_session_single_cookie() {
        assert_eq!(parse_session_token("session=token456"), Some("token456"));
    }

    #[test]
    fn parse_session_not_found() {
        assert_eq!(parse_session_token("other=value"), None);
    }

    #[test]
    fn parse_session_empty_string() {
        assert_eq!(parse_session_token(""), None);
    }

    #[test]
    fn parse_session_multiple_cookies() {
        let header = "theme=dark; session=my-secret; lang=en";
        assert_eq!(parse_session_token(header), Some("my-secret"));
    }

    #[test]
    fn parse_session_empty_value() {
        assert_eq!(parse_session_token("session="), Some(""));
    }

    #[test]
    fn parse_session_trailing_semicolon() {
        assert_eq!(parse_session_token("session=abc;"), Some("abc"));
    }

    #[test]
    fn generate_token_is_uuid() {
        let token = generate_token();
        assert!(uuid::Uuid::parse_str(&token).is_ok());
    }

    #[test]
    fn default_expiry_is_future() {
        let expiry = default_expiry();
        assert!(expiry > chrono::Utc::now());
    }

    #[test]
    fn default_expiry_is_about_30_days() {
        let expiry = default_expiry();
        let diff = expiry - chrono::Utc::now();
        assert!(diff.num_days() >= 29 && diff.num_days() <= 31);
    }

    #[test]
    fn hash_token_is_deterministic() {
        let token = "test-token-123";
        assert_eq!(hash_token(token), hash_token(token));
    }

    #[test]
    fn hash_token_is_64_chars() {
        let hash = hash_token("any-token");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn hash_token_differs_from_input() {
        let token = "my-secret-token";
        assert_ne!(hash_token(token), token);
    }

    #[test]
    fn hash_token_known_value() {
        let hash = hash_token("hello");
        let expected = sha2::Sha256::digest(b"hello");
        assert_eq!(hash, hex::encode(expected));
    }

    #[test]
    fn session_cookie_without_secure() {
        let cookie = session_cookie("abc", 3600, false);
        assert!(cookie.contains("session=abc"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(!cookie.contains("Secure"));
    }

    #[test]
    fn session_cookie_with_secure() {
        let cookie = session_cookie("abc", 3600, true);
        assert!(cookie.contains("Secure"));
    }

    #[test]
    fn session_cookie_logout_has_zero_max_age() {
        let cookie = session_cookie("", 0, false);
        assert!(cookie.contains("Max-Age=0"));
    }
}
