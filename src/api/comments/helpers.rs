//! 评论模块的辅助函数：数据转换、校验、哈希与头像生成。
//!
//! 所有工具函数仅在 `feature = "server"` 启用的服务端构建中使用。

#![allow(clippy::unused_unit, deprecated)]

#[cfg(feature = "server")]
use crate::models::comment::{AdminComment, CommentStatus, PublicComment};

/// 计算字符串的 MD5 哈希，用于 Gravatar。
#[cfg(feature = "server")]
pub fn md5_hash(input: &str) -> String {
    use md5::Digest;
    let hash = md5::Md5::digest(input.as_bytes());
    hex::encode(hash)
}

/// 根据邮箱生成 Cravatar（Gravatar 国内镜像）头像 URL。
#[cfg(feature = "server")]
pub fn gravatar_url(email: &str) -> String {
    let hash = md5_hash(&email.trim().to_lowercase());
    format!("https://cravatar.cn/avatar/{}?d=mp&s=80", hash)
}

/// 解析评论的对外展示身份：`(名称, 头像 URL, 是否作者)`。
///
/// `user_id` 非空（登录用户发表的评论）时，优先使用 JOIN users 得到的
/// **实时**显示名与头像——改名/换头像后历史评论自动跟随；匿名评论
/// （`user_id` 为 NULL）回退到评论行内快照名 + 邮箱 Gravatar。
/// 返回的名称经 HTML 转义，与 `author_name` 列的存储口径一致。
#[cfg(feature = "server")]
pub fn resolve_author_display(
    author_name: &str,
    author_email: &str,
    user_id: Option<i32>,
    user_display_name: Option<&str>,
    user_avatar_url: Option<&str>,
) -> (String, String, bool) {
    let is_author = user_id.is_some();
    let name = user_display_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(crate::utils::html::escape_html)
        .unwrap_or_else(|| author_name.to_string());
    let avatar = user_avatar_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| gravatar_url(author_email));
    (name, avatar, is_author)
}

/// 将数据库行转换为前端展示的公开评论结构。
///
/// 调用方查询须 LEFT JOIN users 并提供 `user_id` / `user_display_name` /
/// `user_avatar` 三列（匿名评论为 NULL）。
#[cfg(feature = "server")]
pub fn row_to_public_comment(row: &tokio_postgres::Row) -> PublicComment {
    let email: String = row.get("author_email");
    let created_at_dt: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let created_at_iso = created_at_dt.to_rfc3339();
    let created_at_relative = format_relative_time(created_at_dt);

    let snapshot_name: String = row.get("author_name");
    let user_id: Option<i32> = row.get("user_id");
    let user_display_name: Option<String> = row.get("user_display_name");
    let user_avatar: Option<String> = row.get("user_avatar");
    let (author_name, avatar_url, is_author) = resolve_author_display(
        &snapshot_name,
        &email,
        user_id,
        user_display_name.as_deref(),
        user_avatar.as_deref(),
    );

    PublicComment {
        id: row.get("id"),
        parent_id: row.get("parent_id"),
        depth: row.get("depth"),
        author_name,
        author_url: row.get("author_url"),
        avatar_url,
        is_author,
        content_html: row.get("content_html"),
        created_at: created_at_relative,
        created_at_iso,
    }
}

/// 将数据库行转换为后台管理使用的评论结构。
///
/// 与 [`row_to_public_comment`] 同一套身份解析：查询须 LEFT JOIN users
/// 提供 `user_id` / `user_display_name` / `user_avatar` 三列。
#[cfg(feature = "server")]
pub fn row_to_admin_comment(row: &tokio_postgres::Row) -> AdminComment {
    let status_str: String = row.get("status");
    let email: String = row.get("author_email");

    let snapshot_name: String = row.get("author_name");
    let user_id: Option<i32> = row.get("user_id");
    let user_display_name: Option<String> = row.get("user_display_name");
    let user_avatar: Option<String> = row.get("user_avatar");
    let (author_name, avatar_url, _) = resolve_author_display(
        &snapshot_name,
        &email,
        user_id,
        user_display_name.as_deref(),
        user_avatar.as_deref(),
    );

    AdminComment {
        id: row.get("id"),
        post_id: row.get("post_id"),
        post_title: row.get("post_title"),
        post_slug: row.get("post_slug"),
        parent_id: row.get("parent_id"),
        depth: row.get("depth"),
        author_name,
        author_email: email.clone(),
        author_url: row.get("author_url"),
        avatar_url,
        content_md: row.get("content_md"),
        status: CommentStatus::from_str(&status_str),
        created_at: row.get("created_at"),
    }
}

