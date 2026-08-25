// 与 posts 模块一致：Dioxus `#[server]` 宏触发 deprecated/unit/too_many_arguments
// 提示，按项目惯例放行（限流/运行器等配置项天然参数多）。
#![allow(clippy::unused_unit, deprecated, clippy::too_many_arguments)]

use dioxus::prelude::*;

#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;
use crate::models::settings::ImageLimitSettings;

// ============================================================================
// 图片尺寸限制配置（需重启生效）
// ============================================================================

/// 启动时用 `MAX_IMAGE_DIMENSION` / `MAX_IMAGE_PIXELS` /
/// `IMAGE_DIMENSIONS_CACHE_TTL_SECS` 环境变量播种图片尺寸限制配置。
#[cfg(feature = "server")]
pub(crate) async fn seed_image_limit_settings_from_env(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    use crate::models::settings as m;

    let mut seeds: Vec<(&'static str, String)> = Vec::new();

    if let Ok(v) = std::env::var("MAX_IMAGE_DIMENSION") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "image_max_dimension",
                m::ImageLimitSettings::clamp_max_dimension(n).to_string(),
            )),
            Err(_) => tracing::warn!("MAX_IMAGE_DIMENSION={v:?} 非法（期望正整数），跳过"),
        }
    }
    if let Ok(v) = std::env::var("MAX_IMAGE_PIXELS") {
        match v.trim().parse::<u64>() {
            Ok(n) => seeds.push((
                "image_max_pixels",
                m::ImageLimitSettings::clamp_max_pixels(n).to_string(),
            )),
            Err(_) => tracing::warn!("MAX_IMAGE_PIXELS={v:?} 非法（期望正整数），跳过"),
        }
    }
    if let Ok(v) = std::env::var("IMAGE_DIMENSIONS_CACHE_TTL_SECS") {
        match v.trim().parse::<u64>() {
            Ok(n) => seeds.push((
                "image_dimensions_cache_ttl_secs",
                m::ImageLimitSettings::clamp_dimensions_cache_ttl_secs(n).to_string(),
            )),
            Err(_) => {
                tracing::warn!("IMAGE_DIMENSIONS_CACHE_TTL_SECS={v:?} 非法（期望正整数），跳过")
            }
        }
    }

    super::insert_env_seeds(client, seeds).await
}

/// 从 settings 表读取图片尺寸限制配置（缺键回退默认值）。
#[cfg(feature = "server")]
pub(crate) async fn load_image_limit_settings(
    client: &tokio_postgres::Client,
) -> Result<ImageLimitSettings, AppError> {
    use crate::models::settings as m;

    let values = super::load_setting_values(
        client,
        &[
            "image_max_dimension",
            "image_max_pixels",
            "image_dimensions_cache_ttl_secs",
        ],
    )
    .await?;
    let max_dimension = values
        .get("image_max_dimension")
        .and_then(|v| v.parse().ok())
        .map(m::ImageLimitSettings::clamp_max_dimension)
        .unwrap_or(m::DEFAULT_IMAGE_MAX_DIMENSION);
    let max_pixels = values
        .get("image_max_pixels")
        .and_then(|v| v.parse().ok())
        .map(m::ImageLimitSettings::clamp_max_pixels)
        .unwrap_or(m::DEFAULT_IMAGE_MAX_PIXELS);
    let dimensions_cache_ttl_secs = values
        .get("image_dimensions_cache_ttl_secs")
        .and_then(|v| v.parse().ok())
        .map(m::ImageLimitSettings::clamp_dimensions_cache_ttl_secs)
        .unwrap_or(m::DEFAULT_IMAGE_DIMENSIONS_CACHE_TTL_SECS);

    Ok(ImageLimitSettings {
        max_dimension,
        max_pixels,
        dimensions_cache_ttl_secs,
    })
}

/// 读取图片尺寸限制配置（面板用）。
#[server(GetImageLimitSettings, "/api")]
pub async fn get_image_limit_settings() -> Result<ImageLimitSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let s = load_image_limit_settings(&client)
            .await
            .map_err(ServerFnError::from)?;
        Ok(s)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(ImageLimitSettings::default())
    }
}

/// 更新图片尺寸限制配置。
///
/// 字段会被 clamp 后写入 DB。配置烘焙进 LazyLock，修改后需**重启进程**生效。
#[server(UpdateImageLimitSettings, "/api")]
pub async fn update_image_limit_settings(
    max_dimension: u32,
    max_pixels: u64,
    dimensions_cache_ttl_secs: u64,
) -> Result<ImageLimitSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let max_dimension = ImageLimitSettings::clamp_max_dimension(max_dimension);
    let max_pixels = ImageLimitSettings::clamp_max_pixels(max_pixels);
    let dimensions_cache_ttl_secs =
        ImageLimitSettings::clamp_dimensions_cache_ttl_secs(dimensions_cache_ttl_secs);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        for (key, value) in [
            ("image_max_dimension", max_dimension.to_string()),
            ("image_max_pixels", max_pixels.to_string()),
            (
                "image_dimensions_cache_ttl_secs",
                dimensions_cache_ttl_secs.to_string(),
            ),
        ] {
            client
                .execute(
                    "INSERT INTO settings (key, value, updated_at) VALUES ($1, $2, NOW())
                     ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                    &[&key, &value],
                )
                .await
                .map_err(AppError::query)?;
        }

        tracing::info!(
            "Image limit settings updated (需重启生效): max_dimension={}, max_pixels={}, ttl={}s",
            max_dimension,
            max_pixels,
            dimensions_cache_ttl_secs
        );
    }

    Ok(ImageLimitSettings {
        max_dimension,
        max_pixels,
        dimensions_cache_ttl_secs,
    })
}
