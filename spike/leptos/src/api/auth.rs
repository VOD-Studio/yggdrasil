//! 认证接口：读取 `session` Cookie、校验会话，并提供当前用户查询。
//!
//! 登录不在本原型范围内：测试时在主应用登录（同一主机，Cookie 不区分端口），
//! 或直接向 sessions 表插入会话行。与主应用不同，这里不使用 moka 会话缓存。

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::models::user::PublicUser;
#[cfg(feature = "ssr")]
use crate::models::user::{SessionUser, UserRole};

/// 当前用户查询响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentUserResponse {
    /// 当前已登录用户的公开信息；未登录时为 `None`。
    pub user: Option<PublicUser>,
}

/// 从 Cookie 头解析 `session` 值。
#[cfg(feature = "ssr")]
fn parse_session_token(cookie_header: &str) -> Option<&str> {
    cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|pair| pair.strip_prefix("session="))
        .filter(|v| !v.is_empty())
}

/// 对 session token 做 SHA-256 并输出十六进制，与主应用 `auth::session::hash_token` 一致。
#[cfg(feature = "ssr")]
fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// 从当前请求头读取 session token；没有时返回 None。
#[cfg(feature = "ssr")]
async fn session_token_from_request() -> Result<Option<String>, AppError> {
    let headers: http::HeaderMap = leptos_axum::extract()
        .await
        .map_err(|e| AppError::internal("extract headers", e))?;
    Ok(headers
        .get_all(http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|h| parse_session_token(h).map(str::to_string)))
}

/// 按 token 查询会话用户：会话未过期、世代号一致且角色为 admin 才返回。
#[cfg(feature = "ssr")]
async fn get_user_by_token(token: &str) -> Result<Option<SessionUser>, AppError> {
    use crate::server::db::{get_conn, query_err};

    let token_hash = hash_token(token);
    let client = get_conn().await?;
    let row = client
        .query_opt(
            "SELECT u.id, u.username, u.email, u.display_name, u.avatar_url, u.role, u.created_at, u.session_generation
             FROM sessions s
             JOIN users u ON s.user_id = u.id
             WHERE s.token_hash = $1 AND s.expires_at > NOW()
               AND s.generation = u.session_generation",
            &[&token_hash],
        )
        .await
        .map_err(query_err)?;

    let Some(row) = row else {
        return Ok(None);
    };
    let role_str: String = row.get("role");
    let role = match UserRole::from_str(&role_str) {
        Some(UserRole::Admin) => UserRole::Admin,
        _ => return Ok(None),
    };
    Ok(Some(SessionUser {
        id: row.get("id"),
        username: row.get("username"),
        email: row.get("email"),
        display_name: row.get("display_name"),
        avatar_url: row.get("avatar_url"),
        role,
        created_at: row.get("created_at"),
        session_generation: row.get("session_generation"),
    }))
}

/// 获取当前登录用户并要求其为 admin，否则返回 Unauthorized / Forbidden。
///
/// 供其它服务端接口内部调用。
#[cfg(feature = "ssr")]
pub async fn require_admin() -> Result<SessionUser, AppError> {
    let token = session_token_from_request()
        .await?
        .ok_or(AppError::Unauthorized)?;
    let user = get_user_by_token(&token)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if user.role != UserRole::Admin {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

/// 当前会话用户；没有 Cookie 或会话失效时返回 None。供评论等接口区分登录路径。
#[cfg(feature = "ssr")]
pub async fn current_session_user() -> Result<Option<SessionUser>, AppError> {
    match session_token_from_request().await? {
        Some(token) => get_user_by_token(&token).await,
        None => Ok(None),
    }
}

/// 获取当前登录用户；未登录或会话失效时 `user` 为 None。
#[server(endpoint = "auth/current_user")]
pub async fn get_current_user() -> Result<CurrentUserResponse, AppError> {
    let Some(token) = session_token_from_request().await? else {
        return Ok(CurrentUserResponse { user: None });
    };
    let user = get_user_by_token(&token).await?.map(PublicUser::from);
    Ok(CurrentUserResponse { user })
}
