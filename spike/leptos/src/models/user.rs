//! 用户模型。
//!
//! 定义用户角色、会话用户结构体以及可暴露给前端的 PublicUser。

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
    #[cfg(feature = "ssr")]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(UserRole::Admin),
            "blocked" => Some(UserRole::Blocked),
            _ => None,
        }
    }
}

/// 会话校验得到的轻量用户结构体，不含密码哈希。
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
    /// 头像 URL。
    pub avatar_url: Option<String>,
    /// 用户角色。
    pub role: UserRole,
    /// 账户创建时间。
    pub created_at: DateTime<Utc>,
    /// 会话世代号；与 users 表当前值不一致则 session 失效。
    pub session_generation: i32,
}

/// 可公开的用户信息，不含会话世代号等敏感字段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublicUser {
    /// 用户主键。
    pub id: i32,
    /// 用户名。
    pub username: String,
    /// 邮箱地址。
    pub email: String,
    /// 对外展示名称（为空时 UI 回退展示 username）。
    pub display_name: Option<String>,
    /// 头像 URL。
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
