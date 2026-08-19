//! 用户模型。
//!
//! 定义用户角色、内部用户结构体以及可暴露给前端的 PublicUser。
//! User 包含密码哈希等敏感字段，PublicUser 用于在 API 中隐藏这些字段。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 用户角色枚举。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UserRole {
    /// 管理员，拥有全部后台权限。
    Admin,
    /// 被禁用的用户，无法登录或操作。
    Blocked,
}

impl UserRole {
    /// 将数据库中的角色字符串解析为 UserRole，无法识别时返回 None。
    #[cfg(feature = "server")]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(UserRole::Admin),
            "blocked" => Some(UserRole::Blocked),
            _ => None,
        }
    }
}

/// 会话缓存使用的轻量用户结构体，不含密码哈希。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionUser {
    /// 用户主键。
    pub id: i32,
    /// 用户名。
    pub username: String,
    /// 邮箱地址。
    pub email: String,
    /// 对外展示名称（为空时 UI 回退展示 username）。
    pub display_name: Option<String>,
    /// 头像 URL（/uploads/ 素材路径或 http(s) 外链）。
    pub avatar_url: Option<String>,
    /// 用户角色。
    pub role: UserRole,
    /// 账户创建时间。
    pub created_at: DateTime<Utc>,
    /// 会话世代号，签发 session 时记录；与 users 表当前值不一致则 session 失效。
    pub session_generation: i32,
}

/// 可公开的用户信息，从 User 转换而来，不含密码哈希。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicUser {
    /// 用户主键。
    pub id: i32,
    /// 用户名。
    pub username: String,
    /// 邮箱地址。
    pub email: String,
    /// 对外展示名称（为空时 UI 回退展示 username）。
    pub display_name: Option<String>,
    /// 头像 URL（/uploads/ 素材路径或 http(s) 外链）。
    pub avatar_url: Option<String>,
    /// 用户角色。
    pub role: UserRole,
    /// 账户创建时间。
    pub created_at: DateTime<Utc>,
}

impl From<SessionUser> for PublicUser {
    /// 将 SessionUser 转换为 PublicUser。
    fn from(u: SessionUser) -> Self {
        PublicUser {
            id: u.id,
            username: u.username,
            email: u.email,
            display_name: u.display_name,
            avatar_url: u.avatar_url,
            role: u.role,
            created_at: u.created_at,
        }
    }
}

impl PublicUser {
    /// 对外展示名称：优先 display_name，未设置时回退 username。
    pub fn display_label(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.username)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn sample_user() -> SessionUser {
        SessionUser {
            id: 1,
            username: "admin".to_string(),
            email: "admin@test.com".to_string(),
            display_name: None,
            avatar_url: None,
            role: UserRole::Admin,
            created_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            session_generation: 0,
        }
    }

    #[test]
    #[cfg(feature = "server")]
    fn user_role_from_str() {
        assert_eq!(UserRole::from_str("admin"), Some(UserRole::Admin));
        assert_eq!(UserRole::from_str("blocked"), Some(UserRole::Blocked));
        assert_eq!(UserRole::from_str("unknown"), None);
        assert_eq!(UserRole::from_str(""), None);
    }

    #[test]
    fn user_role_serde_roundtrip() {
        let json = serde_json::to_string(&UserRole::Admin).unwrap();
        assert_eq!(
            serde_json::from_str::<UserRole>(&json).unwrap(),
            UserRole::Admin
        );
    }

    #[test]
    fn session_user_to_public_user_conversion() {
        // D2：User 结构体已删除（生产路径 auth.rs 直接从 row 构造 SessionUser），
        // 仅保留 From<SessionUser> for PublicUser（auth.rs:431 在用）。
        let session = sample_user();
        let public: PublicUser = session.clone().into();
        assert_eq!(public.id, session.id);
        assert_eq!(public.username, session.username);
        assert_eq!(public.email, session.email);
        assert_eq!(public.role, session.role);
        assert_eq!(public.created_at, session.created_at);
    }

    #[test]
    fn public_user_excludes_session_generation() {
        // PublicUser 不含 session_generation（敏感会话字段）。
        let session = sample_user();
        let public: PublicUser = session.into();
        let json = serde_json::to_string(&public).unwrap();
        assert!(!json.contains("session_generation"));
    }

    #[test]
    fn display_label_falls_back_to_username() {
        let session = sample_user();
        let mut public: PublicUser = session.into();
        // 未设置 display_name 时回退 username。
        assert_eq!(public.display_label(), "admin");
        // 设置后优先展示 display_name。
        public.display_name = Some("站长".to_string());
        assert_eq!(public.display_label(), "站长");
    }
}
