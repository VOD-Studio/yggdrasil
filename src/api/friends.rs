//! 友链管理：Dioxus server functions。
//!
//! 前台 `/friends` 读取可见友链（`list_friend_links`，走 moka 缓存 + SSR 缓存失效，
//! 与文章/标签同架构）；后台 `/admin/friends` 做完整 CRUD。
//! 鉴权走 cookie session（`get_current_admin_user`），与其它后台 server-fn 一致。
//!
//! 排序用整数 `sort_order`（越小越靠前）；删除为物理删除（无回收站/软删除，
//! 友链无审计需求）。

#![allow(clippy::unused_unit, deprecated)]

use dioxus::prelude::*;

use crate::models::friend_link::FriendLink;

#[cfg(feature = "server")]
use crate::api::error::AppError;

/// 前台友链列表（仅活跃，按 sort_order 升序）。
///
/// 缓存命中直返；未命中查询 `friend_links` 活跃行后写缓存。公开接口，无需登录。
#[server(ListFriendLinks, "/api")]
pub async fn list_friend_links() -> Result<Vec<FriendLink>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::error::AppError;
        use crate::cache;
        use crate::db::pool::get_conn;

        if let Some(links) = cache::get_friend_links().await {
            return Ok(links);
        }
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let rows = client
            .query(
                "SELECT id, name, url, avatar_url, description, sort_order, is_active, \
                        created_at, updated_at \
                 FROM friend_links \
                 WHERE is_active \
                 ORDER BY sort_order, id",
                &[],
            )
            .await
            .map_err(AppError::query)?;
        let links: Vec<FriendLink> = rows.iter().map(row_to_friend_link).collect();
        cache::set_friend_links(links.clone()).await;
        Ok(links)
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 后台友链列表（含停用项，按 sort_order 升序）。仅 admin。
#[server(ListAllFriendLinks, "/api")]
pub async fn list_all_friend_links() -> Result<Vec<FriendLink>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;
        use crate::api::error::AppError;
        use crate::db::pool::get_conn;

        let _admin = get_current_admin_user().await?;
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let rows = client
            .query(
                "SELECT id, name, url, avatar_url, description, sort_order, is_active, \
                        created_at, updated_at \
                 FROM friend_links \
                 ORDER BY sort_order, id",
                &[],
            )
            .await
            .map_err(AppError::query)?;
        Ok(rows.iter().map(row_to_friend_link).collect())
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 新增友链。仅 admin。
///
/// 校验通过后 `INSERT ... RETURNING *`，并失效友链 moka 缓存 + `/friends` SSR 缓存
/// + 递增全局世代号，保证前台下次访问立即看到新卡片。
#[server(CreateFriendLink, "/api")]
pub async fn create_friend_link(
    name: String,
    url: String,
    avatar_url: Option<String>,
    description: String,
    sort_order: i32,
) -> Result<FriendLink, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;
        use crate::api::error::AppError;
        use crate::db::pool::get_conn;

        let _admin = get_current_admin_user().await?;
        let avatar_url = validate_link(&name, &url, avatar_url.as_deref(), &description)?;
        let (name, url, description) = trim_fields(name, url, description);

        let client = get_conn().await.map_err(AppError::db_conn)?;
        let row = client
            .query_one(
                "INSERT INTO friend_links \
                    (name, url, avatar_url, description, sort_order) \
                 VALUES ($1, $2, $3, $4, $5) \
                 RETURNING id, name, url, avatar_url, description, sort_order, is_active, \
                           created_at, updated_at",
                &[&name, &url, &avatar_url, &description, &sort_order],
            )
            .await
            .map_err(AppError::query)?;
        invalidate_friend_links_views();
        Ok(row_to_friend_link(&row))
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 更新友链（含启用状态）。仅 admin。
///
/// 行不存在返回 `AppError::NotFound`；成功后同样失效缓存与 SSR。
#[server(UpdateFriendLink, "/api")]
pub async fn update_friend_link(
    id: i32,
    name: String,
    url: String,
    avatar_url: Option<String>,
    description: String,
    sort_order: i32,
    is_active: bool,
) -> Result<FriendLink, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;
        use crate::api::error::AppError;
        use crate::db::pool::get_conn;

        let _admin = get_current_admin_user().await?;
        let avatar_url = validate_link(&name, &url, avatar_url.as_deref(), &description)?;
        let (name, url, description) = trim_fields(name, url, description);

        let client = get_conn().await.map_err(AppError::db_conn)?;
        let row = client
            .query_opt(
                "UPDATE friend_links \
                 SET name = $2, url = $3, avatar_url = $4, description = $5, \
                     sort_order = $6, is_active = $7, updated_at = NOW() \
                 WHERE id = $1 \
                 RETURNING id, name, url, avatar_url, description, sort_order, is_active, \
                           created_at, updated_at",
                &[
                    &id,
                    &name,
                    &url,
                    &avatar_url,
                    &description,
                    &sort_order,
                    &is_active,
                ],
            )
            .await
            .map_err(AppError::query)?;
        let Some(row) = row else {
            return Err(AppError::NotFound("友链不存在").into());
        };
        invalidate_friend_links_views();
        Ok(row_to_friend_link(&row))
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 删除友链（物理删除）。仅 admin。
///
/// id 不存在时静默成功（幂等删除），成功后失效缓存与 SSR。
#[server(DeleteFriendLink, "/api")]
pub async fn delete_friend_link(id: i32) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::api::auth::get_current_admin_user;
        use crate::api::error::AppError;
        use crate::db::pool::get_conn;

        let _admin = get_current_admin_user().await?;
        let client = get_conn().await.map_err(AppError::db_conn)?;
        client
            .execute("DELETE FROM friend_links WHERE id = $1", &[&id])
            .await
            .map_err(AppError::query)?;
        invalidate_friend_links_views();
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    unreachable!()
}

