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
/// 启动时从 settings 表（经 [`crate::config::image_limit`]）读取，默认 8192。
/// 下限 512（防误调到危险小值）。值烘焙进 LazyLock，修改面板值后需重启生效。
pub static MAX_IMAGE_DIMENSION: LazyLock<u32> = LazyLock::new(|| {
    let val = crate::config::image_limit().max_dimension;
    tracing::info!("Image dimension limit loaded from DB: {}", val);
    val
});
#[cfg(feature = "server")]
const DEFAULT_JPEG_QUALITY: u8 = 85;
#[cfg(feature = "server")]
/// 允许处理的最大图片像素数（默认约 7k x 7k）。
///
/// 启动时从 settings 表（经 [`crate::config::image_limit`]）读取，默认 50_000_000。
/// 下限 1_000_000（防误调）。⚠️ 此值同时决定单图解码内存缓冲
/// （max_alloc = pixels × 4 + 1MB），默认 50M 像素对应约 200MB/图。
/// 值烘焙进 LazyLock，修改面板值后需重启生效。
pub static MAX_IMAGE_PIXELS: LazyLock<u32> = LazyLock::new(|| {
    // config 中以 u64 存储；此处回退到 u32（原 env 版本亦是 u32）。
    let val = crate::config::image_limit().max_pixels.min(u32::MAX as u64) as u32;
    tracing::info!("Image pixel limit loaded from DB: {}", val);
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
/// 图片处理（解码/缩放/编码）的全局并发上限。
///
/// 取 CPU 核数 clamp 到 [2, 8]：单图处理大致单线程吃满一个核，下限保证小
/// 机型吞吐，上限防止突发缓存 miss 同时分配过多大缓冲（单图最坏约
/// MAX_IMAGE_PIXELS×4 字节瞬时内存）。只约束并发、不拒绝请求：超出许可
/// 的 miss 排队等待；配合按 miss 计费的限流器，排队长度有界。
static IMAGE_PROCESSING_PERMITS: LazyLock<tokio::sync::Semaphore> = LazyLock::new(|| {
    let cores = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(2);
    tokio::sync::Semaphore::new(cores.clamp(2, 8))
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

/// 手动解析 WebP RIFF header 拿尺寸（前 30 字节即够，不调用 zenwebp 全解码器）。
///
/// 不用 `zenwebp::WebPDecoder::build` 的原因：VP8X（扩展格式）的全解码器在
/// 构造时会扫描所有 RIFF chunk（EXIF/ICCP/ALPH…），文件大于传入缓冲区时失败。
/// [`get_image_dimensions`] 只读 64 KiB header 前缀，大 VP8X WebP（> 64 KiB）
/// 因此丢失尺寸 → rebuild 索引时该文件被跳过 → DB 行被误删（issue #30）。
///
/// 三种子格式的尺寸都在 RIFF 签名之后的前 30 字节内：
/// - `VP8 `（lossy）：byte 26-27 width、28-29 height，`u16_le & 0x3FFF`
/// - `VP8L`（lossless）：byte 21-24 为 `u32_le` header，
///   `width=(1+h)&0x3FFF`、`height=(1+(h>>14))&0x3FFF`
/// - `VP8X`（extended）：byte 24-26 / 27-29 各为 24-bit LE，`+1` 得画布尺寸
#[cfg(feature = "server")]
fn read_webp_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    // RIFF(4) + size(4) + WEBP(4) + chunk(4) = 16 字节签名；VP8X 尺寸到 byte 29。
    if data.len() < 30 || &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return None;
    }
    let dims = match &data[12..16] {
        b"VP8 " => {
            // lossy：byte 26-27 / 28-29 为 u16_le，低 14 位是尺寸（高 2 位是缩放系数）。
            let w = u16::from_le_bytes([data[26], data[27]]) & 0x3FFF;
            let h = u16::from_le_bytes([data[28], data[29]]) & 0x3FFF;
            (w as u32, h as u32)
        }
        b"VP8L" => {
            // lossless：byte 21-24 为 u32_le bitstream header。
            let h = u32::from_le_bytes([data[21], data[22], data[23], data[24]]);
            ((1 + h) & 0x3FFF, (1 + (h >> 14)) & 0x3FFF)
        }
        b"VP8X" => {
            // extended：byte 24-26 / 27-29 各为 24-bit LE，+1 得画布尺寸。
            let w = u32::from_le_bytes([data[24], data[25], data[26], 0]) + 1;
            let h = u32::from_le_bytes([data[27], data[28], data[29], 0]) + 1;
            (w, h)
        }
        _ => return None,
    };
    if dims.0 == 0 || dims.1 == 0 {
        return None;
    }
    Some(dims)
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

/// 检测图片是否为动画（多帧）格式。
///
/// 动图的每一帧都承载信息（运动、表情、进度），而 `image::DynamicImage` 与
/// `zenwebp` 的 `read_image`（`api.rs` 中非动画路径）只能解码单帧——经过
/// `process_image` 解码→缩放→重编码后会永久丢失动画，只剩第一帧。
///
/// 因此在处理流水线入口拦截：动图原样返回字节（动画保留），由两层缓存
/// 按参数化 key 缓存。检测只读容器 header，不做像素解码、不分配：
/// - **WebP**：扫描 RIFF chunk，出现 `ANMF`（动画帧）即动图。
/// - **GIF**：扫描 `NETSCAPE2.0` 应用扩展（动图 GIF 的标准循环标记）。
///
/// JPEG/PNG 不可能是动图，直接返回 false。
#[cfg(feature = "server")]
fn is_animated_image(data: &[u8], format: image::ImageFormat) -> bool {
    match format {
        image::ImageFormat::WebP => {
            // RIFF(4)+size(4)+WEBP(4)=12 字节签名；逐 chunk 扫描 FourCC+size。
            // Animated WebP 必有 VP8X + ANIM + 至少一个 ANMF。
            data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" && {
                let mut pos = 12;
                let mut found = false;
                while pos + 8 <= data.len() {
                    let fourcc = &data[pos..pos + 4];
                    let chunk_size = u32::from_le_bytes([
                        data[pos + 4],
                        data[pos + 5],
                        data[pos + 6],
                        data[pos + 7],
                    ]) as usize;
                    if fourcc == b"ANMF" {
                        found = true;
                        break;
                    }
                    // chunk 体 2 字节对齐（RIFF 规范）。
                    pos += 8 + ((chunk_size + 1) & !1);
                }
                found
            }
        }
        image::ImageFormat::Gif => {
            // 动画 GIF 的标志是 NETSCAPE2.0 应用扩展块（循环控制）。
            // 块结构：0x21(扩展) 0xFF(应用) 0x0B(块大小) "NETSCAPE2.0" ...
            const NETSCAPE: &[u8] = b"\x21\xff\x0bNETSCAPE2.0";
            data.windows(NETSCAPE.len()).any(|w| w == NETSCAPE)
        }
        _ => false,
    }
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

    // 动图（animated WebP / GIF）原样返回，绕过解码→缩放→重编码。
    // `image::DynamicImage` 与 zenwebp 的单帧 `read_image` 无法承载多帧；
    // 一旦走 process_image，动画会被永久压缩成第一帧（issue #29）。
    // 原始字节进入两层缓存（key 含 thumb/w 参数），后续命中即静态返回。
    // 代价：动图缩略图返回全尺寸文件（上传上限 5 MiB，首 miss 后缓存），
    // 相对丢失动画是不可接受的回归。
    if is_animated_image(&data, original_format) {
        return Ok((data, content_type(original_format)));
    }
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
/// 依次执行：路径安全校验 → 参数校验 → 分支：
/// - 无参数原图直返：限流（每次真实读盘，防带宽滥用）→ 读盘返回；
/// - 处理路径：内存缓存 → 磁盘缓存（命中即返回，本质是静态文件服务，
///   不计费）→ 限流（只有穿透到「读盘 + 解码处理」的 miss 才扣令牌）→
///   处理并发排队 → 读取并解码 → 处理 → 写入两级缓存 → 返回。
///
/// 按 miss 计费的原因：素材页/长文首刷单页可产生上百个缩略图请求，
/// 若缓存命中也扣令牌，burst 配额会被正常浏览耗尽，批量 429 误杀。
pub async fn serve_image(
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Path(path): Path<String>,
    Query(params): Query<ImageParams>,
    headers: HeaderMap,
) -> Response {
    let peer = connect_info.map(|Extension(ConnectInfo(addr))| addr);
    let ip = crate::api::rate_limit::get_client_ip_with_peer(&headers, peer).await;

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
        // 原图直返不经过任何缓存层（每次真实读盘），保留限流防带宽滥用。
        if let Err(resp) = crate::api::rate_limit::check_image_limit(&ip) {
            return *resp;
        }
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

    // 两层缓存均未命中：此处起的请求才真实消耗「读盘 + 解码处理」工作量，
    // 计入限流。
    if let Err(resp) = crate::api::rate_limit::check_image_limit(&ip) {
        return *resp;
    }

    // 处理并发上限：限流控速率不控并发，冷缓存突发可在令牌允许内向阻塞池
    // 瞬间提交几十个重处理任务（单图最坏约 200MB 瞬时内存）。超出许可的
    // 请求在此排队，把尖峰摊平成有序处理。
    let _permit = IMAGE_PROCESSING_PERMITS
        .acquire()
        .await
        .expect("图片处理信号量从不 close，acquire 不会失败");

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
    let ttl =
        std::time::Duration::from_secs(crate::config::image_limit().dimensions_cache_ttl_secs);
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

    // —— read_webp_dimensions：手动 RIFF 解析回归测试（issue #30）——

    /// 构造 VP8 (lossy) 最小 RIFF：RIFF+size+WEBP+"VP8 "+chunk_size+
    /// frame_tag(3)+start_code(3: 9d 01 2a)+width(u16le)+height(u16le)。
    fn synth_vp8_riff(w: u16, h: u16) -> Vec<u8> {
        let mut buf = b"RIFF\x00\x00\x00\x00WEBPVP8 \x00\x00\x00\x00".to_vec();
        // frame tag (3 bytes) + start code (3 bytes)
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x9d, 0x01, 0x2a]);
        buf.extend_from_slice(&w.to_le_bytes());
        buf.extend_from_slice(&h.to_le_bytes());
        buf
    }

    /// 构造 VP8L (lossless) 最小 RIFF：RIFF+size+WEBP+"VP8L"+chunk_size+
    /// signature(0x2f)+u32le header。
    fn synth_vp8l_riff(w: u32, h: u32) -> Vec<u8> {
        let mut buf = b"RIFF\x00\x00\x00\x00WEBPVP8L\x00\x00\x00\x00".to_vec();
        buf.push(0x2f); // signature
        let header: u32 = (w - 1) | ((h - 1) << 14);
        buf.extend_from_slice(&header.to_le_bytes());
        // pad to ≥ 30 bytes (VP8L data ends at byte 25, but read_webp_dimensions
        // requires ≥ 30 for uniformity with VP8/VP8X).
        while buf.len() < 30 {
            buf.push(0);
        }
        buf
    }

    /// 构造 VP8X (extended) 最小 RIFF：RIFF+size+WEBP+"VP8X"+chunk_size(10)+
    /// flags(1)+reserved(3)+canvas_w_minus1(3 LE)+canvas_h_minus1(3 LE)。
    fn synth_vp8x_riff(w: u32, h: u32) -> Vec<u8> {
        let mut buf = b"RIFF\x00\x00\x00\x00WEBPVP8X\x0a\x00\x00\x00".to_vec();
        buf.push(0x00); // flags
        buf.extend_from_slice(&[0, 0, 0]); // reserved
        let wm1 = w - 1;
        buf.extend_from_slice(&[wm1 as u8, (wm1 >> 8) as u8, (wm1 >> 16) as u8]);
        let hm1 = h - 1;
        buf.extend_from_slice(&[hm1 as u8, (hm1 >> 8) as u8, (hm1 >> 16) as u8]);
        buf
    }

    #[test]
    fn read_webp_vp8_lossy_dimensions() {
        let data = synth_vp8_riff(640, 480);
        assert_eq!(read_webp_dimensions(&data), Some((640, 480)));
    }

    #[test]
    fn read_webp_vp8l_lossless_dimensions() {
        let data = synth_vp8l_riff(100, 50);
        assert_eq!(read_webp_dimensions(&data), Some((100, 50)));
    }

    #[test]
    fn read_webp_vp8x_extended_dimensions() {
        let data = synth_vp8x_riff(1920, 1080);
        assert_eq!(read_webp_dimensions(&data), Some((1920, 1080)));
    }

    #[test]
    fn read_webp_dimensions_rejects_bad_signature() {
        let mut data = synth_vp8x_riff(100, 100);
        data[0] = b'X'; // corrupt RIFF
        assert_eq!(read_webp_dimensions(&data), None);
    }

    #[test]
    fn read_webp_dimensions_rejects_short_data() {
        assert_eq!(read_webp_dimensions(b"RIFF\x00\x00\x00\x00WEBP"), None);
    }

    #[test]
    fn read_webp_dimensions_rejects_zero_size() {
        // VP8 with zero dimensions → None
        let data = synth_vp8_riff(0, 100);
        assert_eq!(read_webp_dimensions(&data), None);
    }

    /// 核心回归测试：VP8X WebP > 64 KiB 截断后仍能读尺寸（issue #30 根因）。
    #[test]
    fn read_webp_vp8x_large_truncated() {
        // 构造一张大噪声 RGBA 图 → encoder 产出 > 64 KiB 的 VP8X WebP。
        let (w, h) = (400, 400);
        let mut rgba = image::RgbaImage::new(w, h);
        let mut s: u64 = 42;
        for px in rgba.iter_mut() {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
            *px = (s >> 33) as u8;
        }
        let webp = crate::webp::encode(&image::DynamicImage::ImageRgba8(rgba), 100.0, 0).unwrap();
        assert!(
            webp.len() > 65_536,
            "test image should produce > 64 KiB webp, got {}",
            webp.len()
        );
        // 截断到 64 KiB（模拟 get_image_dimensions 只读 header 前缀）
        let truncated = &webp[..65_536];
        assert_eq!(
            read_webp_dimensions(truncated),
            Some((w, h)),
            "VP8X webp > 64 KiB must parse dimensions from 64 KiB header"
        );
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

    // —— serve_image 限流计费路径：缓存命中免费 / miss 计费且 429 带 Retry-After ——

    /// 每个测试用独立的 TEST-NET-3 客户端地址：governor 按 IP 分桶，
    /// 桶互不影响，无需 serial。
    fn unique_peer(third: u8) -> std::net::SocketAddr {
        let ip = std::net::Ipv4Addr::new(203, 0, 113, third);
        std::net::SocketAddr::new(std::net::IpAddr::V4(ip), 8080)
    }

    #[tokio::test]
    async fn serve_image_cache_hit_does_not_consume_rate_limit_tokens() {
        // 直接往内存缓存塞条目，模拟温缓存。
        let path = "test/rl_hit.webp";
        let params = ImageParams {
            thumb: Some("300x300".to_string()),
            ..Default::default()
        };
        let key = params.cache_key(path);
        IMAGE_CACHE
            .insert(
                key.clone(),
                CachedImage {
                    data: Bytes::from_static(b"cached"),
                    content_type: HeaderValue::from_static("image/webp"),
                },
            )
            .await;

        // 连续 60 次（超过默认 burst 50）全部命中缓存 → 无一 429。
        let peer = unique_peer(1);
        for _ in 0..60 {
            let resp = serve_image(
                Some(Extension(ConnectInfo(peer))),
                Path(path.to_string()),
                Query(params.clone()),
                HeaderMap::new(),
            )
            .await;
            assert_eq!(resp.status(), StatusCode::OK, "缓存命中不应消耗限流令牌");
        }
        IMAGE_CACHE.invalidate(&key).await;
    }

    #[tokio::test]
    async fn serve_image_cache_miss_is_rate_limited_with_retry_after() {
        // 不存在的文件 + 每次唯一的 w（保证 miss 且不产生缓存写入）：
        // burst 内的 miss 正常走完全流程（404），令牌耗尽后 429 且带 Retry-After。
        let peer = unique_peer(2);
        let mut not_found = 0;
        let mut too_many = 0;
        for w in 1..=60_u32 {
            let resp = serve_image(
                Some(Extension(ConnectInfo(peer))),
                Path("test/rl_miss_nonexistent.webp".to_string()),
                Query(ImageParams {
                    w: Some(w),
                    ..Default::default()
                }),
                HeaderMap::new(),
            )
            .await;
            match resp.status() {
                StatusCode::NOT_FOUND => not_found += 1,
                StatusCode::TOO_MANY_REQUESTS => {
                    too_many += 1;
                    let retry_after = resp
                        .headers()
                        .get(header::RETRY_AFTER)
                        .expect("429 必须带 Retry-After")
                        .to_str()
                        .expect("Retry-After 仅含 ASCII 数字");
                    assert!(retry_after.parse::<u64>().expect("Retry-After 为秒数") >= 1);
                }
                other => panic!("unexpected status {other}"),
            }
        }
        assert!(not_found > 0, "burst 内的 miss 应正常处理（404）");
        assert!(too_many > 0, "超出 burst 的 miss 应被 429 限流");
    }

    // —— is_animated_image / 动图保留回归测试（issue #29）——
    //
    // 动图（animated WebP/GIF）经 process_image 解码→缩放→重编码后会丢失全部帧
    // 只剩第一帧。is_animated_image 在处理流水线入口拦截，让动图原样返回。
    // 以下测试覆盖：真动图检出、静态图不误报、process_image_blocking 字节保留。

    /// 用 zenwebp 的 AnimationEncoder 构造一个真实的多帧 animated WebP（8x8，2 帧）。
    /// 这是用户实际会上传的格式，比手动拼 RIFF 更贴近真实场景。
    fn make_animated_webp() -> Vec<u8> {
        use zenwebp::mux::{AnimationConfig, AnimationEncoder};
        use zenwebp::{EncoderConfig, PixelLayout};

        let mut enc =
            AnimationEncoder::new(8, 8, AnimationConfig::default()).expect("8x8 在合法画布范围内");
        let cfg = EncoderConfig::new_lossy();
        // 两帧不同内容，确保 finalize 不会降级成单帧静态图。
        let frame_a = vec![255u8; 8 * 8 * 3]; // 白
        let frame_b = vec![0u8; 8 * 8 * 3]; // 黑
        enc.add_frame(&frame_a, PixelLayout::Rgb8, 0, &cfg)
            .expect("首帧编码");
        enc.add_frame(&frame_b, PixelLayout::Rgb8, 100, &cfg)
            .expect("次帧编码");
        enc.finalize(100).expect("动画装配")
    }

    #[test]
    fn is_animated_image_detects_real_animated_webp() {
        let animated = make_animated_webp();
        assert!(
            is_animated_image(&animated, image::ImageFormat::WebP),
            "真实多帧 animated WebP 必须被检出"
        );
        // 交叉验证：zenwebp 自己的 header-only probe 也认为是动画
        let probe = zenwebp::detect::probe(&animated).expect("合法 WebP");
        assert!(probe.has_animation, "probe 应报告 has_animation");
    }

    #[test]
    fn is_animated_image_false_for_static_webp() {
        // 普通静态 WebP（三种子格式）都不应被误判为动图。
        let img = image::DynamicImage::new_rgb8(32, 32);
        let static_webp = crate::webp::encode(&img, 80.0, 2).unwrap();
        assert!(
            !is_animated_image(&static_webp, image::ImageFormat::WebP),
            "静态 WebP 不应被误报为动图"
        );
        // VP8X 扩展格式（无动画标志、无 ANMF chunk）也不应误报
        let extended = synth_vp8x_riff(100, 100);
        assert!(
            !is_animated_image(&extended, image::ImageFormat::WebP),
            "VP8X 非动画 WebP 不应被误报"
        );
    }

    #[test]
    fn process_image_blocking_preserves_animated_webp_bytes() {
        // 核心回归：带处理参数（thumb）的动图请求必须原样返回输入字节，
        // 不能走解码→重编码（会丢帧）。issue #29 的精确症状。
        let animated = make_animated_webp();
        let params = ImageParams {
            thumb: Some("300x300".to_string()),
            ..Default::default()
        };
        let (out_bytes, out_ct) =
            process_image_blocking(animated.clone(), params, "2026/08/13/anim.webp".to_string())
                .expect("动图绕过处理不应失败");
        assert_eq!(
            out_bytes, animated,
            "动图字节必须原样返回（绕过解码/缩放/重编码）"
        );
        assert_eq!(out_ct, "image/webp");
    }

    #[test]
    fn process_image_blocking_still_processes_static_webp() {
        // 静态 WebP 仍走完整处理流水线：输出字节应与输入不同（被缩放/重编码）。
        let img = image::DynamicImage::new_rgb8(200, 200);
        let static_webp = crate::webp::encode(&img, 80.0, 2).unwrap();
        let params = ImageParams {
            thumb: Some("50x50".to_string()),
            ..Default::default()
        };
        let (out_bytes, out_ct) =
            process_image_blocking(static_webp.clone(), params, "static.webp".to_string())
                .expect("静态图处理不应失败");
        assert_ne!(
            out_bytes, static_webp,
            "静态 WebP 应被实际处理（字节变化），不能被错误绕过"
        );
        assert_eq!(out_ct, "image/webp");
    }

    #[test]
    fn is_animated_image_detects_gif() {
        // 动画 GIF 的标志是 NETSCAPE2.0 应用扩展块。
        // 构造包含该标记的最小字节序列（不要求完整 GIF 解码）。
        let anim_gif: Vec<u8> = {
            let mut b = b"GIF89a".to_vec();
            b.extend_from_slice(b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00");
            b.extend_from_slice(&[0x3b]); // trailer
            b
        };
        assert!(
            is_animated_image(&anim_gif, image::ImageFormat::Gif),
            "含 NETSCAPE2.0 标记的 GIF 应被检出为动图"
        );
        // 无标记的 GIF 头不应误报
        let static_gif = b"GIF89a\x01\x00\x01\x00";
        assert!(
            !is_animated_image(static_gif, image::ImageFormat::Gif),
            "无 NETSCAPE 标记的 GIF 不应被误报"
        );
    }

    #[test]
    fn is_animated_image_false_for_jpeg_png() {
        // JPEG/PNG 不可能是动图，直接返回 false（不扫描字节）。
        assert!(!is_animated_image(
            &[0xFF, 0xD8, 0xFF],
            image::ImageFormat::Jpeg
        ));
        assert!(!is_animated_image(
            &[0x89, 0x50, 0x4E, 0x47],
            image::ImageFormat::Png
        ));
    }
}
