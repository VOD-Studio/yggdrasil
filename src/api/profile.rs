//! 个人信息（当前登录账号的资料与密码）的 Dioxus server function。
//!
//! 支撑 `/admin/profile` 页面：
//! - `get_profile`：读取当前用户公开信息 + `ADMIN_*` 环境变量是否激活
//!   （激活时邮箱/密码会在重启后被 env 覆盖，页面据此显示提示横幅）；
//! - `update_profile`：修改邮箱 / 显示名称 / 头像（username 为登录凭据，只读）；
//! - `change_password`：校验当前密码后改密，bump `session_generation` 使
//!   其他设备会话全部失效，同时刷新当前 token 的会话缓存保留本端登录。
//!
//! 资料字段（display_name/avatar_url/email）缓存在 SESSION_CACHE 的
//! SessionUser 里，故写操作成功后必须同步刷新当前 token 的缓存项，
//! 否则 `get_current_user` 会持续返回旧值直到缓存 TTL 过期。

#![allow(clippy::unused_unit, deprecated)]

use dioxus::prelude::*;

#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::api::auth::{admin_env_active, get_current_admin_user, validate_email, validate_password};
#[cfg(feature = "server")]
use crate::auth::{password, session};
#[cfg(feature = "server")]
use crate::auth::session::get_session_from_ctx;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;
use crate::models::user::PublicUser;
#[cfg(feature = "server")]
use crate::models::user::SessionUser;

/// 个人信息查询响应。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetProfileResponse {
    /// 当前登录用户的公开信息。
    pub user: PublicUser,
    /// `ADMIN_*` 环境变量是否激活（激活时邮箱/密码重启后以 env 为准）。
    pub admin_env_active: bool,
}

/// 资料更新响应。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateProfileResponse {
    /// 操作是否成功。
    pub success: bool,
    /// 提示信息。
    pub message: String,
    /// 更新后的用户公开信息（成功时返回，供客户端刷新全局上下文）。
    pub user: Option<PublicUser>,
}

/// 修改密码响应。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangePasswordResponse {
    /// 操作是否成功。
    pub success: bool,
    /// 提示信息。
    pub message: String,
}

#[cfg(feature = "server")]
/// 归一化显示名称：trim 后为空 → None；超过 50 字符报错。
fn normalize_display_name(input: Option<String>) -> Result<Option<String>, String> {
    let trimmed = input.unwrap_or_default().trim().to_string();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > 50 {
        return Err("显示名称长度不能超过 50 个字符".to_string());
    }
    Ok(Some(trimmed))
}

#[cfg(feature = "server")]
/// 归一化头像 URL：trim 后为空 → None；否则必须为 http(s) 链接或
/// 安全的 `/uploads/` 素材路径（与友链头像同一规则），上限 512 字符。
fn normalize_avatar_url(input: Option<String>) -> Result<Option<String>, String> {
    let trimmed = input.unwrap_or_default().trim().to_string();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let is_http_url = trimmed.starts_with("http://") || trimmed.starts_with("https://");
    let is_uploads_path = trimmed.starts_with("/uploads/");
    if !is_http_url && !is_uploads_path {
        return Err("头像必须为 http(s) 链接或 /uploads/ 素材路径".to_string());
    }
    if trimmed.chars().count() > 512 {
        return Err("头像 URL 过长（上限 512 字符）".to_string());
    }
    Ok(Some(trimmed))
}

