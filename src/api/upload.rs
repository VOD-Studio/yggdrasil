//! 图片上传：web 处理器 + 共享入库流水线。
//!
//! 两条入口共用 `process_image_upload`：
//! - web `POST /api/upload`（cookie 鉴权，multipart）—— 见 `upload_image`；
//! - MCP `POST /api/mcp/upload`（bearer 鉴权，multipart）—— 见 `mcp_upload_image`；
//! - MCP `upload_media` 工具（URL 抓取）—— 见 `src/mcp/tools/media.rs`。
//!
//! 流水线：magic bytes 检 MIME → 大小校验 → 尺寸/像素校验 → SHA-256 内容去重
//! （命中即复用）→ GIF/WebP 解码校验 → `spawn_blocking` 转码（GIF/WebP 原样，
//! JPEG/PNG 仅在更小时转 WebP）→ 按日期落盘 → assets 登记（含并发竞态补偿）。
//! JPEG/PNG 自动转 WebP（若体积更小则保留原格式），GIF/WebP 保持原样。
//! 文件按日期分目录存放于 `uploads/`。
//!
//! 内容去重（CAS）：以原始上传字节的 SHA-256 为内容指纹（`assets.content_hash`，
//! 唯一索引）。重复上传同一内容时复用已登记素材——同一行、同一文件，不重复
//! 落盘，响应带 `"reused": true`；并发同内容上传由唯一索引 + ON CONFLICT 兜底。
//! 仅精确去重：尺寸/压缩不同的视觉相似图不合并（那是感知哈希 pHash 的领域，
//! 有意不做）。
//! 本模块属于手动注册的 Axum 路由，仅在 `feature = "server"` 时可用。

#[cfg(feature = "server")]
use axum::extract::{ConnectInfo, Extension, Multipart};
#[cfg(feature = "server")]
use axum::http::{HeaderMap, StatusCode};
#[cfg(feature = "server")]
use axum::response::Response;
#[cfg(feature = "server")]
use axum::{response::IntoResponse, Json};
#[cfg(feature = "server")]
use serde_json::{json, Value};
#[cfg(feature = "server")]
use std::net::SocketAddr;

#[cfg(feature = "server")]
use crate::auth::session::parse_session_token;

#[cfg(feature = "server")]
const ALLOWED_MIME_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];
#[cfg(feature = "server")]
use crate::utils::server::MAX_FILE_SIZE;

// ===========================================================================
// web 处理器（cookie 鉴权）
// ===========================================================================

/// 构造统一的 JSON 错误响应：`{ "success": false, "error": msg }`。
/// pub(crate)：备份导入（api/database/backup.rs）复用同一契约。
#[cfg(feature = "server")]
pub(crate) fn upload_error<T: serde::Serialize>(
    status: StatusCode,
    msg: T,
) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "success": false, "error": msg })))
}

