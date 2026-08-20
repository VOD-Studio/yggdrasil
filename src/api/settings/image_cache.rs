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
use crate::models::settings::ImageCacheSettings;
// 仅 server 构建的函数体引用（WASM 端 server fn 体被宏剥离）。
#[cfg(feature = "server")]
use crate::cache::invalidate_image_cache_settings;

// ============================================================================
// 图片磁盘缓存配置（即时生效）
// ============================================================================

/// 启动时用 `IMAGE_DISK_CACHE_MAX_MB` / `_MAX_AGE_HOURS` 环境变量播种。
///
/// 语义与 [`seed_backup_settings_from_env`][crate::api::settings::seed_backup_settings_from_env] 一致。
#[cfg(feature = "server")]
pub(crate) async fn seed_image_cache_settings_from_env(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    let mut seeds: Vec<(&'static str, String)> = Vec::new();

    if let Ok(v) = std::env::var("IMAGE_DISK_CACHE_MAX_MB") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "image_disk_cache_max_mb",
                crate::models::settings::ImageCacheSettings::clamp_max_mb(n).to_string(),
            )),
            Err(_) => tracing::warn!("IMAGE_DISK_CACHE_MAX_MB={v:?} 非法（期望正整数），跳过"),
        }
    }
    if let Ok(v) = std::env::var("IMAGE_DISK_CACHE_MAX_AGE_HOURS") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "image_disk_cache_max_age_hours",
                crate::models::settings::ImageCacheSettings::clamp_max_age_hours(n).to_string(),
            )),
            Err(_) => {
                tracing::warn!("IMAGE_DISK_CACHE_MAX_AGE_HOURS={v:?} 非法（期望正整数），跳过")
            }
        }
    }

    for (key, value) in seeds {
        client
            .execute(
                "INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO NOTHING",
                &[&key, &value],
            )
            .await
            .map_err(AppError::query)?;
        tracing::info!("图片缓存配置已从环境变量播种: {key}={value}（仅键缺失时生效）");
    }
    Ok(())
}

/// 从 settings 表读取图片磁盘缓存配置（缺键回退默认值）。
#[cfg(feature = "server")]
pub(crate) async fn load_image_cache_settings(
    client: &tokio_postgres::Client,
) -> Result<crate::models::settings::ImageCacheSettings, AppError> {
    async fn read_key(
        client: &tokio_postgres::Client,
        key: &str,
    ) -> Result<Option<String>, AppError> {
        let row = client
            .query_opt("SELECT value FROM settings WHERE key = $1", &[&key])
            .await
            .map_err(AppError::query)?;
        Ok(row.map(|r| r.get::<_, String>("value")))
    }

    let disk_cache_max_mb = read_key(client, "image_disk_cache_max_mb")
        .await?
        .and_then(|v| v.parse().ok())
        .map(crate::models::settings::ImageCacheSettings::clamp_max_mb)
        .unwrap_or(crate::models::settings::DEFAULT_IMAGE_DISK_CACHE_MAX_MB);
    let disk_cache_max_age_hours = read_key(client, "image_disk_cache_max_age_hours")
        .await?
        .and_then(|v| v.parse().ok())
        .map(crate::models::settings::ImageCacheSettings::clamp_max_age_hours)
        .unwrap_or(crate::models::settings::DEFAULT_IMAGE_DISK_CACHE_MAX_AGE_HOURS);

    Ok(crate::models::settings::ImageCacheSettings {
        disk_cache_max_mb,
        disk_cache_max_age_hours,
    })
}

/// 运行时读取图片磁盘缓存配置：先查 moka 缓存，未命中查 DB 并回填。
#[cfg(feature = "server")]
pub(crate) async fn runtime_image_cache_settings() -> crate::models::settings::ImageCacheSettings {
    if let Some(s) = crate::cache::get_image_cache_settings().await {
        return s;
    }
    let fallback = crate::models::settings::ImageCacheSettings::default();
    // 无 DATABASE_URL 时（如单元测试环境）直接回退默认值，
    // 避免触发 DB_POOL LazyLock 的防御性 panic。
    if std::env::var("DATABASE_URL").is_err() {
        return fallback;
    }
    match get_conn().await {
        Ok(client) => match load_image_cache_settings(&client).await {
            Ok(s) => {
                crate::cache::set_image_cache_settings(s.clone()).await;
                s
            }
            Err(e) => {
                tracing::warn!("读取图片缓存配置失败，回退默认值：{e:?}");
                fallback
            }
        },
        Err(e) => {
            tracing::warn!("获取连接读取图片缓存配置失败，回退默认值：{e:?}");
            fallback
        }
    }
}

/// 读取图片磁盘缓存配置（面板用）。
#[server(GetImageCacheSettings, "/api")]
pub async fn get_image_cache_settings() -> Result<ImageCacheSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let s = load_image_cache_settings(&client)
            .await
            .map_err(ServerFnError::from)?;
        crate::cache::set_image_cache_settings(s.clone()).await;
        Ok(s)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(ImageCacheSettings::default())
    }
}

/// 更新图片磁盘缓存配置。
#[server(UpdateImageCacheSettings, "/api")]
pub async fn update_image_cache_settings(
    disk_cache_max_mb: u32,
    disk_cache_max_age_hours: u32,
) -> Result<ImageCacheSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let disk_cache_max_mb = ImageCacheSettings::clamp_max_mb(disk_cache_max_mb);
    let disk_cache_max_age_hours =
        ImageCacheSettings::clamp_max_age_hours(disk_cache_max_age_hours);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        for (key, value) in [
            ("image_disk_cache_max_mb", disk_cache_max_mb.to_string()),
            (
                "image_disk_cache_max_age_hours",
                disk_cache_max_age_hours.to_string(),
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

        invalidate_image_cache_settings();
        tracing::info!(
            "Image cache settings updated: max_mb={}, max_age_hours={}",
            disk_cache_max_mb,
            disk_cache_max_age_hours
        );
    }

    Ok(ImageCacheSettings {
        disk_cache_max_mb,
        disk_cache_max_age_hours,
    })
}
