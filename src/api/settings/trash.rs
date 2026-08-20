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
use crate::models::settings::TrashSettings;

/// 读取回收站配置。
///
/// settings 表缺失键时回退到默认值，保证向后兼容。
#[server(GetTrashSettings, "/api")]
pub async fn get_trash_settings() -> Result<TrashSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        let enabled: bool = client
            .query_opt(
                "SELECT value FROM settings WHERE key = 'trash_auto_purge_enabled'",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .and_then(|r| r.get::<_, String>("value").parse().ok())
            .unwrap_or(crate::models::settings::DEFAULT_AUTO_PURGE_ENABLED);

        let days: i32 = client
            .query_opt(
                "SELECT value FROM settings WHERE key = 'trash_retention_days'",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .and_then(|r| r.get::<_, String>("value").parse().ok())
            .unwrap_or(crate::models::settings::DEFAULT_RETENTION_DAYS);

        Ok(TrashSettings {
            auto_purge_enabled: enabled,
            retention_days: TrashSettings::clamp_retention(days),
        })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(TrashSettings::default())
    }
}

/// 更新回收站配置。
///
/// retention_days 会被 clamp 到合法范围后写入。
#[server(UpdateTrashSettings, "/api")]
pub async fn update_trash_settings(
    auto_purge_enabled: bool,
    retention_days: i32,
) -> Result<TrashSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let retention_days = TrashSettings::clamp_retention(retention_days);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        // UPSERT 两个键。
        client
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('trash_auto_purge_enabled', $1, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                &[&auto_purge_enabled.to_string()],
            )
            .await
            .map_err(AppError::query)?;

        client
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('trash_retention_days', $1, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                &[&retention_days.to_string()],
            )
            .await
            .map_err(AppError::query)?;

        tracing::info!(
            "Trash settings updated: auto_purge={}, retention_days={}",
            auto_purge_enabled,
            retention_days
        );
    }

    Ok(TrashSettings {
        auto_purge_enabled,
        retention_days,
    })
}