/// 处理图片上传的 Axum handler（web 端，cookie 鉴权）。
///
/// 流程：限流 → 解析 session → 校验 admin → 读取 multipart → 早拒非法声明类型 →
/// 读取字节 → 交给共享流水线 `process_image_upload`。
///
/// `ConnectInfo` 以可选扩展注入：`dioxus::server::serve()` 接管了 listener，
/// 无法调用 `into_make_service_with_connect_info::<SocketAddr>()`，所以这里
/// 与 `serve_image` 保持一致的优雅降级——扩展缺失时退回 `"unknown"` 限流桶。
/// 生产环境应在反向代理后部署并配置 `TRUSTED_PROXY_COUNT`，让限流拿到真实 IP。
#[cfg(feature = "server")]
pub async fn upload_image(
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // 0. Rate limit check
    let peer = connect_info.map(|Extension(ConnectInfo(addr))| addr);
    let ip = crate::api::rate_limit::get_client_ip_with_peer(&headers, peer).await;
    if let Err(msg) = crate::api::rate_limit::check_upload_limit(&ip) {
        return Err(upload_error(StatusCode::TOO_MANY_REQUESTS, msg));
    }

    // 1. Extract session from cookie
    let cookie_header = headers
        .get("cookie")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    let token = match parse_session_token(cookie_header) {
        Some(t) => t,
        None => {
            return Err(upload_error(StatusCode::UNAUTHORIZED, "未登录"));
        }
    };

    // 2. Verify admin
    let user = match crate::api::auth::get_user_by_token(token).await {
        Ok(Some(u)) => u,
        _ => {
            return Err(upload_error(StatusCode::UNAUTHORIZED, "会话已过期"));
        }
    };

    if user.role != crate::models::user::UserRole::Admin {
        return Err(upload_error(StatusCode::FORBIDDEN, "权限不足"));
    }

    // 3. Read multipart field
    let field = match multipart.next_field().await {
        Ok(Some(f)) => f,
        Ok(None) => {
            return Err(upload_error(StatusCode::BAD_REQUEST, "未找到文件"));
        }
        Err(e) => {
            tracing::error!("Multipart error: {:?}", e);
            return Err(upload_error(StatusCode::BAD_REQUEST, "文件读取失败"));
        }
    };

    // 4. 早拒非法声明类型（快速路径，避免读字节后再判）。
    //    流水线仍以 magic bytes 为权威——声明 jpeg 但实为 png 会被识别为 png 接受。
    let declared_mime = field.content_type().unwrap_or("").to_string();
    if !ALLOWED_MIME_TYPES.contains(&declared_mime.as_str()) {
        return Err(upload_error(StatusCode::BAD_REQUEST, "不支持的文件类型"));
    }

    // 原始文件名（客户端提供，仅作 assets 表展示字段）；需在 bytes() 消耗 field 前取出。
    let original_filename = field.file_name().map(|s| s.to_string());

    // 5. Read file data
    let data = match field.bytes().await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Read file error: {:?}", e);
            return Err(upload_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "文件读取失败",
            ));
        }
    };

    // 6. 共享入库流水线。
    match process_image_upload(data, original_filename).await {
        Ok(out) => Ok(Json(json!({
            "success": true,
            "url": out.url,
            "reused": out.reused
        }))),
        Err(e) => {
            let (status, msg) = e.status_and_msg();
            Err(upload_error(status, msg))
        }
    }
}

// ===========================================================================
// MCP 处理器（bearer 鉴权，multipart 二进制，带外传输）
// ===========================================================================

/// MCP bearer 上传错误 → JSON 响应（与 web 端格式一致）。
#[cfg(feature = "server")]
fn mcp_upload_error<T: serde::Serialize>(status: StatusCode, msg: T) -> Response {
    (status, Json(json!({ "success": false, "error": msg }))).into_response()
}