/// 校验友链字段并返回归一化后的头像 URL。
///
/// 规则（全部 `AppError::BadRequest`，消息原样透传给后台表单）：
/// - `name` trim 后非空且 ≤ 64 字符；
/// - `url` trim 后必须 `http://` / `https://` 前缀且 ≤ 512 字符（仅前缀检查，
///   不引入完整 URL 解析）；
/// - `avatar_url`：`None` 或 trim 后为空 → 归一化为 `None`；否则必须为 http(s) 链接或
///   安全的 `/uploads/` 素材路径；
/// - `description` trim 后 ≤ 200 字符。
#[cfg(feature = "server")]
fn is_local_asset_url(value: &str) -> bool {
    let Some(path) = value.strip_prefix("/uploads/") else {
        return false;
    };
    !path.is_empty() && !path.starts_with('/') && !path.contains("..") && !path.contains('\0')
}

#[cfg(feature = "server")]
fn validate_link(
    name: &str,
    url: &str,
    avatar_url: Option<&str>,
    description: &str,
) -> Result<Option<String>, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::BadRequest("友链名称不能为空".to_string()));
    }
    if name.trim().chars().count() > 64 {
        return Err(AppError::BadRequest(
            "友链名称过长（上限 64 字符）".to_string(),
        ));
    }
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(AppError::BadRequest(
            "友链 URL 必须为 http(s) 链接".to_string(),
        ));
    }
    if url.chars().count() > 512 {
        return Err(AppError::BadRequest(
            "URL 过长（上限 512 字符）".to_string(),
        ));
    }
    let avatar_url = match avatar_url.map(str::trim) {
        None | Some("") => None,
        Some(a) => {
            let is_http_url = a.starts_with("http://") || a.starts_with("https://");
            if !is_http_url && !is_local_asset_url(a) {
                return Err(AppError::BadRequest(
                    "头像 URL 必须为 http(s) 链接或 /uploads/ 素材路径".to_string(),
                ));
            }
            if a.chars().count() > 512 {
                return Err(AppError::BadRequest(
                    "头像 URL 过长（上限 512 字符）".to_string(),
                ));
            }
            Some(a.to_string())
        }
    };
    if description.trim().chars().count() > 200 {
        return Err(AppError::BadRequest(
            "描述过长（上限 200 字符）".to_string(),
        ));
    }
    Ok(avatar_url)
}

