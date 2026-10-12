//! 管理员备份导入和下载 HTTP 处理器。

use super::files::{
    backup_partition_free_space, backup_path, has_valid_signature, is_valid_backup_filename,
    read_first_line, BACKUP_DIR,
};
use super::MULTIPART_FRAME_SLACK;

/// 单文件默认上限，可由 BACKUP_IMPORT_MAX_MB 覆盖。
const DEFAULT_IMPORT_MAX_MB: u64 = 512;

/// 路由请求体上限与处理器流式载荷计数共用的单文件字节上限。
pub(crate) fn import_max_bytes() -> u64 {
    std::env::var("BACKUP_IMPORT_MAX_MB")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|mb| *mb > 0)
        .unwrap_or(DEFAULT_IMPORT_MAX_MB)
        .saturating_mul(1024 * 1024)
}

/// 校验并清洗导入文件名：剥离路径分量（旧浏览器可能带 `C:\fakepath\` 前缀），
/// 要求 `.sql` 后缀 + 非隐藏文件 + 备份文件名白名单。纯函数便于单测。
fn sanitize_import_filename(raw: &str) -> Option<String> {
    let name = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    if name.len() > 255
        || !name.ends_with(".sql")
        || name.starts_with('.')
        || !is_valid_backup_filename(name)
    {
        return None;
    }
    Some(name.to_string())
}

/// Axum 处理器：导入备份（multipart 流式落盘，admin 鉴权 + 签名校验 + 同名拒绝）。
///
/// 设计要点（见 `docs/adr/0001-backup-import.md`）：
/// - 两步式：导入只入库 backups/ 并出现在列表，恢复仍走 [`super::restore_backup`] 管线；
/// - 导入时即校验签名（拒绝外来 SQL 入库），与恢复时的签名校验是两层独立防线；
/// - 流式写 `.tmp` + 原子 rename：半截文件永不入库，任一失败路径清理 tmp；
/// - 体积双保险：路由 `DefaultBodyLimit`（上限 + [`MULTIPART_FRAME_SLACK`]）
///   兜整个请求体，此处流式计数只计载荷字节，超限 413 早断；
/// - 仅 server 构建：纯 Axum 路由（在 main.rs 注册），无 WASM 消费者。
pub async fn import_backup(
    connect_info: Option<
        axum::extract::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>,
    >,
    headers: axum::http::HeaderMap,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, axum::Json<serde_json::Value>)>
{
    use crate::api::upload::upload_error;
    use axum::http::StatusCode;
    use tokio::io::AsyncWriteExt;

    // 0. 限流：复用 UPLOAD 桶（同为 admin 文件上传，不为低频导入单开配置面）。
    let peer = connect_info.map(|axum::extract::Extension(axum::extract::ConnectInfo(addr))| addr);
    let ip = crate::api::rate_limit::get_client_ip_with_peer(&headers, peer).await;
    if let Err(msg) = crate::api::rate_limit::check_upload_limit(&ip) {
        return Err(upload_error(StatusCode::TOO_MANY_REQUESTS, msg));
    }

    // 1. cookie session → admin（与 upload_image 同一校验链）。
    let cookie_header = headers
        .get("cookie")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let token = match crate::auth::session::parse_session_token(cookie_header) {
        Some(t) => t,
        None => return Err(upload_error(StatusCode::UNAUTHORIZED, "未登录")),
    };
    let user = match crate::api::auth::get_user_by_token(token).await {
        Ok(Some(u)) => u,
        _ => return Err(upload_error(StatusCode::UNAUTHORIZED, "会话已过期")),
    };
    if user.role != crate::models::user::UserRole::Admin {
        return Err(upload_error(StatusCode::FORBIDDEN, "权限不足"));
    }

    let max_bytes = import_max_bytes();

    // 2. Content-Length 早拒：明显超限不读 body 直接 413。
    let content_length = headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    if let Some(cl) = content_length {
        if cl > max_bytes + MULTIPART_FRAME_SLACK {
            return Err(upload_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "文件超过导入上限",
            ));
        }
    }

    // 3. 取 multipart 字段并清洗文件名。
    let mut field = match multipart.next_field().await {
        Ok(Some(f)) => f,
        Ok(None) => return Err(upload_error(StatusCode::BAD_REQUEST, "未找到文件")),
        Err(e) => {
            tracing::error!("backup import multipart error: {e:?}");
            return Err(upload_error(StatusCode::BAD_REQUEST, "文件读取失败"));
        }
    };
    let filename = match sanitize_import_filename(field.file_name().unwrap_or_default()) {
        Some(n) => n,
        None => {
            return Err(upload_error(
                StatusCode::BAD_REQUEST,
                "文件名不合法：仅接受以 .sql 结尾的备份文件名（字母/数字/下划线/点/连字符）",
            ))
        }
    };

    // 4. 同名冲突拒绝（不覆盖——覆盖是破坏性操作；用户可先删除旧文件再导入）。
    if let Err(e) = std::fs::create_dir_all(BACKUP_DIR) {
        tracing::error!("backup import: create dir failed: {e}");
        return Err(upload_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "无法创建备份目录",
        ));
    }
    let final_path = backup_path(&filename);
    if final_path.exists() {
        return Err(upload_error(StatusCode::CONFLICT, "已存在同名备份文件"));
    }

    // 5. 磁盘空间预检（Content-Length 已知且探测成功时；否则降级跳过）。
    if let (Some(cl), Some(free)) = (content_length, backup_partition_free_space()) {
        if cl > free {
            return Err(upload_error(
                StatusCode::INSUFFICIENT_STORAGE,
                "磁盘空间不足",
            ));
        }
    }

    // 6. 流式落盘 tmp：按载荷字节精确计数，超限/写失败/读失败都清理半截文件。
    let tmp_name = format!(
        ".import-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let tmp_path = backup_path(&tmp_name);
    let mut out = match tokio::fs::File::create(&tmp_path).await {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("backup import: create tmp failed: {e}");
            return Err(upload_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "无法写入备份目录",
            ));
        }
    };
    let mut written: u64 = 0;
    let stream_result: Result<(), (StatusCode, &'static str)> = loop {
        match field.chunk().await {
            Ok(Some(chunk)) => {
                written += chunk.len() as u64;
                if written > max_bytes {
                    break Err((StatusCode::PAYLOAD_TOO_LARGE, "文件超过导入上限"));
                }
                if let Err(e) = out.write_all(&chunk).await {
                    tracing::error!("backup import: write failed: {e}");
                    break Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "写入失败（磁盘可能已满）",
                    ));
                }
            }
            Ok(None) => break Ok(()),
            Err(e) => {
                tracing::error!("backup import: chunk error: {e:?}");
                break Err((StatusCode::BAD_REQUEST, "文件读取失败"));
            }
        }
    };
    drop(out);
    if let Err((status, msg)) = stream_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(upload_error(status, msg));
    }

    // 7. 导入时即签名校验：非本系统文件拒收且不留盘。
    let first_line = read_first_line(&tmp_path).unwrap_or_default();
    if !has_valid_signature(&first_line) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(upload_error(
            StatusCode::BAD_REQUEST,
            "非本系统生成的备份文件，拒绝导入",
        ));
    }

    // 8. 原子 rename 入库；rename 前二次冲突检查，收窄两个并发同名导入的竞态窗。
    if final_path.exists() {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(upload_error(StatusCode::CONFLICT, "已存在同名备份文件"));
    }
    if let Err(e) = std::fs::rename(&tmp_path, &final_path) {
        let _ = std::fs::remove_file(&tmp_path);
        tracing::error!("backup import: rename failed: {e}");
        return Err(upload_error(StatusCode::INTERNAL_SERVER_ERROR, "入库失败"));
    }

    tracing::info!(
        operator = %user.username,
        filename = %filename,
        size_bytes = written,
        "备份导入成功"
    );
    Ok(axum::Json(
        serde_json::json!({ "success": true, "filename": filename }),
    ))
}