/// 处理图片上传的 Axum handler（MCP 端，bearer token 鉴权）。
///
/// 与 web `upload_image` 的区别：
/// - 鉴权用 `Authorization: Bearer ygg_...`（不是 cookie），经
///   [`crate::mcp::auth::resolve_bearer_principal`] 解析；
/// - 不挂 CSRF 中间件——bearer 在请求头里，浏览器不会自动附带，无 CSRF 风险；
/// - 限流按 token_id 计数（复用 MCP 的 token-keyed governor）。
///
/// 供 AI 客户端的 host/shell 直接 POST 二进制（Claude Code 的 Bash+curl 等），
/// 二进制不经 JSON-RPC，绕开 rmcp 4MiB 请求体上限。返回可直接嵌入 Markdown 的
/// `/uploads/...` URL。
#[cfg(feature = "server")]
pub async fn mcp_upload_image(headers: HeaderMap, mut multipart: Multipart) -> Response {
    // 1. bearer → principal（含 scope 校验：media 需要 write）。
    let principal = match crate::mcp::auth::resolve_bearer_principal(&headers).await {
        Ok(p) => p,
        Err(status) => return mcp_upload_error(status, "未授权或令牌无效"),
    };
    if !principal
        .scope
        .grants(crate::models::mcp_token::TokenScope::Write)
    {
        return mcp_upload_error(StatusCode::FORBIDDEN, "权限不足：需要 write 作用域");
    }

    // 2. token-keyed 限流（与 /mcp 中间件的 MCP_LIMITER 隔离：上传单独配额）。
    if let Err(msg) = crate::mcp::auth::check_mcp_upload_limit(&principal.token_id) {
        return mcp_upload_error(StatusCode::TOO_MANY_REQUESTS, msg);
    }

    // 3. 读取 multipart 字段。
    let field = match multipart.next_field().await {
        Ok(Some(f)) => f,
        Ok(None) => return mcp_upload_error(StatusCode::BAD_REQUEST, "未找到文件"),
        Err(e) => {
            tracing::error!("MCP multipart error: {:?}", e);
            return mcp_upload_error(StatusCode::BAD_REQUEST, "文件读取失败");
        }
    };

    // 早拒非法声明类型（快速路径）。
    let declared_mime = field.content_type().unwrap_or("").to_string();
    if !ALLOWED_MIME_TYPES.contains(&declared_mime.as_str()) {
        return mcp_upload_error(StatusCode::BAD_REQUEST, "不支持的文件类型");
    }

    let original_filename = field.file_name().map(|s| s.to_string());
    let data = match field.bytes().await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("MCP read file error: {:?}", e);
            return mcp_upload_error(StatusCode::INTERNAL_SERVER_ERROR, "文件读取失败");
        }
    };

    // 4. 共享入库流水线。
    match process_image_upload(data, original_filename).await {
        Ok(out) => Json(json!({
            "success": true,
            "url": out.url,
            "reused": out.reused,
            "width": out.width,
            "height": out.height,
            "mime": out.mime
        }))
        .into_response(),
        Err(e) => {
            let (status, msg) = e.status_and_msg();
            mcp_upload_error(status, msg)
        }
    }
}

// ===========================================================================
// 共享入库流水线
// ===========================================================================

/// 单条图片入库的结果。
#[cfg(feature = "server")]
#[derive(Debug, serde::Serialize)]
pub(crate) struct UploadOutcome {
    /// 可直接嵌入 Markdown 的相对 URL：`/uploads/YYYY/MM/DD/HHMMSS.uuid.ext`。
    pub url: String,
    /// 是否命中已登记素材（内容去重或并发竞态复用）。
    pub reused: bool,
    pub width: u32,
    pub height: u32,
    /// 最终 MIME（转码后；JPEG→WebP 成功则为 image/webp）。
    pub mime: String,
}

/// 流水线错误：映射到 HTTP 状态 + 脱敏消息（不泄露 SQL/路径细节）。
#[cfg(feature = "server")]
#[derive(Debug)]
pub(crate) enum UploadError {
    Empty,
    BadType,   // magic bytes 无法识别为 JPEG/PNG/GIF/WebP
    TooLarge,  // 超过 MAX_FILE_SIZE
    Oversized, // 像素超过 MAX_IMAGE_PIXELS
    Corrupt,   // GIF/WebP 解码失败
    /// 内部错误：携带静态上下文标签供 Debug 诊断（status_and_msg 统一返回脱敏消息）。
    #[allow(dead_code)]
    Internal(&'static str),
}

#[cfg(feature = "server")]
impl UploadError {
    /// 包装底层错误：服务端日志记完整 `{e}`，客户端只见静态 `ctx`。
    fn internal<E: std::fmt::Display>(e: E, ctx: &'static str) -> Self {
        tracing::error!("upload {ctx}: {e}");
        UploadError::Internal(ctx)
    }

