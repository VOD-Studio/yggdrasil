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
use crate::models::settings::WebpSettings;

// ============================================================================
// WebP 编码配置（需重启生效）
// ============================================================================

/// 启动时用 `WEBP_QUALITY` / `WEBP_METHOD` 环境变量播种 WebP 编码配置。
///
/// 语义与 [`seed_security_settings_from_env`][crate::api::settings::seed_security_settings_from_env] 一致：仅当对应 settings 键
/// **不存在**时插入（首次部署），之后以面板写入的 DB 值为准。单个变量非法只
/// 告警跳过。这些值在进程启动时烘焙进 LazyLock，改 DB 值后需重启生效。
#[cfg(feature = "server")]
pub(crate) async fn seed_webp_settings_from_env(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    use crate::models::settings as m;

    let mut seeds: Vec<(&'static str, String)> = Vec::new();

    if let Ok(v) = std::env::var("WEBP_QUALITY") {
        match v.trim().parse::<f32>() {
            Ok(q) => seeds.push((
                "webp_quality",
                m::WebpSettings::clamp_quality(q).to_string(),
            )),
            Err(_) => tracing::warn!("WEBP_QUALITY={v:?} 非法（期望浮点数），跳过"),
        }
    }
    if let Ok(v) = std::env::var("WEBP_METHOD") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push(("webp_method", m::WebpSettings::clamp_method(n).to_string())),
            Err(_) => tracing::warn!("WEBP_METHOD={v:?} 非法（期望非负整数），跳过"),
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
        tracing::info!("WebP 配置已从环境变量播种: {key}={value}（仅键缺失时生效）");
    }
    Ok(())
}

/// 从 settings 表读取 WebP 配置（缺键回退默认值）。
///
/// 启动时由 main.rs 调用，将结果写入 `config::WEBP_CFG`，供 infra/webp.rs 的 LazyLock
/// 在首次编码时读取。
#[cfg(feature = "server")]
pub(crate) async fn load_webp_settings(
    client: &tokio_postgres::Client,
) -> Result<WebpSettings, AppError> {
    use crate::models::settings as m;

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

    let quality = read_key(client, "webp_quality")
        .await?
        .and_then(|v| v.parse().ok())
        .map(m::WebpSettings::clamp_quality)
        .unwrap_or(m::DEFAULT_WEBP_QUALITY);
    let method = read_key(client, "webp_method")
        .await?
        .and_then(|v| v.parse().ok())
        .map(m::WebpSettings::clamp_method)
        .unwrap_or(m::DEFAULT_WEBP_METHOD);

    Ok(WebpSettings { quality, method })
}

/// 读取 WebP 配置（面板用）。
#[server(GetWebpSettings, "/api")]
pub async fn get_webp_settings() -> Result<WebpSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let s = load_webp_settings(&client)
            .await
            .map_err(ServerFnError::from)?;
        Ok(s)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(WebpSettings::default())
    }
}

/// 更新 WebP 配置。
///
/// 字段会被 clamp 后写入 DB。由于配置烘焙进 LazyLock 静态量，修改后需**重启进程**
/// 生效——不做运行时缓存失效。
#[server(UpdateWebpSettings, "/api")]
pub async fn update_webp_settings(
    quality: f32,
    method: u32,
) -> Result<WebpSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let quality = WebpSettings::clamp_quality(quality);
    let method = WebpSettings::clamp_method(method);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        for (key, value) in [
            ("webp_quality", quality.to_string()),
            ("webp_method", method.to_string()),
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
            "WebP settings updated (需重启生效): quality={}, method={}",
            quality,
            method
        );
    }

    Ok(WebpSettings { quality, method })
}
