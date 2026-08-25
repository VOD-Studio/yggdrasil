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
use crate::models::settings::UploadSettings;

// ============================================================================
// 素材上传配置
// ============================================================================

/// 启动时用 `UPLOAD_CONCURRENCY` 环境变量播种素材上传并发数。
///
/// 语义与 [`seed_backup_settings_from_env`][crate::api::settings::seed_backup_settings_from_env] 一致：仅当 settings 键**不存在**时
/// 插入（首次部署），之后以「站点配置」面板写入的 DB 值为准，重启不被 env 覆盖。
/// 非法值只告警跳过，不影响启动。
#[cfg(feature = "server")]
pub(crate) async fn seed_upload_settings_from_env(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    let Ok(v) = std::env::var("UPLOAD_CONCURRENCY") else {
        return Ok(());
    };
    let value = match v.trim().parse::<i32>() {
        Ok(n) => UploadSettings::clamp_concurrency(n),
        Err(_) => {
            tracing::warn!("UPLOAD_CONCURRENCY={v:?} 非法（期望整数），跳过");
            return Ok(());
        }
    };
    super::insert_env_seeds(client, vec![("upload_concurrency", value.to_string())]).await
}

/// 读取素材上传配置（上传弹窗并发数）。
///
/// settings 表缺失键时回退到默认值，保证向后兼容。
#[server(GetUploadSettings, "/api")]
pub async fn get_upload_settings() -> Result<UploadSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        let values = super::load_setting_values(&client, &["upload_concurrency"]).await?;

        let concurrency = values
            .get("upload_concurrency")
            .and_then(|v| v.parse().ok())
            .map(UploadSettings::clamp_concurrency)
            .unwrap_or(crate::models::settings::DEFAULT_UPLOAD_CONCURRENCY);

        Ok(UploadSettings { concurrency })
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(UploadSettings::default())
    }
}

/// 更新素材上传配置。
///
/// concurrency 会被 clamp 到合法范围后写入。
#[server(UpdateUploadSettings, "/api")]
pub async fn update_upload_settings(concurrency: i32) -> Result<UploadSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let concurrency = UploadSettings::clamp_concurrency(concurrency);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        client
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('upload_concurrency', $1, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                &[&concurrency.to_string()],
            )
            .await
            .map_err(AppError::query)?;

        tracing::info!("Upload settings updated: concurrency={}", concurrency);
    }

    Ok(UploadSettings { concurrency })
}
