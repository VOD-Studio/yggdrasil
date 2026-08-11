//! 图片服务的 Axum 处理器与处理流水线。
//!
//! 支持按宽度/高度、缩略图、旋转角度、输出格式/质量动态处理图片，
//! 使用内存（moka）+ 磁盘两级缓存加速响应。
//! WebP 编解码走 `zenwebp`（`image` crate 未启用 WebP feature）。
//! 本模块属于手动注册的 Axum 路由，仅在 `feature = "server"` 时可用。

#[cfg(feature = "server")]
use axum::{
    extract::{ConnectInfo, Extension, Path, Query},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
#[cfg(feature = "server")]
use bytes::Bytes;
#[cfg(feature = "server")]
use moka::future::Cache;
#[cfg(feature = "server")]
use moka::sync::Cache as SyncCache;
#[cfg(feature = "server")]
use serde::Deserialize;
#[cfg(feature = "server")]
use std::net::SocketAddr;
#[cfg(feature = "server")]
use std::sync::LazyLock;

#[cfg(feature = "server")]
fn etag_for(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(data);
    format!("\"{}\"", hex::encode(&hash[..16]))
}

#[cfg(feature = "server")]
fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    let trimmed = if_none_match.trim();
    if trimmed == "*" {
        return true;
    }
    trimmed
        .split(',')
        .map(|s| s.trim().trim_start_matches("W/"))
        .any(|candidate| candidate == etag)
}

#[cfg(feature = "server")]
/// 图片单边（宽或高）尺寸上限，单位像素。
///
/// 启动时从 `MAX_IMAGE_DIMENSION` 环境变量读取，默认 8192。
/// 只设下限 512（防误调到危险小值导致正常图都传不上），无上限（完全信任运维）。
/// 低于下限时 clamp 回 512 并打 WARN。
pub static MAX_IMAGE_DIMENSION: LazyLock<u32> = LazyLock::new(|| {
    const DEFAULT: u32 = 8192;
    const MIN: u32 = 512;
    let (val, clamped) = std::env::var("MAX_IMAGE_DIMENSION")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|v| if v < MIN { (MIN, true) } else { (v, false) })
        .unwrap_or((DEFAULT, false));
    if clamped {
        tracing::warn!(
            "MAX_IMAGE_DIMENSION was clamped from {} to {} (minimum {})",
            std::env::var("MAX_IMAGE_DIMENSION").unwrap_or_default(),
            val,
            MIN
        );
    }
    tracing::info!("Image dimension limit loaded: {}", val);
    val
});
#[cfg(feature = "server")]
const DEFAULT_JPEG_QUALITY: u8 = 85;
#[cfg(feature = "server")]
/// 允许处理的最大图片像素数（默认约 7k x 7k）。
///
/// 启动时从 `MAX_IMAGE_PIXELS` 环境变量读取，默认 50_000_000。
/// 只设下限 1_000_000（防误调），无上限。
/// ⚠️ 此值同时决定单图解码内存缓冲（max_alloc = pixels × 4 + 1MB），
///    默认 50M 像素对应约 200MB/图，上调前确认部署环境内存。
pub static MAX_IMAGE_PIXELS: LazyLock<u32> = LazyLock::new(|| {
    const DEFAULT: u32 = 50_000_000;
    const MIN: u32 = 1_000_000;
    let (val, clamped) = std::env::var("MAX_IMAGE_PIXELS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|v| if v < MIN { (MIN, true) } else { (v, false) })
        .unwrap_or((DEFAULT, false));
    if clamped {
        tracing::warn!(
            "MAX_IMAGE_PIXELS was clamped from {} to {} (minimum {})",
            std::env::var("MAX_IMAGE_PIXELS").unwrap_or_default(),
            val,
            MIN
        );
    }
    tracing::info!("Image pixel limit loaded: {}", val);
    val
});

#[cfg(feature = "server")]
#[derive(Debug, Clone)]
/// 缓存条目，保存处理后的图片字节与 Content-Type。
struct CachedImage {
    data: Bytes,
    content_type: HeaderValue,
}

#[cfg(feature = "server")]
static IMAGE_CACHE: LazyLock<Cache<String, CachedImage>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(100)
        .time_to_idle(std::time::Duration::from_secs(300))
        .build()
});

#[cfg(feature = "server")]
#[derive(Debug, Deserialize, Clone, Hash, Eq, PartialEq, Default)]
/// 图片处理查询参数。
pub struct ImageParams {
    /// 限制最大宽度。
    pub w: Option<u32>,
    /// 限制最大高度。
    pub h: Option<u32>,
    /// 缩略图尺寸，格式 `WxH`。
    pub thumb: Option<String>,
    /// 旋转角度，仅允许 0/90/180/270。
    pub rotate: Option<u16>,
    /// 输出格式：`jpeg`/`jpg`、`png`、`webp`。
    pub format: Option<String>,
    /// 输出质量，范围 1-100。
    pub quality: Option<u8>,
}

#[cfg(feature = "server")]
impl ImageParams {
    fn is_empty(&self) -> bool {
        self.w.is_none()
            && self.h.is_none()
            && self.thumb.is_none()
            && self.rotate.is_none()
            && self.format.is_none()
            && self.quality.is_none()
    }