    /// 映射到 (HTTP 状态, 脱敏消息)。
    fn status_and_msg(&self) -> (StatusCode, &'static str) {
        match self {
            UploadError::Empty => (StatusCode::BAD_REQUEST, "空文件"),
            UploadError::BadType => (StatusCode::BAD_REQUEST, "不支持的文件类型"),
            UploadError::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "文件超过大小限制"),
            UploadError::Oversized => (StatusCode::BAD_REQUEST, "图片尺寸超过上限"),
            UploadError::Corrupt => (StatusCode::BAD_REQUEST, "图片文件损坏或格式不正确"),
            UploadError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "文件保存失败"),
        }
    }
}

/// 单一图片入库流水线（web 上传 / MCP bearer 端点 / MCP URL 抓取共用）。
///
/// 输入：原始字节 + 可选展示文件名。**不信任客户端声明的 MIME**——以 magic
/// bytes 为唯一真相。输出可直接嵌入 Markdown 的 `/uploads/...` URL。
///
/// 步骤：大小校验 → magic bytes 检 MIME → 尺寸/像素校验 → SHA-256 去重（命中
/// 即复用，跳过最贵的转码）→ GIF/WebP 解码校验 → `spawn_blocking` 转码 →
/// 按日期落盘 → assets 登记（含并发竞态补偿，落败者删自己的文件复用胜出者）。
#[cfg(feature = "server")]
pub(crate) async fn process_image_upload(
    data: bytes::Bytes,
    original_filename: Option<String>,
) -> Result<UploadOutcome, UploadError> {
    if data.is_empty() {
        return Err(UploadError::Empty);
    }
    if data.len() > MAX_FILE_SIZE {
        return Err(UploadError::TooLarge);
    }

    // 1. magic bytes 检 MIME（不信任声明类型/扩展名）。
    let mime_type = detect_mime(&data).ok_or(UploadError::BadType)?;

    // 2. 仅读 header 校验尺寸/像素上限，并拿回 (w,h) 供 assets 登记，避免二次解析。
    //    超限直接拒绝，避免大图 decode 后被静默降级（原 fallback 存原图）。
    let (img_width, img_height) =
        crate::api::image::upload_dimensions(&data, mime_type).map_err(|msg| {
            tracing::warn!("upload dimensions check failed: {msg}");
            UploadError::Oversized
        })?;

    let is_gif = mime_type == "image/gif";
    let is_webp = mime_type == "image/webp";

    // 3. 内容去重（CAS）：对原始上传字节算 SHA-256，命中已登记素材直接复用，
    //    跳过 GIF/WebP 解码验证、转码与落盘（省下整个流程最贵的 CPU）。
    //    放在安全性校验之后、转码之前。命中时刷新 created_at/updated_at：
    //    重传代表使用意图，重启 7 天清理保护窗（PURGE_GRACE_DAYS 保护的是
    //    「刚上传还没被文章引用」的素材）。
    let content_hash = {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(&data))
    };
    {
        let client = crate::db::pool::get_conn()
            .await
            .map_err(|e| UploadError::internal(e, "dedup conn"))?;
        let reused = client
            .query_opt(
                "UPDATE assets SET created_at = NOW(), updated_at = NOW() \
                 WHERE content_hash = $1 RETURNING path",
                &[&content_hash],
            )
            .await
            .map_err(|e| UploadError::internal(e, "dedup check"))?;
        if let Some(row) = reused {
            let path: String = row.get("path");
            tracing::info!(
                "Image deduped: reuse {} (hash {})",
                path,
                &content_hash[..12]
            );
            return Ok(UploadOutcome {
                url: format!("/uploads/{}", path),
                reused: true,
                width: img_width,
                height: img_height,
                mime: mime_type.to_string(),
            });
        }
    }

    // 4. GIF/WebP 解码校验（不经过重编码的格式必须验真，防伪造扩展名的恶意文件）。
    //    GIF 走 image::load_from_memory 会完整解码，移到阻塞线程池避免拖住 async 运行时。
    if is_gif || is_webp {
        let validate_data = data.clone();
        let validate_mime = mime_type.to_string();
        let is_valid = tokio::task::spawn_blocking(move || {
            validate_raw_image(&validate_data, validate_mime.as_str())
        })
        .await
        .map_err(|e| UploadError::internal(e, "validate task"))?;
        if !is_valid {
            return Err(UploadError::Corrupt);
        }
    }

    // 5. 转码：GIF/WebP 原样；JPEG/PNG 仅在 WebP 更小时转。
    //    Bytes clone 廉价（引用计数 +1），move 进阻塞闭包无需全文件深拷贝。
    let (final_data, final_ext) = transcode(data, mime_type, is_gif, is_webp).await;

    // 6. 按上传时间组织目录：uploads/YYYY/MM/DD。
    //    chrono 的 DelayedFormat 实现 Display，可直接进 format!，省掉中间 String。
    let now = chrono::Utc::now();
    let date = now.format("%Y/%m/%d");
    let uuid_str = uuid::Uuid::new_v4().to_string();

    let dir_path = format!("uploads/{}", date);
    let file_name = format!("{}.{}.{}", now.format("%H%M%S"), uuid_str, final_ext);
    let file_path = format!("{}/{}", dir_path, file_name);
    let rel_path = format!("{}/{}", date, file_name);
    let url_path = format!("/uploads/{}", rel_path);
    let final_mime = mime_for_ext(&final_ext);

    if let Err(e) = tokio::fs::create_dir_all(&dir_path).await {
        return Err(UploadError::internal(e, "create dir"));
    }
    if let Err(e) = tokio::fs::write(&file_path, &final_data).await {
        return Err(UploadError::internal(e, "write file"));
    }

    tracing::info!("Image uploaded: {} ({} bytes)", file_path, final_data.len());

    // 7. 登记 assets 注册表。失败时补偿删除已落盘文件，避免产生未登记的孤儿文件。
    //    ON CONFLICT (content_hash) DO NOTHING 兜底并发竞态：两个请求同时上传同一
    //    新内容时会双双错过上面的去重检查，唯一索引保证只有一个 INSERT 成功；
    //    落败者删自己的落盘文件、复用胜出者的路径（返回 Some(reused_path)）。
    let registered: Result<Option<String>, UploadError> = async {
        let client = crate::db::pool::get_conn()
            .await
            .map_err(|e| UploadError::internal(e, "register conn"))?;
        // id 用 Uuid 类型直连 uuid 列（with-uuid-1 桥接），避免 String→uuid 序列化失败。
        let asset_id = uuid::Uuid::new_v4();
        let inserted = client
            .execute(
                "INSERT INTO assets (id, path, filename, mime, size_bytes, width, height, content_hash)\
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
                 ON CONFLICT (content_hash) DO NOTHING",
                &[
                    &asset_id,
                    &rel_path,
                    &original_filename.unwrap_or_else(|| file_name.clone()),
                    &final_mime,
                    &(final_data.len() as i64),
                    &(img_width as i32),
                    &(img_height as i32),
                    &content_hash,
                ],
            )
            .await
            .map_err(|e| UploadError::internal(e, "register asset"))?;
        if inserted == 0 {
            // 竞态落败：胜出者的行必然已提交（唯一索引冲突即可见），取其路径复用。
            let row = client
                .query_one(
                    "SELECT path FROM assets WHERE content_hash = $1",
                    &[&content_hash],
                )
                .await
                .map_err(|e| UploadError::internal(e, "select reused asset"))?;
            return Ok(Some(row.get("path")));
        }
        Ok(None)
    }
    .await;

    match registered {
        Ok(Some(reused_path)) => {
            let _ = tokio::fs::remove_file(&file_path).await;
            tracing::info!("Image deduped (concurrent race): reuse {}", reused_path);
            Ok(UploadOutcome {
                url: format!("/uploads/{}", reused_path),
                reused: true,
                width: img_width,
                height: img_height,
                mime: mime_type.to_string(),
            })
        }
        Ok(None) => Ok(UploadOutcome {
            url: url_path,
            reused: false,
            width: img_width,
            height: img_height,
            mime: final_mime.to_string(),
        }),
        Err(e) => {
            // 登记失败：补偿删除已落盘文件。
            let _ = tokio::fs::remove_file(&file_path).await;
            Err(e)
        }
    }
}

