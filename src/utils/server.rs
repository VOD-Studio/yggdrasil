//! 服务端共享工具（仅 `feature = "server"` 编译）。
//!
//! 集中跨模块重复的服务端常量与工具函数（issue #7 重复常量去重）。

#![cfg(feature = "server")]

use sha2::{Digest, Sha256};

/// 明文 token / 任意字符串 → SHA-256 hex。
///
/// 此前 `auth/session.rs` 与 `mcp/auth.rs` 各有一份逐字相同的实现。
pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// 邮箱格式正则（`^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$`）。
///
/// 此前 `api/auth.rs` 与 `api/comments/helpers.rs` 各有一份逐字相同的 LazyLock。
pub static EMAIL_REGEX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")
        .expect("EMAIL_REGEX 正则模式应在编译期通过校验")
});

/// 上传文件大小上限（5 MiB）。
///
/// 此前 `api/upload.rs` 与 `mcp/tools/media.rs` 各定义一份相同的常量。
pub const MAX_FILE_SIZE: usize = 5 * 1024 * 1024;

/// 启动期数据库迁移超时窗口（秒），由 `MIGRATE_STARTUP_TIMEOUT_SECS` 控制，默认 30。
///
/// 此前 `main.rs` 与 `db/pool.rs`（`get_conn_for_startup`、`ensure_database_exists` 两处）
/// 各有一份逐字相同的 `.ok().and_then(parse).unwrap_or(30)` 解析链。
pub fn parse_migrate_startup_timeout() -> u64 {
    std::env::var("MIGRATE_STARTUP_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(30)
}

/// 默认 SSR 增量缓存时长（秒）。
pub const DEFAULT_SSR_CACHE_SECS: u64 = 3600;

pub fn parse_bool_value(value: Option<&str>, default: bool) -> bool {
    match value.map(str::trim) {
        Some(value)
            if value.eq_ignore_ascii_case("1")
                || value.eq_ignore_ascii_case("true")
                || value.eq_ignore_ascii_case("yes")
                || value.eq_ignore_ascii_case("on") =>
        {
            true
        }
        Some(value)
            if value.eq_ignore_ascii_case("0")
                || value.eq_ignore_ascii_case("false")
                || value.eq_ignore_ascii_case("no")
                || value.eq_ignore_ascii_case("off") =>
        {
            false
        }
        _ => default,
    }
}

/// Read a boolean environment variable with an explicit default.
pub fn parse_env_bool(name: &str, default: bool) -> bool {
    let value = std::env::var(name).ok();
    parse_bool_value(value.as_deref(), default)
}

/// SSR cache TTL from the environment, with the shared default.
pub fn parse_ssr_cache_secs() -> u64 {
    std::env::var("SSR_CACHE_SECS")
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(DEFAULT_SSR_CACHE_SECS)
}

#[cfg(test)]
mod tests {
    use super::parse_bool_value;

    #[test]
    fn parse_bool_value_accepts_common_spellings() {
        for value in ["1", "true", "yes", "on"] {
            assert!(parse_bool_value(Some(value), false));
        }
        for value in ["0", "false", "no", "off"] {
            assert!(!parse_bool_value(Some(value), true));
        }
    }

    #[test]
    fn parse_bool_value_uses_default_for_unknown_values() {
        assert!(parse_bool_value(Some("maybe"), true));
        assert!(!parse_bool_value(Some("maybe"), false));
        assert!(parse_bool_value(None, true));
        assert!(!parse_bool_value(None, false));
    }
}

/// 转义 SQL `LIKE` 模式串中的特殊字符（`\`、`%`、`_`），配合 `ESCAPE '\\'` 使用。
///
/// 此前 `api/posts/list.rs`（逐字符循环）与 `api/posts/search.rs`、`mcp/tools/read.rs`
///（replace 链）各有一份实现；统一为等价且更简洁的 replace 链风格。
pub fn escape_like_pattern(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
