//! 笔记图片：原图随数据库备份；仅所有者或引用该图片的公开版本可读取。
use crate::api::error::AppError;
use crate::db::pool::get_conn;
use axum::{
    extract::{Multipart, Path},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};

async fn owner(headers: &HeaderMap) -> Option<i32> {
    let cookie = headers.get("cookie")?.to_str().ok()?;
    let token = crate::auth::session::parse_session_token(cookie)?;
    let user = crate::api::auth::get_user_by_token(token).await.ok()??;
    (user.role == crate::models::user::UserRole::Admin).then_some(user.id)
}

pub async fn upload(headers: HeaderMap, mut multipart: Multipart) -> Response {
    let Some(owner_id) = owner(&headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let ip = crate::api::rate_limit::get_client_ip(&headers).await;
    if crate::api::rate_limit::check_upload_limit(&ip).is_err() {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let data = match multipart.next_field().await {
        Ok(Some(field)) => match field.bytes().await {
            Ok(data) => data,
            _ => return StatusCode::BAD_REQUEST.into_response(),
        },
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    if data.len() > 5 * 1024 * 1024 {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let Some(mime) = crate::api::upload::detect_mime(&data) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };
    let check = data.clone();
    let valid = tokio::task::spawn_blocking(move || {
        if crate::api::image::upload_dimensions(&check, mime).is_err() {
            return false;
        }
        if mime == "image/webp" {
            return crate::infra::webp::decode(&check).is_ok();
        }
        let Ok(mut reader) =
            image::ImageReader::new(std::io::Cursor::new(check)).with_guessed_format()
        else {
            return false;
        };
        reader.limits(crate::api::image::image_reader_limits());
        reader.decode().is_ok()
    })
    .await
    .unwrap_or(false);
    if !valid {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(client) = get_conn().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let id = uuid::Uuid::new_v4();
    if client
        .execute(
            "INSERT INTO note_attachments(id,owner_id,mime,data) VALUES ($1,$2,$3,$4)",
            &[&id, &owner_id, &mime, &&data[..]],
        )
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    Json(serde_json::json!({"success":true,"url":format!("/note-media/{id}")})).into_response()
}

pub async fn read(headers: HeaderMap, Path(id): Path<String>) -> Response {
    let Ok(id) = uuid::Uuid::parse_str(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let owner_id = owner(&headers).await;
    let Ok(client) = get_conn().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let row = client.query_opt("SELECT mime,data FROM note_attachments a WHERE a.id=$1 AND
        (a.owner_id=$2 OR EXISTS (SELECT 1 FROM note_revision_attachments r JOIN notes n ON n.id=r.note_id
          WHERE r.attachment_id=a.id AND r.version=n.published_version AND n.deleted_at IS NULL))", &[&id,&owner_id]).await;
    let Ok(Some(row)) = row else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime: String = row.get(0);
    let data: Vec<u8> = row.get(1);
    // 每次重新鉴权，撤回发布后不残留可公开复用的浏览器/CDN缓存。
    (
        [
            ("content-type", mime.as_str()),
            ("cache-control", "private, no-store"),
            ("x-content-type-options", "nosniff"),
            ("referrer-policy", "same-origin"),
        ],
        data,
    )
        .into_response()
}

pub async fn sync_refs(
    tx: &deadpool_postgres::Transaction<'_>,
    owner_id: i32,
    id: i32,
    version: i32,
    html: &str,
) -> Result<(), AppError> {
    static MEDIA: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        // 输入已由 Markdown 渲染器清洗并序列化为双引号 HTML 属性。
        // 只把真正的本站图片 / 链接视为引用，代码示例和外站同路径不能开放私密图片。
        regex::Regex::new(
            r#"<(?:img|a)\b[^>]*\s(?:src|href)="/note-media/([0-9a-fA-F-]{36})(?:[?#][^"]*)?""#,
        )
        .expect("note media regex")
    });
    for cap in MEDIA.captures_iter(html) {
        let attachment = uuid::Uuid::parse_str(&cap[1])
            .map_err(|_| AppError::BadRequest("图片地址无效".into()))?;
        let changed = tx.execute("INSERT INTO note_revision_attachments(note_id,version,attachment_id)
            SELECT $1,$2,id FROM note_attachments WHERE id=$3 AND owner_id=$4 ON CONFLICT DO NOTHING", &[&id,&version,&attachment,&owner_id]).await.map_err(AppError::tx)?;
        if changed == 0 {
            let exists: bool = tx.query_one("SELECT EXISTS(SELECT 1 FROM note_revision_attachments WHERE note_id=$1 AND version=$2 AND attachment_id=$3)", &[&id,&version,&attachment]).await.map_err(AppError::query)?.get(0);
            if !exists {
                return Err(AppError::Forbidden("图片不存在或不属于当前账号"));
            }
        }
    }
    for path in crate::api::posts::helpers::extract_asset_paths(html, None) {
        tx.execute("INSERT INTO note_asset_refs(note_id,version,asset_id) SELECT $1,$2,id FROM assets WHERE path=$3 ON CONFLICT DO NOTHING", &[&id,&version,&path]).await.map_err(AppError::tx)?;
    }
    Ok(())
}