/// 归一化 name / url / description 的空白（校验通过后调用）。
#[cfg(feature = "server")]
fn trim_fields(name: String, url: String, description: String) -> (String, String, String) {
    (
        name.trim().to_string(),
        url.trim().to_string(),
        description.trim().to_string(),
    )
}

/// 写操作后的统一视图失效：友链 moka 缓存 + `/friends` SSR 缓存 + 全局世代号。
#[cfg(feature = "server")]
fn invalidate_friend_links_views() {
    crate::cache::invalidate_friend_links();
    crate::ssr_cache::invalidate_ssr_route("/friends");
    crate::ssr_cache::bump_global_generation();
}

/// 把 `friend_links` 行解析为 [`FriendLink`]。
#[cfg(feature = "server")]
fn row_to_friend_link(row: &tokio_postgres::Row) -> FriendLink {
    FriendLink {
        id: row.get("id"),
        name: row.get("name"),
        url: row.get("url"),
        avatar_url: row.get("avatar_url"),
        description: row.get("description"),
        sort_order: row.get("sort_order"),
        is_active: row.get("is_active"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    /// 断言错误为带指定子串消息的 BadRequest。
    fn assert_bad_request(err: AppError, needle: &str) {
        match err {
            AppError::BadRequest(m) => {
                assert!(m.contains(needle), "消息应为 {needle:?}，实际：{m}")
            }
            other => panic!("应为 BadRequest，实际：{other:?}"),
        }
    }

    #[test]
    fn validate_accepts_valid_link() {
        assert!(validate_link(
            "示例站",
            "https://example.com",
            Some("https://example.com/a.png"),
            "描述"
        )
        .is_ok());
    }

    #[test]
    fn validate_accepts_local_asset_avatar() {
        let avatar = validate_link(
            "示例站",
            "https://example.com",
            Some("/uploads/2026/08/10/avatar.webp"),
            "",
        )
        .expect("本地素材头像应通过友链字段校验");
        assert_eq!(avatar, Some("/uploads/2026/08/10/avatar.webp".to_string()));
    }

    #[test]
    fn validate_rejects_empty_name() {
        assert_bad_request(
            validate_link("  ", "https://example.com", None, "").unwrap_err(),
            "友链名称不能为空",
        );
    }

    #[test]
    fn validate_rejects_overlong_name() {
        let long = "名".repeat(65);
        assert_bad_request(
            validate_link(&long, "https://example.com", None, "").unwrap_err(),
            "友链名称过长",
        );
    }

    #[test]
    fn validate_rejects_non_http_url() {
        assert_bad_request(
            validate_link("示例站", "ftp://example.com", None, "").unwrap_err(),
            "友链 URL 必须为 http(s) 链接",
        );
    }

    #[test]
    fn validate_normalizes_empty_avatar() {
        let avatar = validate_link("示例站", "https://example.com", Some("   "), "").unwrap();
        assert_eq!(avatar, None);
        let avatar = validate_link("示例站", "https://example.com", None, "").unwrap();
        assert_eq!(avatar, None);
    }

    #[test]
    fn validate_rejects_bad_avatar() {
        assert_bad_request(
            validate_link(
                "示例站",
                "https://example.com",
                Some("javascript:alert(1)"),
                "",
            )
            .unwrap_err(),
            "头像 URL 必须为 http(s) 链接或 /uploads/ 素材路径",
        );
    }

    #[test]
    fn validate_rejects_unsafe_local_avatar() {
        assert_bad_request(
            validate_link(
                "示例站",
                "https://example.com",
                Some("/uploads/../secret.png"),
                "",
            )
            .unwrap_err(),
            "头像 URL 必须为 http(s) 链接或 /uploads/ 素材路径",
        );
    }

    #[test]
    fn validate_rejects_overlong_description() {
        let long = "描".repeat(201);
        assert_bad_request(
            validate_link("示例站", "https://example.com", None, &long).unwrap_err(),
            "描述过长",
        );
    }
}