/// Axum 处理器：下载备份文件（admin 鉴权 + 路径白名单）。
/// 仅 server 构建：纯 Axum 路由（在 main.rs 注册），无 WASM 消费者。
pub async fn download_backup(
    axum::extract::Path(filename): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<impl axum::response::IntoResponse, (axum::http::StatusCode, String)> {
    use axum::http::{header, StatusCode};

    // 鉴权
    let cookie_header = headers
        .get("cookie")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let token = crate::auth::session::parse_session_token(cookie_header).map(str::to_string);
    let token = match token {
        Some(t) => t,
        None => return Err((StatusCode::UNAUTHORIZED, "未登录".to_string())),
    };
    let user = match crate::api::auth::get_user_by_token(&token).await {
        Ok(Some(u)) => u,
        _ => return Err((StatusCode::UNAUTHORIZED, "会话已过期".to_string())),
    };
    if user.role != crate::models::user::UserRole::Admin {
        return Err((StatusCode::FORBIDDEN, "权限不足".to_string()));
    }

    // 路径白名单
    if !is_valid_backup_filename(&filename) {
        return Err((StatusCode::BAD_REQUEST, "无效的文件名".to_string()));
    }
    let path = backup_path(&filename);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "文件不存在".to_string()))?;
    let disposition = format!("attachment; filename=\"{}\"", filename);
    let content_type = if filename.ends_with(".tar.gz") {
        "application/gzip"
    } else {
        "application/sql; charset=utf-8"
    };
    Ok((
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static(content_type),
            ),
            (
                header::CONTENT_DISPOSITION,
                axum::http::HeaderValue::from_str(&disposition)
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("attachment")),
            ),
        ],
        axum::body::Body::from(bytes),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_filename_accepts_plain_backup_name() {
        assert_eq!(
            sanitize_import_filename("backup_20260816_200000.sql"),
            Some("backup_20260816_200000.sql".to_string())
        );
        // 255 字节边界接受
        let max_ok = format!("{}.sql", "a".repeat(251));
        assert!(sanitize_import_filename(&max_ok).is_some());
    }

    #[test]
    fn import_filename_strips_path_components() {
        // 旧浏览器可能送 `C:\fakepath\` 前缀；POSIX 路径同样剥掉，只留 basename。
        assert_eq!(
            sanitize_import_filename("C:\\fakepath\\auto_20260816_200000.sql"),
            Some("auto_20260816_200000.sql".to_string())
        );
        assert_eq!(
            sanitize_import_filename("/tmp/x/backup_1.sql"),
            Some("backup_1.sql".to_string())
        );
    }

    #[test]
    fn import_filename_rejects_non_sql_and_hidden() {
        // 配对 tar.gz 不可经导入入口塞进 backups/
        assert_eq!(sanitize_import_filename("x_uploads.tar.gz"), None);
        assert_eq!(sanitize_import_filename("noext"), None);
        // 隐藏文件与裸 ".sql" 拒绝（避开 tmp 文件命名空间）
        assert_eq!(sanitize_import_filename(".hidden.sql"), None);
        assert_eq!(sanitize_import_filename(".sql"), None);
        assert_eq!(sanitize_import_filename(""), None);
    }

    #[test]
    fn import_filename_rejects_illegal_chars_and_overlong() {
        assert_eq!(sanitize_import_filename("带中文.sql"), None);
        assert_eq!(sanitize_import_filename("has space.sql"), None);
        let long = format!("{}.sql", "a".repeat(252));
        assert_eq!(sanitize_import_filename(&long), None);
    }
}
