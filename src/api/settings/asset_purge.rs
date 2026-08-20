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
use crate::models::settings::AssetPurgeSettings;

// ============================================================================
// 孤儿素材自动清理配置（即时生效）
// ============================================================================

/// 启动时用 `ASSET_ORPHAN_PURGE_*` 环境变量播种孤儿素材清理配置。
///
/// 语义与 [`seed_image_cache_settings_from_env`][crate::api::settings::seed_image_cache_settings_from_env] 一致：仅当对应 settings 键
/// **不存在**时插入（首次部署），之后以面板写入的 DB 值为准，重启不被 env 覆盖。
/// 单个变量非法只告警跳过，不影响启动。
#[cfg(feature = "server")]
pub(crate) async fn seed_asset_purge_settings_from_env(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    let mut seeds: Vec<(&'static str, String)> = Vec::new();

    if let Ok(v) = std::env::var("ASSET_ORPHAN_PURGE_ENABLED") {
        match v.trim().parse::<bool>() {
            Ok(b) => seeds.push(("asset_orphan_purge_enabled", b.to_string())),
            Err(_) => {
                tracing::warn!("ASSET_ORPHAN_PURGE_ENABLED={v:?} 非法（期望 true/false），跳过")
            }
        }
    }
    if let Ok(v) = std::env::var("ASSET_ORPHAN_RETENTION_DAYS") {
        match v.trim().parse::<i32>() {
            Ok(n) => seeds.push((
                "asset_orphan_retention_days",
                crate::models::settings::AssetPurgeSettings::clamp_retention(n).to_string(),
            )),
            Err(_) => {
                tracing::warn!("ASSET_ORPHAN_RETENTION_DAYS={v:?} 非法（期望正整数），跳过")
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
        tracing::info!("孤儿素材清理配置已从环境变量播种: {key}={value}（仅键缺失时生效）");
    }
    Ok(())
}

/// 读取孤儿素材清理配置（面板用）。
///
/// settings 表缺失键时回退到默认值，保证向后兼容。
#[server(GetAssetPurgeSettings, "/api")]
pub async fn get_asset_purge_settings() -> Result<AssetPurgeSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        let enabled: bool = client
            .query_opt(
                "SELECT value FROM settings WHERE key = 'asset_orphan_purge_enabled'",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .and_then(|r| r.get::<_, String>("value").parse().ok())
            .unwrap_or(crate::models::settings::DEFAULT_ASSET_ORPHAN_PURGE_ENABLED);

        let days: i32 = client
            .query_opt(
                "SELECT value FROM settings WHERE key = 'asset_orphan_retention_days'",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .and_then(|r| r.get::<_, String>("value").parse().ok())
            .unwrap_or(crate::models::settings::DEFAULT_ASSET_ORPHAN_RETENTION_DAYS);

        Ok(AssetPurgeSettings {
            auto_purge_enabled: enabled,
            retention_days: AssetPurgeSettings::clamp_retention(days),
        })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(AssetPurgeSettings::default())
    }
}

/// 更新孤儿素材清理配置。
///
/// retention_days 会被 clamp 到合法范围后写入。清理任务每天 tick 时读取
/// 最新值，无需重启。
#[server(UpdateAssetPurgeSettings, "/api")]
pub async fn update_asset_purge_settings(
    auto_purge_enabled: bool,
    retention_days: i32,
) -> Result<AssetPurgeSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let retention_days = AssetPurgeSettings::clamp_retention(retention_days);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        for (key, value) in [
            ("asset_orphan_purge_enabled", auto_purge_enabled.to_string()),
            ("asset_orphan_retention_days", retention_days.to_string()),
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
            "Asset purge settings updated: auto_purge={}, retention_days={}",
            auto_purge_enabled,
            retention_days
        );
    }

    Ok(AssetPurgeSettings {
        auto_purge_enabled,
        retention_days,
    })
}
