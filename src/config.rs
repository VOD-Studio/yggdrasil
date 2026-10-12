//! 启动期一次性加载的进程级配置（Tier B：重启生效）。
//!
//! 这些配置原本经环境变量在首次请求时经 LazyLock 读取（限流器等）。
//! 迁移到 settings 表后，启动时由 main.rs 从 DB 加载并写入此处的 OnceLock；
//! 各 LazyLock 静态量改为从 [`rate_limit`] 读取，而非直接读环境变量。
//!
//! 与 Tier A（SecuritySettings / ImageCacheSettings）不同，Tier B 配置被烘焙进
//! LazyLock 静态量，首次请求时构造即固化，修改 DB 值需重启进程才能生效。
//!
//! `set_*` 与 getter 仅在 server feature（测试构建保留）下编译；未初始化时回退默认值。
#![cfg(any(feature = "server", test))]

use crate::models::settings::{
    ImageLimitSettings, RateLimitSettings, RunnerSettings, WebpSettings,
};
use std::sync::OnceLock;

// ============================================================================
// 限流配置
// ============================================================================

/// 限流配置（启动时从 DB 加载一次）。
static RATE_LIMIT_CFG: OnceLock<RateLimitSettings> = OnceLock::new();

/// 在启动时写入限流配置（仅调用一次）。
///
/// 若已设置则忽略后续调用——正常流程中 main.rs 仅调用一次。
#[cfg(feature = "server")]
pub fn set_rate_limit(cfg: RateLimitSettings) {
    if RATE_LIMIT_CFG.set(cfg).is_err() {
        tracing::warn!("rate_limit config already initialized, ignoring subsequent set");
    }
}

/// 读取限流配置。
///
/// OnceLock 已设置则返回其值；未设置（如单元测试 / 启动极早期）回退到
/// [`RateLimitSettings::default`]。
pub fn rate_limit() -> RateLimitSettings {
    RATE_LIMIT_CFG.get().cloned().unwrap_or_default()
}

// ============================================================================
// WebP 编码配置
// ============================================================================

/// WebP 编码配置（启动时从 DB 加载一次）。
static WEBP_CFG: OnceLock<WebpSettings> = OnceLock::new();

/// 在启动时写入 WebP 配置（仅调用一次）。
#[cfg(feature = "server")]
pub fn set_webp(cfg: WebpSettings) {
    if WEBP_CFG.set(cfg).is_err() {
        tracing::warn!("webp config already initialized, ignoring subsequent set");
    }
}

/// 读取 WebP 配置。未设置时回退到 [`WebpSettings::default`]。
pub fn webp() -> WebpSettings {
    WEBP_CFG.get().cloned().unwrap_or_default()
}

// ============================================================================
// 图片尺寸限制配置
// ============================================================================

/// 图片尺寸限制配置（启动时从 DB 加载一次）。
static IMAGE_LIMIT_CFG: OnceLock<ImageLimitSettings> = OnceLock::new();

/// 在启动时写入图片尺寸限制配置（仅调用一次）。
#[cfg(feature = "server")]
pub fn set_image_limit(cfg: ImageLimitSettings) {
    if IMAGE_LIMIT_CFG.set(cfg).is_err() {
        tracing::warn!("image_limit config already initialized, ignoring subsequent set");
    }
}

/// 读取图片尺寸限制配置。未设置时回退到 [`ImageLimitSettings::default`]。
pub fn image_limit() -> ImageLimitSettings {
    IMAGE_LIMIT_CFG.get().cloned().unwrap_or_default()
}

#[cfg(feature = "server")]
/// 图片单边（宽或高）尺寸上限，单位像素。
///
/// 启动时从 settings 表（经 [`crate::config::image_limit`]）读取，默认 8192。
/// 下限 512（防误调到危险小值）。值烘焙进 LazyLock，修改面板值后需重启生效。
pub static MAX_IMAGE_DIMENSION: std::sync::LazyLock<u32> = std::sync::LazyLock::new(|| {
    let val = crate::config::image_limit().max_dimension;
    tracing::info!("Image dimension limit loaded from DB: {}", val);
    val
});
#[cfg(feature = "server")]
/// 允许处理的最大图片像素数（默认约 7k x 7k）。
///
/// 启动时从 settings 表（经 [`crate::config::image_limit`]）读取，默认 50_000_000。
/// 下限 1_000_000（防误调）。⚠️ 此值同时决定单图解码内存缓冲
/// （max_alloc = pixels × 4 + 1MB），默认 50M 像素对应约 200MB/图。
/// 值烘焙进 LazyLock，修改面板值后需重启生效。
pub static MAX_IMAGE_PIXELS: std::sync::LazyLock<u32> = std::sync::LazyLock::new(|| {
    // config 中以 u64 存储；此处回退到 u32（原 env 版本亦是 u32）。
    let val = crate::config::image_limit().max_pixels.min(u32::MAX as u64) as u32;
    tracing::info!("Image pixel limit loaded from DB: {}", val);
    val
});

// ============================================================================
// 代码运行器配置
// ============================================================================

/// 代码运行器配置（启动时从 DB 加载一次）。
static RUNNER_CFG: OnceLock<RunnerSettings> = OnceLock::new();

/// 在启动时写入代码运行器配置（仅调用一次）。
#[cfg(feature = "server")]
pub fn set_runner(cfg: RunnerSettings) {
    if RUNNER_CFG.set(cfg).is_err() {
        tracing::warn!("runner config already initialized, ignoring subsequent set");
    }
}

/// 读取代码运行器配置。未设置时回退到 [`RunnerSettings::default`]。
pub fn runner() -> RunnerSettings {
    RUNNER_CFG.get().cloned().unwrap_or_default()
}