// ===========================================================================
// 图片处理辅助
// ===========================================================================

/// 从 magic bytes 检测 MIME 类型（不信任客户端声明的扩展名/Content-Type）。
#[cfg(feature = "server")]
pub(crate) fn detect_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

#[cfg(feature = "server")]
fn mime_to_ext(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "bin",
    }
}

#[cfg(feature = "server")]
fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "jpg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        _ => "image/webp",
    }
}

/// 解码验证 GIF/WebP 原始字节，确保不是伪造扩展名的恶意文件。
#[cfg(feature = "server")]
fn validate_raw_image(data: &[u8], mime_type: &str) -> bool {
    match mime_type {
        "image/webp" => crate::webp::decode(data).is_ok(),
        "image/gif" => image::load_from_memory(data).is_ok(),
        _ => true,
    }
}

/// 转码核心（同步）：GIF/WebP 保持原格式，JPEG/PNG 尝试转 WebP（更小才采用）。
#[cfg(feature = "server")]
fn transcode_image_blocking(
    data: &[u8],
    mime: &'static str,
    is_gif: bool,
    is_webp: bool,
) -> (Vec<u8>, String) {
    if is_gif {
        return (data.to_vec(), "gif".to_string());
    }
    if is_webp {
        return (data.to_vec(), "webp".to_string());
    }

    // JPEG/PNG → 尝试 WebP。
    let format = match mime {
        "image/jpeg" => image::ImageFormat::Jpeg,
        "image/png" => image::ImageFormat::Png,
        _ => image::ImageFormat::Jpeg,
    };
    let cursor = std::io::Cursor::new(data);
    let mut reader = image::ImageReader::with_format(cursor, format);
    reader.limits(crate::api::image::image_reader_limits());

    match reader.decode() {
        Ok(img) => {
            let config = crate::webp::WEBP_CONFIG.clone();
            match crate::webp::encode(&img, config.quality, config.method) {
                Ok(webp_data) if webp_data.len() < data.len() => {
                    tracing::info!(
                        "WebP conversion: {}x{} {} -> {} bytes",
                        img.width(),
                        img.height(),
                        data.len(),
                        webp_data.len()
                    );
                    (webp_data, "webp".to_string())
                }
                Ok(_) => {
                    // WebP 更大，保留原格式。
                    (data.to_vec(), mime_to_ext(mime).to_string())
                }
                Err(e) => {
                    tracing::warn!("WebP encode failed ({}), keeping original", e);
                    (data.to_vec(), mime_to_ext(mime).to_string())
                }
            }
        }
        // 到这里尺寸校验已通过（超限在 header 阶段被拒），decode 失败只能是真损坏。
        Err(e) => {
            tracing::warn!("Failed to decode image ({}), keeping original format", e);
            (data.to_vec(), mime_to_ext(mime).to_string())
        }
    }
}