/// 获取当前登录用户的资料。
///
/// Dioxus server function，注册在 `/api` 路径下。
#[server(GetProfile, "/api")]
pub async fn get_profile() -> Result<GetProfileResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let user = get_current_admin_user().await?;
        Ok(GetProfileResponse {
            admin_env_active: admin_env_active(),
            user: user.into(),
        })
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 更新当前登录用户的资料（邮箱 / 显示名称 / 头像）。
///
/// 校验失败或邮箱被占用返回 `Ok(UpdateProfileResponse{success:false,...})`，
/// 与注册/登录的「业务拒绝走 Ok」约定一致。
/// Dioxus server function，注册在 `/api` 路径下。
#[server(UpdateProfile, "/api")]
pub async fn update_profile(
    email: String,
    display_name: Option<String>,
    avatar_url: Option<String>,
) -> Result<UpdateProfileResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        let fail = |message: String| UpdateProfileResponse {
            success: false,
            message,
            user: None,
        };

        let token = get_session_from_ctx().ok_or(AppError::Unauthorized("未登录"))?;
        let user = get_current_admin_user().await?;

        if let Err(e) = validate_email(&email) {
            return Ok(fail(e));
        }
        let display_name = match normalize_display_name(display_name) {
            Ok(v) => v,
            Err(e) => return Ok(fail(e)),
        };
        let avatar_url = match normalize_avatar_url(avatar_url) {
            Ok(v) => v,
            Err(e) => return Ok(fail(e)),
        };

        let client = get_conn().await.map_err(AppError::db_conn)?;

        // 邮箱唯一性预检（DB 唯一索引仍是最终兜底）。
        let email_taken: bool = client
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM users WHERE email = $1 AND id <> $2)",
                &[&email, &user.id],
            )
            .await
            .map_err(AppError::query)?
            .get(0);
        if email_taken {
            return Ok(fail("邮箱已被占用".to_string()));
        }

        client
            .execute(
                "UPDATE users SET email = $2, display_name = $3, avatar_url = $4 WHERE id = $1",
                &[&user.id, &email, &display_name, &avatar_url],
            )
            .await
            .map_err(AppError::query)?;

        // 资料字段缓存在 SESSION_CACHE 的 SessionUser 里，同步刷新当前 token
        // 的缓存项，避免 get_current_user 在缓存 TTL 内返回旧值。同用户的其他
        // 会话（其他设备）在 TTL 内可能持有旧展示字段——纯展示数据，可接受。
        let updated = SessionUser {
            email,
            display_name,
            avatar_url,
            ..user
        };
        crate::cache::set_session_user(&session::hash_token(&token), updated.clone()).await;

        Ok(UpdateProfileResponse {
            success: true,
            message: "资料已保存".to_string(),
            user: Some(updated.into()),
        })
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 修改当前登录用户的密码。
///
/// 校验当前密码 → 更新哈希并 bump `session_generation`（其他设备会话全部
/// 失效）→ 删除该用户除当前 token 外的所有会话 → 刷新当前 token 的会话缓存
/// （写入新世代号），本端保持登录。Argon2 计算走 `spawn_blocking`。
/// Dioxus server function，注册在 `/api` 路径下。
#[server(ChangePassword, "/api")]
pub async fn change_password(
    current_password: String,
    new_password: String,
) -> Result<ChangePasswordResponse, ServerFnError> {
    #[cfg(feature = "server")]
    {
        // Argon2 校验是高价操作，与登录/注册同挂严格限流。
        if let Some(ctx) = dioxus::fullstack::FullstackContext::current() {
            let headers = ctx.parts_mut().headers.clone();
            let ip = crate::api::rate_limit::get_client_ip(&headers).await;
            if let Err(msg) = crate::api::rate_limit::check_strict_limit(&ip) {
                return Ok(ChangePasswordResponse {
                    success: false,
                    message: msg,
                });
            }
        }

        let fail = |message: &str| ChangePasswordResponse {
            success: false,
            message: message.to_string(),
        };

        let token = get_session_from_ctx().ok_or(AppError::Unauthorized("未登录"))?;
        let user = get_current_admin_user().await?;

        if let Err(e) = validate_password(&new_password) {
            return Ok(fail(&e));
        }
        if current_password == new_password {
            return Ok(fail("新密码不能与当前密码相同"));
        }

        let mut client = get_conn().await.map_err(AppError::db_conn)?;

        let password_hash: String = client
            .query_one("SELECT password_hash FROM users WHERE id = $1", &[&user.id])
            .await
            .map_err(AppError::query)?
            .get(0);

        // Argon2 是 memory-hard 计算，必须在 spawn_blocking 中执行。
        let current_for_verify = current_password.clone();
        let hash_for_verify = password_hash.clone();
        let valid = tokio::task::spawn_blocking(move || {
            password::verify_password(&current_for_verify, &hash_for_verify)
        })
        .await
        .map_err(|_| AppError::Internal("密码处理任务失败"))?
        .map_err(|_| AppError::Internal("密码处理失败"))?;
        if !valid {
            return Ok(fail("当前密码不正确"));
        }

        let new_for_hash = new_password.clone();
        let new_hash = tokio::task::spawn_blocking(move || password::hash_password(&new_for_hash))
            .await
            .map_err(|_| AppError::Internal("密码处理任务失败"))?
            .map_err(|_| AppError::Internal("密码处理失败"))?;

        let token_hash = session::hash_token(&token);

        // 事务内：更新哈希 + bump 世代号 + 删除其他会话。当前会话行保留，
        // 靠下方缓存刷新写入新世代号维持有效。
        let tx = client.transaction().await.map_err(AppError::query)?;
        let new_generation: i32 = tx
            .query_one(
                "UPDATE users SET password_hash = $2, session_generation = session_generation + 1 \
                 WHERE id = $1 RETURNING session_generation",
                &[&user.id, &new_hash],
            )
            .await
            .map_err(AppError::query)?
            .get(0);
        tx.execute(
            "DELETE FROM sessions WHERE user_id = $1 AND token_hash <> $2",
            &[&user.id, &token_hash],
        )
        .await
        .map_err(AppError::query)?;
        tx.commit().await.map_err(AppError::query)?;

        // get_user_by_token 每次命中缓存都会回查 users.session_generation，
        // 必须把当前 token 的缓存项刷到新世代号，否则本端会被视为已登出。
        let refreshed = SessionUser {
            session_generation: new_generation,
            ..user
        };
        crate::cache::set_session_user(&token_hash, refreshed).await;

        Ok(ChangePasswordResponse {
            success: true,
            message: "密码已修改，其他设备已退出登录".to_string(),
        })
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn display_name_normalization() {
        assert_eq!(normalize_display_name(None), Ok(None));
        assert_eq!(normalize_display_name(Some("   ".to_string())), Ok(None));
        assert_eq!(
            normalize_display_name(Some("  Alice ".to_string())),
            Ok(Some("Alice".to_string()))
        );
        // 恰好 50 字符（含多字节）通过。
        let fifty = "名".repeat(50);
        assert_eq!(
            normalize_display_name(Some(fifty.clone())),
            Ok(Some(fifty))
        );
        // 51 字符拒绝。
        let over = "a".repeat(51);
        assert!(normalize_display_name(Some(over)).is_err());
    }

    #[test]
    fn avatar_url_normalization() {
        assert_eq!(normalize_avatar_url(None), Ok(None));
        assert_eq!(normalize_avatar_url(Some("  ".to_string())), Ok(None));
        assert_eq!(
            normalize_avatar_url(Some("/uploads/2026/08/a.webp".to_string())),
            Ok(Some("/uploads/2026/08/a.webp".to_string()))
        );
        assert_eq!(
            normalize_avatar_url(Some("https://example.com/a.png".to_string())),
            Ok(Some("https://example.com/a.png".to_string()))
        );
        // 非 http(s)/非 /uploads/ 一律拒绝（含 javascript: 等伪协议）。
        assert!(normalize_avatar_url(Some("javascript:alert(1)".to_string())).is_err());
        assert!(normalize_avatar_url(Some("ftp://example.com/a".to_string())).is_err());
        assert!(normalize_avatar_url(Some("/etc/passwd".to_string())).is_err());
        // 超长拒绝。
        let over = format!("/uploads/{}", "a".repeat(512));
        assert!(normalize_avatar_url(Some(over)).is_err());
    }
}