    fn cache_key(&self, path: &str) -> String {
        use std::fmt::Write as _;
        // 旧实现 vec![path.to_string()] + 最多 6 个 format! + join，最坏 9 次堆分配。
        // 单次 with_capacity + write! 直写，1 次分配。
        let mut key = String::with_capacity(path.len() + 64);
        key.push_str(path);
        if let Some(w) = self.w {
            let _ = write!(key, "|w={}", w);
        }
        if let Some(h) = self.h {
            let _ = write!(key, "|h={}", h);
        }
        if let Some(ref thumb) = self.thumb {
            let _ = write!(key, "|thumb={}", thumb);
        }
        if let Some(r) = self.rotate {
            let _ = write!(key, "|rotate={}", r);
        }
        if let Some(ref fmt) = self.format {
            let _ = write!(key, "|format={}", fmt);
        }
        if let Some(q) = self.quality {
            let _ = write!(key, "|quality={}", q);
        }
        key
    }

    /// 校验参数合法性，返回 HTTP 400 状态码表示非法。
    fn validate(&self) -> Result<(), StatusCode> {
        if let Some(dim) = self.w {
            if dim == 0 || dim > *MAX_IMAGE_DIMENSION {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        if let Some(dim) = self.h {
            if dim == 0 || dim > *MAX_IMAGE_DIMENSION {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        if let Some(r) = self.rotate {
            if !matches!(r, 0 | 90 | 180 | 270) {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        if let Some(ref fmt) = self.format {
            if !matches!(fmt.to_lowercase().as_str(), "jpeg" | "jpg" | "png" | "webp") {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        if let Some(ref thumb) = self.thumb {
            let parts: Vec<&str> = thumb.split('x').collect();
            if parts.len() != 2 {
                return Err(StatusCode::BAD_REQUEST);
            }
            let tw: u32 = parts[0].parse().map_err(|_| StatusCode::BAD_REQUEST)?;
            let th: u32 = parts[1].parse().map_err(|_| StatusCode::BAD_REQUEST)?;
            if tw == 0 || th == 0 || tw > *MAX_IMAGE_DIMENSION || th > *MAX_IMAGE_DIMENSION {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        if let Some(q) = self.quality {
            if q == 0 || q > 100 {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
        Ok(())
    }
}

#[cfg(feature = "server")]
fn detect_format(path: &str) -> ImageFmt {
    // 仅对路径后缀做大小写不敏感匹配，避免 to_lowercase() 对整条路径的 String 分配。
    // path 形如 `uploads/2026/06/22/abc.webp`，rsplit('.') 取最后一段后缀即可。
    let ext = path.rsplit('.').next().unwrap_or("");
    if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") {
        ImageFmt::Jpeg
    } else if ext.eq_ignore_ascii_case("png") {
        ImageFmt::Png
    } else if ext.eq_ignore_ascii_case("webp") {
        ImageFmt::WebP
    } else if ext.eq_ignore_ascii_case("gif") {
        ImageFmt::Gif
    } else {
        ImageFmt::Jpeg
    }
}

/// detect_format 的轻量返回类型，避免在热路径上构造 image::ImageFormat。
#[cfg(feature = "server")]
type ImageFmt = image::ImageFormat;

#[cfg(feature = "server")]
fn content_type(format: image::ImageFormat) -> HeaderValue {
    match format {
        image::ImageFormat::Jpeg => HeaderValue::from_static("image/jpeg"),
        image::ImageFormat::Png => HeaderValue::from_static("image/png"),
        image::ImageFormat::WebP => HeaderValue::from_static("image/webp"),
        image::ImageFormat::Gif => HeaderValue::from_static("image/gif"),
        _ => HeaderValue::from_static("application/octet-stream"),
    }
}

#[cfg(feature = "server")]
fn image_response(
    data: Bytes,
    content_type: HeaderValue,
    cache_control: &'static str,
    headers: &HeaderMap,
) -> Response {
    let etag = etag_for(&data);
    // etag 形如 `"deadbeef..."`（引号 + hex），都是合法 token-char，from_str 不可能失败。
    // 用 expect 说明恒成立的不变量，避免裸 unwrap 触发 lint。
    let etag_value = HeaderValue::from_str(&etag)
        .expect("etag 仅含 ASCII hex 与双引号，必然是合法的 HeaderValue");

    if let Some(if_none_match) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
    {
        if etag_matches(if_none_match, &etag) {
            return (
                StatusCode::NOT_MODIFIED,
                [
                    (header::ETAG, etag_value.clone()),
                    (
                        header::CACHE_CONTROL,
                        HeaderValue::from_static(cache_control),
                    ),
                    (header::CONTENT_TYPE, content_type),
                    // nosniff 防止浏览器对 content-type 错配的图片字节做 MIME sniff（M2）。
                    (
                        header::X_CONTENT_TYPE_OPTIONS,
                        HeaderValue::from_static("nosniff"),
                    ),
                ],
            )
                .into_response();
        }
    }

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static(cache_control),
            ),
            (header::ETAG, etag_value),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
        ],
        data,
    )
        .into_response()
}

#[cfg(feature = "server")]
fn check_image_dimensions(width: u32, height: u32) -> Result<(), StatusCode> {
    if width == 0 || height == 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > u64::from(*MAX_IMAGE_PIXELS) {
        tracing::warn!(
            "Image dimensions too large: {}x{} ({} pixels, max {})",
            width,
            height,
            pixels,
            *MAX_IMAGE_PIXELS
        );
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    Ok(())
}

#[cfg(feature = "server")]
/// 仅读取 header 校验上传图片的尺寸/像素是否超限，并返回 (width, height)。
///
/// - 输入是原始字节 + MIME,内部按 MIME 分发只解析 header 拿尺寸(不解码像素)
/// - 返回带友好中文提示的 `&'static str`(供上传接口直接回给用户)
///
/// 上传入口三种格式在此统一拦截至尺寸上限;WebP 走 `zenwebp` header,
/// JPEG/PNG/GIF 走 `image` crate 的 `into_dimensions`(均只读 header)。
/// 尺寸随结果返回，供 `upload_image` 校验通过后直接写入 assets 表，避免二次解析。
pub(crate) fn upload_dimensions(data: &[u8], mime_type: &str) -> Result<(u32, u32), &'static str> {
    let dims = read_dimensions_by_mime(data, mime_type)?;
    let (width, height) = dims;
    if width == 0 || height == 0 {
        return Err("图片文件损坏或格式不正确");
    }
    let pixels = u64::from(width) * u64::from(height);
    let max_dim = *MAX_IMAGE_DIMENSION;
    let max_pixels = *MAX_IMAGE_PIXELS;
    if width > max_dim || height > max_dim || pixels > u64::from(max_pixels) {
        tracing::warn!(
            "Uploaded image too large: {}x{} ({} pixels, max {}x{} / {} pixels)",
            width,
            height,
            pixels,
            max_dim,
            max_dim,
            max_pixels
        );
        return Err("图片尺寸过大,请压缩后再上传");
    }
    Ok(dims)
}

#[cfg(feature = "server")]
/// 按 MIME 只读 header 拿 (width, height)。失败返回损坏错误。
fn read_dimensions_by_mime(data: &[u8], mime_type: &str) -> Result<(u32, u32), &'static str> {
    match mime_type {
        "image/webp" => read_webp_dimensions(data).ok_or("图片文件损坏或格式不正确"),
        "image/jpeg" | "image/png" | "image/gif" => {
            let format = match mime_type {
                "image/jpeg" => image::ImageFormat::Jpeg,
                "image/png" => image::ImageFormat::Png,
                _ => image::ImageFormat::Gif,
            };
            read_image_dimensions(data, format).ok_or("图片文件损坏或格式不正确")
        }
        // 上游已用 ALLOWED_MIME_TYPES 白名单拦截,理论不到这里
        _ => Err("图片文件损坏或格式不正确"),
    }
}

/// 读 WebP header 拿尺寸（zenwebp 只解析 RIFF header，不解码像素）。
#[cfg(feature = "server")]
fn read_webp_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let decoder = zenwebp::WebPDecoder::build(data).ok()?;
    let info = decoder.info();
    Some((info.width, info.height))
}

/// 读 image crate 支持格式（jpeg/png/gif）的 header 拿尺寸。
#[cfg(feature = "server")]
fn read_image_dimensions(data: &[u8], format: image::ImageFormat) -> Option<(u32, u32)> {
    let reader = image::ImageReader::with_format(std::io::Cursor::new(data), format);
    reader.into_dimensions().ok()
}

/// 构造统一的 image::Limits（宽度/高度/分配上限），供 upload 与 image serving 共享。
#[cfg(feature = "server")]
pub(crate) fn image_reader_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(*MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(*MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(*MAX_IMAGE_PIXELS as u64 * 4 + 1024 * 1024);
    limits
}

#[cfg(feature = "server")]
fn process_image(
    img: image::DynamicImage,
    params: &ImageParams,
    original_format: image::ImageFormat,
) -> Result<(Vec<u8>, HeaderValue), StatusCode> {
    check_image_dimensions(img.width(), img.height())?;
    let mut img = img;

    // Rotate first, then resize
    if let Some(degrees) = params.rotate {
        img = match degrees {
            90 => img.rotate90(),
            180 => img.rotate180(),
            270 => img.rotate270(),
            _ => img,
        };
    }

    // Resize by max dimensions (keep aspect ratio)
    if params.w.is_some() || params.h.is_some() {
        let max_w = params.w.unwrap_or(img.width());
        let max_h = params.h.unwrap_or(img.height());
        if img.width() > max_w || img.height() > max_h {
            img = img.resize(max_w, max_h, image::imageops::FilterType::Lanczos3);
        }
    }

    // Thumbnail: fit-in-box (same semantics as resize, but both dimensions required)
    if let Some(ref thumb_spec) = params.thumb {
        let parts: Vec<&str> = thumb_spec.split('x').collect();
        if parts.len() == 2 {
            let tw: u32 = parts[0].parse().map_err(|_| StatusCode::BAD_REQUEST)?;
            let th: u32 = parts[1].parse().map_err(|_| StatusCode::BAD_REQUEST)?;
            if tw > 0 && th > 0 && tw <= *MAX_IMAGE_DIMENSION && th <= *MAX_IMAGE_DIMENSION {
                img = img.thumbnail(tw, th);
            }
        }
    }

    // Output format (case-insensitive)
    let output_format = match params.format.as_deref().map(str::to_lowercase).as_deref() {
        Some("webp") => image::ImageFormat::WebP,
        Some("png") => image::ImageFormat::Png,
        Some("jpeg") | Some("jpg") => image::ImageFormat::Jpeg,
        _ => original_format,
    };

    let quality = params.quality.unwrap_or(DEFAULT_JPEG_QUALITY);

    let mut buf = std::io::Cursor::new(Vec::new());
    match output_format {
        image::ImageFormat::Jpeg => {
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
            img.write_with_encoder(encoder)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        image::ImageFormat::WebP => {
            let config = crate::webp::WEBP_CONFIG.clone();
            let webp_quality = params.quality.map(|q| q as f32).unwrap_or(config.quality);
            let webp_data = crate::webp::encode(&img, webp_quality, config.method)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            buf = std::io::Cursor::new(webp_data);
        }
        _ => {
            img.write_to(&mut buf, output_format)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    let ct = content_type(output_format);
    Ok((buf.into_inner(), ct))
}

#[cfg(feature = "server")]
fn process_image_blocking(
    data: Vec<u8>,
    params: ImageParams,
    path: String,
) -> Result<(Vec<u8>, HeaderValue), StatusCode> {
    let original_format = detect_format(&path);

    let img = if original_format == image::ImageFormat::WebP {
        match crate::webp::decode(&data) {
            Ok(img) => {
                check_image_dimensions(img.width(), img.height())?;
                img
            }
            Err(e) => {
                // decode 失败不再降级返回原始字节（可能是构造的畸形文件，配合 nosniff
                // 构成内容混淆面），直接报错让上层返回 422（M3）。
                tracing::warn!("WebP decode failed ({}), rejecting", e);
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
        }
    } else {
        let cursor = std::io::Cursor::new(&data);
        let mut reader = image::ImageReader::with_format(cursor, original_format);
        reader.limits(image_reader_limits());
        match reader.decode() {
            Ok(img) => img,
            Err(e) => {
                tracing::warn!("Image decode failed ({}), rejecting", e);
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
        }
    };

    process_image(img, &params, original_format)
}

#[cfg(feature = "server")]
/// 校验请求路径不会逃出 uploads 目录。
///
/// 两层校验：① 子串级拒绝 `..`/`\0`/绝对路径前缀；② 对已存在文件用 canonicalize
/// 确认解析后真实路径仍在 uploads 目录内（纵深防御，抵御符号链接等绕过）。
/// 文件不存在或 uploads 目录不存在时只做第一层校验（由后续 read 报 404）。
async fn is_path_safe(path: &str) -> bool {
    if path.contains("..") || path.contains('\0') || path.starts_with('/') {
        return false;
    }
    let candidate = std::path::Path::new("uploads").join(path);
    let uploads_root = match tokio::fs::canonicalize("uploads").await {
        Ok(p) => p,
        Err(_) => return true, // uploads 目录不存在（测试环境），只靠第一层校验。
    };
    match tokio::fs::canonicalize(&candidate).await {
        Ok(resolved) => resolved.starts_with(&uploads_root),
        Err(_) => true, // 文件不存在，交由后续读取报 404。
    }
}

#[cfg(feature = "server")]
use axum::http::HeaderMap;

#[cfg(feature = "server")]
const CACHE_DIR: &str = "uploads/.cache";

#[cfg(feature = "server")]
/// 素材删除时清理其派生缓存。
///
/// - 内存处理缓存 `IMAGE_CACHE`：key 形如 `{path}|w=..|thumb=..`，按前缀批量失效；
/// - 尺寸缓存 `IMAGE_DIMENSIONS_CACHE`：key 即相对路径，直接失效。
///
/// 磁盘派生缓存（`uploads/.cache/cache_<sha256(key)>`）文件名是整 key 的哈希，
/// 无法按路径前缀枚举；这些死条目由 `image_cache_cleanup` 后台任务
/// 按容量/年龄回收，不在删除路径上处理。
pub async fn invalidate_asset_caches(rel_path: &str) {
    let prefix = format!("{}|", rel_path);
    // moka 的 invalidate_entries_if 是同步谓词求值，返回 Result（仅谓词 panic 时 Err）。
    let _ = IMAGE_CACHE.invalidate_entries_if(move |k, _| k.starts_with(&prefix));
    IMAGE_DIMENSIONS_CACHE.invalidate(rel_path);
}

#[cfg(feature = "server")]
fn disk_cache_base(cache_key: &str) -> String {
    // 使用 SHA-256 生成稳定的磁盘缓存文件名，避免进程重启后 DefaultHasher 随机 seed
    // 导致旧缓存无法命中且文件无限累积。
    use sha2::Digest;
    let hash = sha2::Sha256::digest(cache_key.as_bytes());
    let hash_hex = hex::encode(hash);
    format!("{}/cache_{}", CACHE_DIR, hash_hex)
}

#[cfg(feature = "server")]
async fn read_disk_cache(cache_key: &str) -> Option<CachedImage> {
    let base = disk_cache_base(cache_key);
    let data = tokio::fs::read(format!("{}.dat", base)).await.ok()?;
    let ct_str = tokio::fs::read_to_string(format!("{}.ct", base))
        .await
        .ok()
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let content_type = HeaderValue::from_str(&ct_str).ok()?;
    Some(CachedImage {
        data: Bytes::from(data),
        content_type,
    })
}

#[cfg(feature = "server")]
async fn write_disk_cache(cache_key: &str, cached: &CachedImage) {
    let base = disk_cache_base(cache_key);
    if let Err(e) = tokio::fs::create_dir_all(CACHE_DIR).await {
        tracing::warn!("Failed to create cache dir: {:?}", e);
        return;
    }
    let ct_str = cached
        .content_type
        .to_str()
        .unwrap_or("application/octet-stream");

    // 原子写：先写 .tmp 再 rename，避免并发请求读到 .dat 与 .ct 错配的半成品（L5）。
    let dat_path = format!("{}.dat", base);
    let ct_path = format!("{}.ct", base);
    let dat_tmp = format!("{}.dat.tmp", base);
    let ct_tmp = format!("{}.ct.tmp", base);

    // 两个临时文件都写成功后才 rename；任一失败则清理半成品。
    let writes_ok = tokio::fs::write(&dat_tmp, &cached.data).await.is_ok()
        && tokio::fs::write(&ct_tmp, ct_str).await.is_ok();

    if !writes_ok {
        let _ = tokio::fs::remove_file(&dat_tmp).await;
        let _ = tokio::fs::remove_file(&ct_tmp).await;
        tracing::warn!("Failed to write disk cache temp files at {}", base);
        return;
    }

    let rename_dat = tokio::fs::rename(&dat_tmp, &dat_path).await;
    let rename_ct = tokio::fs::rename(&ct_tmp, &ct_path).await;
    if rename_dat.is_err() || rename_ct.is_err() {
        // rename 失败：清理可能残留的临时文件与目标，避免读到错配内容。
        let _ = tokio::fs::remove_file(&dat_tmp).await;
        let _ = tokio::fs::remove_file(&ct_tmp).await;
        tracing::warn!("Failed to atomically rename disk cache at {}", base);
    }
}

#[cfg(feature = "server")]
/// 图片访问与动态处理的 Axum handler。
///
/// 依次执行：限流 → 路径安全校验 → 参数校验 → 无参数时直接返回原文件 →
/// 查询内存缓存 → 查询磁盘缓存 → 读取并解码 → 处理 → 写入两级缓存 → 返回。
pub async fn serve_image(
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(path): Path<String>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Response {
    let peer = connect_info.map(|Extension(ConnectInfo(addr))| addr);
    let ip = crate::api::rate_limit::get_client_ip_with_peer(&headers, peer).await;
    if let Err(status) = crate::api::rate_limit::check_image_limit(&ip) {
        return status.into_response();
    }

    if !is_path_safe(&path).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let file_path = format!("uploads/{}", path);

    // Validate params
    if let Err(status) = params.validate() {
        return status.into_response();
    }

    // No processing params: return raw file with long-lived cache headers.
    if params.is_empty() {
        // 原始分支也限制大小，避免读取超大文件撑爆内存（M3）。上限 20MB
        // 覆盖正常上传图（上传侧 MAX_FILE_SIZE=5MB），拒绝异常大文件。
        const MAX_RAW_BYTES: u64 = 20 * 1024 * 1024;
        return match tokio::fs::metadata(&file_path).await {
            Ok(meta) if meta.len() > MAX_RAW_BYTES => StatusCode::PAYLOAD_TOO_LARGE.into_response(),
            Ok(_) => match tokio::fs::read(&file_path).await {
                Ok(data) => {
                    let ct = content_type(detect_format(&path));
                    image_response(
                        Bytes::from(data),
                        ct,
                        "public, max-age=31536000, immutable",
                        &headers,
                    )
                }
                Err(_) => StatusCode::NOT_FOUND.into_response(),
            },
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        };
    }

    let cache_key = params.cache_key(&path);
    if let Some(cached) = IMAGE_CACHE.get(&cache_key).await {
        return image_response(
            cached.data.clone(),
            cached.content_type,
            "public, max-age=86400",
            &headers,
        );
    }

    if let Some(cached) = read_disk_cache(&cache_key).await {
        let data = cached.data.clone();
        let content_type = cached.content_type.clone();
        let _ = IMAGE_CACHE.insert(cache_key.clone(), cached).await;
        return image_response(data, content_type, "public, max-age=86400", &headers);
    }

    let data = match tokio::fs::read(&file_path).await {
        Ok(d) => d,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    // Offload decode + resize + encode to the blocking pool so the async
    // runtime stays responsive to other requests.
    let (processed, content_type) =
        match tokio::task::spawn_blocking(move || process_image_blocking(data, params, path)).await
        {
            Ok(Ok(r)) => r,
            Ok(Err(status)) => return status.into_response(),
            Err(_) => {
                tracing::error!("Image processing task panicked");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let processed = Bytes::from(processed);
    let cached = CachedImage {
        data: processed,
        content_type,
    };
    let _ = IMAGE_CACHE.insert(cache_key.clone(), cached.clone()).await;
    write_disk_cache(&cache_key, &cached).await;

    image_response(
        cached.data,
        cached.content_type,
        "public, max-age=86400",
        &headers,
    )
}

/// 图片尺寸缓存（moka sync）。key = 相对路径如 "2026/06/22/x.webp"。
/// 用 sync cache 而非 future cache：render_markdown_enhanced 是同步函数，不能 .await。
#[cfg(feature = "server")]
static IMAGE_DIMENSIONS_CACHE: LazyLock<SyncCache<String, (u32, u32)>> = LazyLock::new(|| {
    let ttl = std::env::var("IMAGE_DIMENSIONS_CACHE_TTL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(std::time::Duration::from_secs)
        .unwrap_or(std::time::Duration::from_secs(86400)); // 默认 24h
    SyncCache::builder().time_to_live(ttl).build()
});

/// 读取图片真实尺寸（只读 header，不解码像素）。
///
/// - `rel_path`：相对路径如 "2026/06/22/x.webp"（不含 /uploads/ 前缀和 query）
/// - 优先查缓存；miss 时读文件、解析 header、写入缓存
/// - 失败返回 None（调用方回退到不设 aspect-ratio）
#[cfg(feature = "server")]
pub fn get_image_dimensions(rel_path: &str) -> Option<(u32, u32)> {
    if let Some(dims) = IMAGE_DIMENSIONS_CACHE.get(rel_path) {
        return Some(dims);
    }
    let full_path = std::path::Path::new("uploads").join(rel_path);
    // 只读取文件头部：尺寸信息位于各格式 header（PNG IHDR / GIF / WebP RIFF /
    // JPEG SOF），无需把整张多 MB 图片读进内存。JPEG 的 SOF 可能跟在
    // EXIF/APPn 标记之后，64 KiB 足以覆盖常见情况。
    let file = std::fs::File::open(&full_path).ok()?;
    use std::io::Read;
    let mut header = Vec::new();
    file.take(65_536).read_to_end(&mut header).ok()?;
    let dims = read_dimensions_from_bytes(&header, rel_path)?;
    IMAGE_DIMENSIONS_CACHE.insert(rel_path.to_string(), dims);
    Some(dims)
}

/// 按扩展名分发：webp 走 zenwebp header，gif/png/jpeg 走 image crate。
#[cfg(feature = "server")]
fn read_dimensions_from_bytes(data: &[u8], path: &str) -> Option<(u32, u32)> {
    let ext = std::path::Path::new(path)
        .extension()?
        .to_str()?
        .to_lowercase();
    match ext.as_str() {
        "webp" => read_webp_dimensions(data),
        "jpg" | "jpeg" => read_image_dimensions(data, image::ImageFormat::Jpeg),
        "png" => read_image_dimensions(data, image::ImageFormat::Png),
        "gif" => read_image_dimensions(data, image::ImageFormat::Gif),
        _ => None,
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn read_webp_dimensions_from_bytes() {
        // 构造一个 16x9 的 webp
        let img = image::DynamicImage::new_rgb8(16, 9);
        let webp_bytes = crate::webp::encode(&img, 85.0, 2).unwrap();
        let dims = read_dimensions_from_bytes(&webp_bytes, "test.webp");
        assert_eq!(dims, Some((16, 9)));
    }

    #[test]
    fn read_png_dimensions_from_bytes() {
        let img = image::DynamicImage::new_rgb8(32, 24);
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        let dims = read_dimensions_from_bytes(&buf.into_inner(), "test.png");
        assert_eq!(dims, Some((32, 24)));
    }

    #[test]
    fn read_dimensions_unknown_extension_returns_none() {
        let dims = read_dimensions_from_bytes(b"not an image", "test.xyz");
        assert_eq!(dims, None);
    }

    // —— upload_dimensions：统一上传尺寸/像素上限校验 ——

    /// 构造指定尺寸的 PNG 字节(内存占用 = 尺寸,仅用于 header 校验测试)。
    fn make_png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::DynamicImage::new_rgb8(w, h);
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn upload_dimensions_accepts_small_png() {
        let data = make_png_bytes(100, 100);
        assert!(upload_dimensions(&data, "image/png").is_ok());
    }

    #[test]
    fn upload_dimensions_accepts_boundary_png() {
        // 用 7000×7000(≈49M 像素):单边 7000 < 默认 8192 上限,且总像素 < 默认 50M 上限,应放行。
        // 注意不能直接用 *MAX_IMAGE_DIMENSION 做正方形边——8192²≈67M 会触发像素上限拒绝。
        let data = make_png_bytes(7000, 7000);
        assert!(upload_dimensions(&data, "image/png").is_ok());
    }

    #[test]
    fn upload_dimensions_rejects_oversized_width() {
        // 单边超限:(上限+1)×1,像素远低于上限,但单边越界
        let data = make_png_bytes(*MAX_IMAGE_DIMENSION + 1, 1);
        let err = upload_dimensions(&data, "image/png").unwrap_err();
        assert!(err.contains("尺寸过大"));
    }

    #[test]
    fn upload_dimensions_rejects_oversized_height() {
        let data = make_png_bytes(1, *MAX_IMAGE_DIMENSION + 1);
        let err = upload_dimensions(&data, "image/png").unwrap_err();
        assert!(err.contains("尺寸过大"));
    }

    #[test]
    fn upload_dimensions_accepts_small_webp() {
        let img = image::DynamicImage::new_rgb8(64, 48);
        let webp_bytes = crate::webp::encode(&img, 85.0, 2).unwrap();
        assert!(upload_dimensions(&webp_bytes, "image/webp").is_ok());
    }

    #[test]
    fn upload_dimensions_accepts_gif() {
        // image crate 默认启用 gif feature,into_dimensions 可读 GIF header
        let img = image::DynamicImage::new_rgb8(32, 32);
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Gif).unwrap();
        assert!(upload_dimensions(&buf.into_inner(), "image/gif").is_ok());
    }

    #[test]
    fn upload_dimensions_rejects_corrupt_bytes() {
        // 非 magic bytes,与现有损坏文件校验文案对齐
        let err = upload_dimensions(b"not an image at all", "image/png").unwrap_err();
        assert_eq!(err, "图片文件损坏或格式不正确");
    }

    #[test]
    fn read_dimensions_by_mime_dispatches_webp() {
        let img = image::DynamicImage::new_rgb8(16, 9);
        let webp_bytes = crate::webp::encode(&img, 85.0, 2).unwrap();
        assert_eq!(
            read_dimensions_by_mime(&webp_bytes, "image/webp").unwrap(),
            (16, 9)
        );
    }

    #[test]
    fn image_params_validate_valid_defaults() {
        let params = ImageParams::default();
        assert!(params.validate().is_ok());
    }

    #[test]
    fn image_params_validate_valid_width() {
        let params = ImageParams {
            w: Some(100),
            ..Default::default()
        };
        assert!(params.validate().is_ok());
    }

    #[test]
    fn image_params_validate_zero_width_rejected() {
        let params = ImageParams {
            w: Some(0),
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn image_params_validate_oversized_width_rejected() {
        let params = ImageParams {
            w: Some(*MAX_IMAGE_DIMENSION + 1),
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn image_params_validate_valid_rotation() {
        for angle in [0, 90, 180, 270] {
            let params = ImageParams {
                rotate: Some(angle),
                ..Default::default()
            };
            assert!(params.validate().is_ok(), "angle {} should be valid", angle);
        }
    }

    #[test]
    fn image_params_validate_invalid_rotation_rejected() {
        let params = ImageParams {
            rotate: Some(45),
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn image_params_validate_valid_format() {
        for fmt in &["jpeg", "jpg", "png", "webp", "JPEG", "PNG"] {
            let params = ImageParams {
                format: Some(fmt.to_string()),
                ..Default::default()
            };
            assert!(params.validate().is_ok(), "format {} should be valid", fmt);
        }
    }

    #[test]
    fn image_params_validate_invalid_format_rejected() {
        let params = ImageParams {
            format: Some("gif".to_string()),
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn image_params_validate_valid_thumbnail() {
        let params = ImageParams {
            thumb: Some("200x150".to_string()),
            ..Default::default()
        };
        assert!(params.validate().is_ok());
    }

    #[test]
    fn image_params_validate_invalid_thumbnail_rejected() {
        let params = ImageParams {
            thumb: Some("200".to_string()),
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn image_params_validate_valid_quality() {
        let params = ImageParams {
            quality: Some(85),
            ..Default::default()
        };
        assert!(params.validate().is_ok());
    }

    #[test]
    fn image_params_validate_zero_quality_rejected() {
        let params = ImageParams {
            quality: Some(0),
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn image_params_validate_over_100_quality_rejected() {
        let params = ImageParams {
            quality: Some(101),
            ..Default::default()
        };
        assert!(params.validate().is_err());
    }

    #[tokio::test]
    async fn is_path_safe_normal() {
        assert!(is_path_safe("images/photo.jpg").await);
        assert!(is_path_safe("2024/01/photo.png").await);
    }

    #[tokio::test]
    async fn is_path_safe_rejects_parent_dir() {
        assert!(!is_path_safe("../etc/passwd").await);
        assert!(!is_path_safe("foo/../../bar").await);
    }

    #[tokio::test]
    async fn is_path_safe_rejects_null_bytes() {
        assert!(!is_path_safe("foo\0bar").await);
    }

    #[tokio::test]
    async fn is_path_safe_rejects_absolute_path() {
        assert!(!is_path_safe("/etc/passwd").await);
    }

    #[test]
    fn detect_format_jpeg() {
        assert!(matches!(
            detect_format("photo.jpg"),
            image::ImageFormat::Jpeg
        ));
        assert!(matches!(
            detect_format("photo.jpeg"),
            image::ImageFormat::Jpeg
        ));
        assert!(matches!(
            detect_format("PHOTO.JPG"),
            image::ImageFormat::Jpeg
        ));
    }

    #[test]
    fn detect_format_png() {
        assert!(matches!(detect_format("icon.png"), image::ImageFormat::Png));
    }

    #[test]
    fn detect_format_webp() {
        assert!(matches!(
            detect_format("anim.webp"),
            image::ImageFormat::WebP
        ));
    }

    #[test]
    fn detect_format_defaults_to_jpeg() {
        assert!(matches!(
            detect_format("file.xyz"),
            image::ImageFormat::Jpeg
        ));
    }

    #[test]
    fn cache_key_differs_for_different_params() {
        let p1 = ImageParams {
            w: Some(100),
            ..Default::default()
        };
        let p2 = ImageParams {
            w: Some(200),
            ..Default::default()
        };
        assert_ne!(p1.cache_key("img.jpg"), p2.cache_key("img.jpg"));
    }

    #[test]
    fn is_empty_true_when_all_none() {
        let params = ImageParams::default();
        assert!(params.is_empty());
    }

    #[test]
    fn is_empty_false_when_any_set() {
        let params = ImageParams {
            w: Some(100),
            ..Default::default()
        };
        assert!(!params.is_empty());
    }

    #[test]
    fn disk_cache_base_is_deterministic() {
        let key = "path|w=800";
        let base1 = disk_cache_base(key);
        let base2 = disk_cache_base(key);
        assert_eq!(base1, base2);
        assert!(base1.starts_with("uploads/.cache/cache_"));
    }

    #[test]
    fn disk_cache_base_differs_for_different_keys() {
        let base1 = disk_cache_base("path|w=800");
        let base2 = disk_cache_base("path|w=1200");
        assert_ne!(base1, base2);
    }

    #[test]
    fn process_image_blocking_resizes_png() {
        let img = image::DynamicImage::new_rgb8(100, 100);
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        let data = buf.into_inner();

        let params = ImageParams {
            w: Some(50),
            format: Some("webp".to_string()),
            ..Default::default()
        };

        let (out, ct) = process_image_blocking(data, params, "test.png".to_string()).unwrap();
        assert!(!out.is_empty());
        assert_eq!(ct, HeaderValue::from_static("image/webp"));
    }

    #[test]
    fn image_response_includes_cache_headers() {
        let resp = image_response(
            Bytes::from(vec![1, 2, 3]),
            HeaderValue::from_static("image/webp"),
            "public, max-age=86400",
            &HeaderMap::new(),
        );
        assert_eq!(resp.status(), StatusCode::OK);
        let headers = resp.headers();
        assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "image/webp");
        assert_eq!(
            headers.get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=86400"
        );
        assert!(headers
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with('"'));
    }

    #[test]
    fn image_response_returns_304_when_etag_matches() {
        let data = Bytes::from(vec![1, 2, 3]);
        let etag = etag_for(&data);
        let mut req_headers = HeaderMap::new();
        req_headers.insert(header::IF_NONE_MATCH, HeaderValue::from_str(&etag).unwrap());
        let resp = image_response(
            data,
            HeaderValue::from_static("image/webp"),
            "public, max-age=86400",
            &req_headers,
        );
        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        let headers = resp.headers();
        assert_eq!(headers.get(header::ETAG).unwrap(), etag.as_str());
        assert_eq!(headers.get(header::CONTENT_TYPE).unwrap(), "image/webp");
        assert_eq!(
            headers.get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=86400"
        );
    }

    #[test]
    fn etag_matches_single() {
        assert!(etag_matches("\"abc\"", "\"abc\""));
        assert!(!etag_matches("\"abc\"", "\"def\""));
    }

    #[test]
    fn etag_matches_list() {
        assert!(etag_matches("\"abc\", \"def\"", "\"def\""));
        assert!(!etag_matches("\"abc\", \"def\"", "\"ghi\""));
    }

    #[test]
    fn etag_matches_weak_prefix() {
        assert!(etag_matches("W/\"abc\"", "\"abc\""));
    }

    #[test]
    fn etag_matches_wildcard() {
        assert!(etag_matches("*", "\"anything\""));
    }

    #[test]
    fn image_response_raw_file_is_immutable() {
        let resp = image_response(
            Bytes::from(vec![1, 2, 3]),
            HeaderValue::from_static("image/jpeg"),
            "public, max-age=31536000, immutable",
            &HeaderMap::new(),
        );
        assert_eq!(resp.status(), StatusCode::OK);
        let cache_control = resp
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cache_control.contains("immutable"));
    }

    #[test]
    fn etag_for_same_data_is_stable() {
        let a = etag_for(b"hello");
        let b = etag_for(b"hello");
        assert_eq!(a, b);
        assert_ne!(a, etag_for(b"world"));
    }
}