/// 在阻塞线程中执行转码，避免阻塞 async 运行时。
/// Bytes clone 廉价（引用计数 +1）；join 失败（panic）时回退原格式。
#[cfg(feature = "server")]
async fn transcode(
    data: bytes::Bytes,
    mime: &'static str,
    is_gif: bool,
    is_webp: bool,
) -> (Vec<u8>, String) {
    let for_task = data.clone();
    match tokio::task::spawn_blocking(move || {
        transcode_image_blocking(&for_task, mime, is_gif, is_webp)
    })
    .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("transcode task panicked ({}), keeping original", e);
            (data.to_vec(), mime_to_ext(mime).to_string())
        }
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    #[test]
    fn filename_format_no_spaces() {
        let now_str = "120000";
        let uuid = "abc-123";
        let ext = "jpg";
        let file_name = format!("{}.{}.{}", now_str, uuid, ext);
        assert!(
            !file_name.contains(' '),
            "filename should not contain spaces: got '{}'",
            file_name
        );
    }

    #[test]
    fn should_use_webp_ext_for_non_gif() {
        let ext = "jpg";
        let mime = "image/jpeg";
        let is_gif = mime == "image/gif";
        let final_ext = if is_gif { ext } else { "webp" };
        assert_eq!(final_ext, "webp");
    }

    #[test]
    fn should_preserve_gif_ext() {
        let ext = "gif";
        let mime = "image/gif";
        let is_gif = mime == "image/gif";
        let final_ext = if is_gif { ext } else { "webp" };
        assert_eq!(final_ext, "gif");
    }

    #[test]
    fn convert_to_webp_produces_bytes() {
        let img = image::DynamicImage::new_rgb8(10, 10);
        let result = crate::webp::encode(&img, 85.0, 4).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn webp_roundtrip_from_rgba() {
        let img = image::DynamicImage::new_rgba8(2, 2);
        let webp_bytes = crate::webp::encode(&img, 85.0, 4).unwrap();
        let loaded = crate::webp::decode(&webp_bytes);
        assert!(loaded.is_ok());
    }

    #[test]
    fn mime_to_ext_maps_jpeg() {
        assert_eq!(super::mime_to_ext("image/jpeg"), "jpg");
    }

    #[test]
    fn mime_to_ext_maps_png() {
        assert_eq!(super::mime_to_ext("image/png"), "png");
    }

    #[test]
    fn mime_to_ext_maps_gif() {
        assert_eq!(super::mime_to_ext("image/gif"), "gif");
    }

    #[test]
    fn mime_to_ext_maps_webp() {
        assert_eq!(super::mime_to_ext("image/webp"), "webp");
    }

    #[test]
    fn mime_to_ext_falls_back_for_unknown_mime() {
        assert_eq!(super::mime_to_ext("image/avif"), "bin");
        assert_eq!(super::mime_to_ext("application/octet-stream"), "bin");
    }

    #[test]
    fn mime_for_ext_roundtrip() {
        assert_eq!(super::mime_for_ext("jpg"), "image/jpeg");
        assert_eq!(super::mime_for_ext("png"), "image/png");
        assert_eq!(super::mime_for_ext("gif"), "image/gif");
        assert_eq!(super::mime_for_ext("webp"), "image/webp");
    }

    #[test]
    fn detect_mime_jpeg() {
        assert_eq!(
            super::detect_mime(&[0xFF, 0xD8, 0xFF, 0xE0]),
            Some("image/jpeg")
        );
        assert_eq!(super::detect_mime(&[0x89, 0x50]), None);
    }

    #[test]
    fn detect_mime_png() {
        assert_eq!(
            super::detect_mime(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
            Some("image/png")
        );
        assert_eq!(super::detect_mime(&[0xFF, 0xD8]), None);
    }

    #[test]
    fn detect_mime_gif() {
        assert_eq!(super::detect_mime(b"GIF89a"), Some("image/gif"));
        assert_eq!(super::detect_mime(b"GIF87a"), Some("image/gif"));
        assert_eq!(super::detect_mime(b"GIF90a"), None);
    }

    #[test]
    fn detect_mime_webp() {
        let webp = b"RIFF\x00\x00\x00\x00WEBPVP8 ";
        assert_eq!(super::detect_mime(&webp[..12]), Some("image/webp"));
        assert_eq!(super::detect_mime(&[0xFF, 0xD8]), None);
    }

    #[test]
    fn detect_mime_unknown() {
        assert_eq!(super::detect_mime(b"hello world"), None);
        assert_eq!(super::detect_mime(&[]), None);
    }
}
