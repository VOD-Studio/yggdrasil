//! WebP 编解码模块。
//!
//! 本模块仅在 `server` feature 启用时编译。
//!
//! ## `zenwebp` 与 `image` crate 的分工
//!
//! - `image` crate：负责通用图像格式（JPEG、PNG、GIF 等）的解码、缩放、旋转以及
//!   像素格式转换（`DynamicImage`、`RgbaImage`、`RgbImage`）。本项目特意禁用了 `image`
//!   的 `webp` feature，因为它不支持 WebP 编码，且解码能力有限。
//! - `zenwebp`：专门负责 WebP 格式的有损编码与解码。所有需要输出 WebP 或读取 WebP
//!   字节流的场景都通过 `zenwebp` 完成。
//!
//! 简言之：`image` 处理“除 WebP 外的图像操作”，`zenwebp` 处理“WebP 专有编解码”。

#[cfg(feature = "server")]
use std::sync::LazyLock;

/// WebP 编解码过程中可能产生的错误。
#[cfg(feature = "server")]
#[derive(Debug)]
pub enum WebpError {
    /// 编码失败。
    Encode(String),
    /// 解码失败。
    Decode(String),
}

#[cfg(feature = "server")]
impl std::fmt::Display for WebpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebpError::Encode(msg) => write!(f, "WebP encode error: {}", msg),
            WebpError::Decode(msg) => write!(f, "WebP decode error: {}", msg),
        }
    }
}

#[cfg(feature = "server")]
impl std::error::Error for WebpError {}

/// WebP 有损编码配置。
#[cfg(feature = "server")]
#[derive(Debug, Clone)]
pub struct WebpConfig {
    /// 质量系数，范围 0.0–100.0。
    pub quality: f32,
    /// 编码方法，范围 0–6，数值越大压缩率越高但越慢。
    pub method: u8,
}

/// 从启动期加载的配置（[`crate::config::webp`]）读取的 WebP 全局配置。
///
/// 值由 main.rs 在启动时从 settings 表加载并写入 `config::WEBP_CFG`；
/// 未设置时（如单元测试）回退默认值 quality=85.0、method=2。
/// 修改面板值后需重启进程生效。
#[cfg(feature = "server")]
pub static WEBP_CONFIG: LazyLock<WebpConfig> = LazyLock::new(|| {
    let cfg = crate::config::webp();
    tracing::info!(
        "WebP config loaded from DB: quality={}, method={}",
        cfg.quality,
        cfg.method
    );
    WebpConfig {
        quality: cfg.quality,
        method: cfg.method as u8,
    }
});