/// 将 UTC 时间格式化为相对时间（刚刚 / N 分钟前 / N 小时前 / N 天前 / 日期）。
///
/// 分档规则与前端 `crate::utils::time::relative_label_from_millis` 完全一致，
/// 通过共享分档函数保证服务端预渲染与前端实时计算的口径统一。
#[cfg(feature = "server")]
pub fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let delta_millis = now.signed_duration_since(dt).num_milliseconds();
    let iso = dt.to_rfc3339();
    crate::utils::time::relative_label_from_millis(delta_millis, &iso).0
}

/// 校验评论作者昵称：非空且不超过 50 字符。
#[cfg(feature = "server")]
pub fn validate_comment_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("请输入昵称".to_string());
    }
    if trimmed.len() > 50 {
        return Err("昵称长度不能超过 50 个字符".to_string());
    }
    Ok(())
}

/// 校验评论作者邮箱格式。
#[cfg(feature = "server")]
pub fn validate_comment_email(email: &str) -> Result<(), String> {
    if !crate::utils::server::EMAIL_REGEX.is_match(email.trim()) {
        return Err("邮箱格式不正确".to_string());
    }
    Ok(())
}

/// 校验评论作者网址：为空时允许，非空时必须以 http:// 或 https:// 开头、
/// 不含 HTML 特殊字符与空白，且不超过 200 字符。
#[cfg(feature = "server")]
pub fn validate_comment_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err("网址必须以 http:// 或 https:// 开头".to_string());
    }
    if trimmed.len() > 200 {
        return Err("网址长度不能超过 200 个字符".to_string());
    }
    if trimmed
        .chars()
        .any(|c| matches!(c, '<' | '>' | '"' | '\'' | '&' | ' ' | '\t' | '\n' | '\r'))
    {
        return Err("网址包含非法字符".to_string());
    }
    Ok(())
}

/// 校验评论内容：非空且不超过 10000 字符。
#[cfg(feature = "server")]
pub fn validate_comment_content(content: &str) -> Result<(), String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("请输入评论内容".to_string());
    }
    if trimmed.len() > 10000 {
        return Err("评论内容不能超过 10000 个字符".to_string());
    }
    Ok(())
}

/// 校验蜜罐字段：正常用户蜜罐为空，机器人可能填入任意内容。
///
/// 为空视为通过；一旦被填入内容即判定为机器人提交并拒绝。
/// 这是禁用 JS / 无视前端校验的机器人也能在服务端被拦下的防线。
#[cfg(feature = "server")]
pub fn validate_comment_honeypot(value: &str) -> Result<(), String> {
    if value.is_empty() {
        Ok(())
    } else {
        Err("评论提交异常".to_string())
    }
}

/// 计算评论内容哈希，用于检测短时间内的重复提交。
#[cfg(feature = "server")]
pub fn compute_content_hash(
    post_id: i32,
    parent_id: Option<i64>,
    name: &str,
    content: &str,
) -> String {
    use sha2::Digest;
    let input = format!(
        "{}:{}:{}:{}",
        post_id,
        parent_id.map(|id| id.to_string()).unwrap_or_default(),
        name.trim(),
        content.trim()
    );
    let hash = sha2::Sha256::digest(input.as_bytes());
    hex::encode(hash)
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn md5_hash_known_value() {
        assert_eq!(md5_hash("hello"), "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn md5_hash_empty() {
        assert_eq!(md5_hash(""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn gravatar_url_format() {
        let url = gravatar_url("test@example.com");
        assert!(url.starts_with("https://cravatar.cn/avatar/"));
        assert!(url.contains("?d=mp&s=80"));
    }

    #[test]
    fn gravatar_url_normalizes_email() {
        let url1 = gravatar_url("Test@Example.com");
        let url2 = gravatar_url("test@example.com");
        assert_eq!(url1, url2);
    }

    #[test]
    fn gravatar_url_trims_whitespace() {
        let url1 = gravatar_url(" test@example.com ");
        let url2 = gravatar_url("test@example.com");
        assert_eq!(url1, url2);
    }

    #[test]
    fn resolve_author_display_anonymous_uses_snapshot_and_gravatar() {
        let (name, avatar, is_author) =
            resolve_author_display("Alice", "a@example.com", None, None, None);
        assert_eq!(name, "Alice");
        assert_eq!(avatar, gravatar_url("a@example.com"));
        assert!(!is_author);
    }

    #[test]
    fn resolve_author_display_user_with_profile_fields() {
        let (name, avatar, is_author) = resolve_author_display(
            "snapshot",
            "a@example.com",
            Some(1),
            Some("站长"),
            Some("/uploads/a.webp"),
        );
        assert_eq!(name, "站长");
        assert_eq!(avatar, "/uploads/a.webp");
        assert!(is_author);
    }

    #[test]
    fn resolve_author_display_user_without_display_name_falls_back_to_snapshot() {
        // 登录用户未设显示名/头像：名称回退到写入时的快照（username），头像回退 Gravatar。
        let (name, avatar, is_author) =
            resolve_author_display("admin", "a@example.com", Some(1), Some("  "), None);
        assert_eq!(name, "admin");
        assert_eq!(avatar, gravatar_url("a@example.com"));
        assert!(is_author);
    }

    #[test]
    fn resolve_author_display_escapes_live_display_name() {
        // users.display_name 是未转义原始输入，与 author_name 列口径一致需转义。
        let (name, _, _) = resolve_author_display(
            "snapshot",
            "a@example.com",
            Some(1),
            Some("<b>x</b>"),
            None,
        );
        assert!(!name.contains('<'));
    }

    #[test]
    fn format_relative_time_just_now() {
        let now = chrono::Utc::now();
        assert_eq!(format_relative_time(now), "刚刚");
    }

    #[test]
    fn format_relative_time_minutes() {
        let dt = chrono::Utc::now() - chrono::Duration::minutes(5);
        assert_eq!(format_relative_time(dt), "5 分钟前");
    }

    #[test]
    fn format_relative_time_hours() {
        let dt = chrono::Utc::now() - chrono::Duration::hours(3);
        assert_eq!(format_relative_time(dt), "3 小时前");
    }

    #[test]
    fn format_relative_time_days() {
        let dt = chrono::Utc::now() - chrono::Duration::days(7);
        assert_eq!(format_relative_time(dt), "7 天前");
    }

    #[test]
    fn format_relative_time_one_minute() {
        let dt = chrono::Utc::now() - chrono::Duration::minutes(1);
        assert_eq!(format_relative_time(dt), "1 分钟前");
    }

    #[test]
    fn format_relative_time_one_hour() {
        let dt = chrono::Utc::now() - chrono::Duration::hours(1);
        assert_eq!(format_relative_time(dt), "1 小时前");
    }

    #[test]
    fn format_relative_time_one_day() {
        let dt = chrono::Utc::now() - chrono::Duration::days(1);
        assert_eq!(format_relative_time(dt), "1 天前");
    }

    #[test]
    fn format_relative_time_old_date() {
        let dt = chrono::Utc::now() - chrono::Duration::days(60);
        let result = format_relative_time(dt);
        assert!(result.contains('-'));
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn validate_comment_name_valid() {
        assert!(validate_comment_name("Alice").is_ok());
        assert!(validate_comment_name("张三").is_ok());
    }

    #[test]
    fn validate_comment_name_empty() {
        assert!(validate_comment_name("").is_err());
        assert!(validate_comment_name("   ").is_err());
    }

    #[test]
    fn validate_comment_name_too_long() {
        assert!(validate_comment_name(&"a".repeat(51)).is_err());
    }

    #[test]
    fn validate_comment_name_max_length() {
        assert!(validate_comment_name(&"a".repeat(50)).is_ok());
    }

    #[test]
    fn validate_comment_email_valid() {
        assert!(validate_comment_email("user@example.com").is_ok());
        assert!(validate_comment_email("a.b+c@domain.co").is_ok());
    }

    #[test]
    fn validate_comment_email_invalid() {
        assert!(validate_comment_email("notanemail").is_err());
        assert!(validate_comment_email("@domain.com").is_err());
        assert!(validate_comment_email("user@").is_err());
    }

    #[test]
    fn validate_comment_url_valid() {
        assert!(validate_comment_url("http://example.com").is_ok());
        assert!(validate_comment_url("https://example.com/path").is_ok());
    }

    #[test]
    fn validate_comment_url_empty_is_ok() {
        assert!(validate_comment_url("").is_ok());
        assert!(validate_comment_url("   ").is_ok());
    }

    #[test]
    fn validate_comment_url_invalid_scheme() {
        assert!(validate_comment_url("ftp://example.com").is_err());
        assert!(validate_comment_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn validate_comment_url_uppercase_scheme() {
        assert!(validate_comment_url("HTTP://example.com").is_ok());
        assert!(validate_comment_url("HTTPS://example.com").is_ok());
        assert!(validate_comment_url("Http://example.com").is_ok());
    }

    #[test]
    fn validate_comment_url_fragment() {
        assert!(validate_comment_url("https://example.com#section").is_ok());
    }

    #[test]
    fn validate_comment_url_relative_path_rejected() {
        assert!(validate_comment_url("/path/to/page").is_err());
        assert!(validate_comment_url("path/to/page").is_err());
    }

    #[test]
    fn validate_comment_url_too_long() {
        let long_url = format!("https://example.com/{}", "a".repeat(200));
        assert!(validate_comment_url(&long_url).is_err());
    }

    #[test]
    fn validate_comment_content_valid() {
        assert!(validate_comment_content("Hello world").is_ok());
    }

    #[test]
    fn validate_comment_content_empty() {
        assert!(validate_comment_content("").is_err());
        assert!(validate_comment_content("   ").is_err());
    }

    #[test]
    fn validate_comment_content_too_long() {
        assert!(validate_comment_content(&"a".repeat(10001)).is_err());
    }

    #[test]
    fn validate_comment_content_max_length() {
        assert!(validate_comment_content(&"a".repeat(10000)).is_ok());
    }

    #[test]
    fn compute_content_hash_deterministic() {
        let h1 = compute_content_hash(1, None, "Alice", "Hello");
        let h2 = compute_content_hash(1, None, "Alice", "Hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_content_hash_different_inputs() {
        let h1 = compute_content_hash(1, None, "Alice", "Hello");
        let h2 = compute_content_hash(2, None, "Alice", "Hello");
        assert_ne!(h1, h2);
    }

    #[test]
    fn compute_content_hash_trims_whitespace() {
        let h1 = compute_content_hash(1, None, "Alice", "Hello");
        let h2 = compute_content_hash(1, None, " Alice ", " Hello ");
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_content_hash_64_hex_chars() {
        let h = compute_content_hash(1, None, "Alice", "Hello");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn validate_comment_honeypot_empty_is_ok() {
        assert!(validate_comment_honeypot("").is_ok());
    }

    #[test]
    fn validate_comment_honeypot_filled_is_err() {
        assert!(validate_comment_honeypot("anything").is_err());
        assert!(validate_comment_honeypot(" ").is_err());
    }
}