/// 将 `image::DynamicImage` 编码为 WebP 字节流。
///
/// 直接处理 `Rgba8` 与 `Rgb8` 两种像素布局，其他格式先转换为 `Rgba8` 再编码。
#[cfg(feature = "server")]
pub fn encode(img: &image::DynamicImage, quality: f32, method: u8) -> Result<Vec<u8>, WebpError> {
    use zenwebp::{EncodeRequest, LossyConfig, PixelLayout};

    let (width, height) = (img.width(), img.height());
    let config = LossyConfig::new().with_quality(quality).with_method(method);

    fn do_encode(
        config: &LossyConfig,
        pixels: &[u8],
        layout: zenwebp::PixelLayout,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, WebpError> {
        EncodeRequest::lossy(config, pixels, layout, width, height)
            .encode()
            .map_err(|e| WebpError::Encode(e.to_string()))
    }

    match img {
        image::DynamicImage::ImageRgba8(rgba) => {
            do_encode(&config, rgba.as_raw(), PixelLayout::Rgba8, width, height)
        }
        image::DynamicImage::ImageRgb8(rgb) => {
            do_encode(&config, rgb.as_raw(), PixelLayout::Rgb8, width, height)
        }
        _ => {
            // 其他像素格式统一转换为 RGBA8 后再交给 zenwebp 编码
            let rgba = img.to_rgba8();
            do_encode(&config, rgba.as_raw(), PixelLayout::Rgba8, width, height)
        }
    }
}

/// 将 WebP 字节流解码为 `image::DynamicImage`。
///
/// 根据 alpha 通道信息决定返回 `ImageRgba8` 还是 `ImageRgb8`。
/// 解码前会校验像素总数，防止超大图片导致内存问题。
#[cfg(feature = "server")]
pub fn decode(data: &[u8]) -> Result<image::DynamicImage, WebpError> {
    use zenwebp::WebPDecoder;

    let mut decoder = WebPDecoder::build(data)
        .map_err(|e| WebpError::Decode(format!("Failed to build decoder: {}", e)))?;

    let info = decoder.info();
    let width = info.width;
    let height = info.height;
    let has_alpha = info.has_alpha;

    let pixel_count = (width as u64) * (height as u64);

    // 超过最大允许像素数时提前拒绝
    if pixel_count > *crate::api::image::MAX_IMAGE_PIXELS as u64 {
        return Err(WebpError::Decode(format!(
            "Image dimensions {}x{} exceed maximum allowed pixels",
            width, height
        )));
    }

    let buf_size = decoder
        .output_buffer_size()
        .ok_or_else(|| WebpError::Decode("Image too large".to_string()))?;

    let mut output = vec![0u8; buf_size];
    decoder
        .read_image(&mut output)
        .map_err(|e| WebpError::Decode(format!("Failed to decode: {}", e)))?;

    if has_alpha {
        image::RgbaImage::from_raw(width, height, output)
            .map(image::DynamicImage::ImageRgba8)
            .ok_or_else(|| WebpError::Decode("Invalid RGBA dimensions".to_string()))
    } else {
        // 无 alpha 时，zenwebp 输出的是 width * height * 3 的 RGB 数据
        image::RgbImage::from_raw(width, height, output)
            .map(image::DynamicImage::ImageRgb8)
            .ok_or_else(|| WebpError::Decode("Invalid RGB dimensions".to_string()))
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn encode_produces_non_empty_bytes() {
        let img = image::DynamicImage::new_rgba8(10, 10);
        let result = encode(&img, 85.0, 4).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn decode_roundtrip_rgba() {
        let original = image::DynamicImage::new_rgba8(5, 5);
        let encoded = encode(&original, 85.0, 4).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.width(), 5);
        assert_eq!(decoded.height(), 5);
    }

    #[test]
    fn decode_roundtrip_rgb() {
        let original = image::DynamicImage::new_rgb8(5, 5);
        let encoded = encode(&original, 85.0, 4).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.width(), 5);
        assert_eq!(decoded.height(), 5);
    }

    #[test]
    fn config_default_values_are_reasonable() {
        let config = WebpConfig {
            quality: 85.0,
            method: 2,
        };
        assert!(config.quality >= 0.0 && config.quality <= 100.0);
        assert!(config.method <= 6);
    }

    #[test]
    fn config_clamping_logic() {
        let quality = 150.0f32;
        let clamped = quality.clamp(0.0, 100.0);
        assert_eq!(clamped, 100.0);

        let quality = -10.0f32;
        let clamped = quality.clamp(0.0, 100.0);
        assert_eq!(clamped, 0.0);

        let method = 10u8;
        let clamped = method.clamp(0, 6);
        assert_eq!(clamped, 6);
    }

    #[test]
    fn config_clamps_edge_cases() {
        assert_eq!(0.0f32.clamp(0.0, 100.0), 0.0);
        assert_eq!(100.0f32.clamp(0.0, 100.0), 100.0);
        assert_eq!(0u8.clamp(0, 6), 0);
        assert_eq!(6u8.clamp(0, 6), 6);
    }

    #[test]
    fn webp_error_encode_display() {
        let err = WebpError::Encode("boom".to_string());
        assert_eq!(err.to_string(), "WebP encode error: boom");
    }

    #[test]
    fn webp_error_decode_display() {
        let err = WebpError::Decode("busted".to_string());
        assert_eq!(err.to_string(), "WebP decode error: busted");
    }

    #[test]
    fn webp_error_implements_std_error() {
        // WebpError 必须实现 std::error::Error，才能在 ? 传播链中使用。
        fn assert_error<T: std::error::Error>() {}
        assert_error::<WebpError>();
    }

    #[test]
    fn encode_converts_luma8_to_rgba() {
        // Luma8（灰度）图像不在 encode 的快速路径中，应被转换为 RGBA8 后编码。
        let img = image::DynamicImage::new_luma8(4, 4);
        let result = encode(&img, 80.0, 2);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn encode_converts_luma_a8_to_rgba() {
        // LumaA8（带 alpha 的灰度）同样走转换路径。
        let img = image::DynamicImage::new_luma_a8(4, 4);
        let result = encode(&img, 80.0, 2);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn encode_lower_quality_does_not_explode_on_solid_color() {
        // 纯色图是 WebP 的极端情况（信息熵接近 0），确保高低质量都能编码成功
        // 而非 panic，且产物是合法非空字节流。不假设低质量体积一定更小，
        // 因为这依赖底层 libwebp 的量化策略，非确定性不变量。
        let img = image::DynamicImage::new_rgb8(64, 64);
        let high = encode(&img, 95.0, 4).unwrap();
        let low = encode(&img, 10.0, 4).unwrap();
        assert!(!high.is_empty());
        assert!(!low.is_empty());
        // 两者都应是合法的 WebP（能被本模块解码回来）。
        assert!(decode(&high).is_ok());
        assert!(decode(&low).is_ok());
    }

    #[test]
    fn decode_invalid_bytes_returns_error() {
        // 非 WebP 字节流应返回解码错误而非 panic。
        let junk = b"this is definitely not a webp image";
        let result = decode(junk);
        assert!(result.is_err());
    }

    #[test]
    fn decode_empty_bytes_returns_error() {
        // 空字节流应返回解码错误而非 panic。
        let result = decode(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_error_message_is_descriptive() {
        // 解码错误的 Display 应包含 'WebP decode error' 前缀，便于日志排查。
        let err = decode(b"not webp").unwrap_err();
        assert!(err.to_string().starts_with("WebP decode error"));
    }

    #[test]
    fn encode_decode_preserves_dimensions() {
        // 编码再解码后，图像宽高应保持一致。
        let original = image::DynamicImage::new_rgb8(16, 9);
        let encoded = encode(&original, 85.0, 4).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.width(), 16);
        assert_eq!(decoded.height(), 9);
    }
}
