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
use crate::models::settings::RateLimitSettings;

// ============================================================================
// 限流配置（重启生效）
// ============================================================================
//
// 原本经环境变量在首次请求时经 LazyLock 读取（src/api/rate_limit.rs）。迁移到
// settings 表后语义为 Tier B：env 首启播种 DB，之后面板值优先。由于限流器是
// LazyLock 静态量，修改 DB 值需**重启进程**生效，不做运行时缓存失效。

/// 启动时用 `RATE_LIMIT_*` 环境变量播种限流配置。
///
/// 语义与 [`seed_image_cache_settings_from_env`][crate::api::settings::seed_image_cache_settings_from_env] 一致：仅当对应 settings 键
/// **不存在**时插入（首次部署），之后以面板写入的 DB 值为准，重启不被 env 覆盖。
/// 单个变量非法只告警跳过，不影响其他变量与启动。
#[cfg(feature = "server")]
pub(crate) async fn seed_rate_limit_from_env(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    // (env 变量名, DB 键, clamp 函数)
    type EnvField = (&'static str, &'static str, fn(u32) -> u32);

    use crate::models::settings::RateLimitSettings as R;

    let mut seeds: Vec<(&'static str, String)> = Vec::new();

    let fields: &[EnvField] = &[
        (
            "RATE_LIMIT_STRICT_PER_SEC",
            "ratelimit_strict_per_sec",
            R::clamp_per_sec,
        ),
        (
            "RATE_LIMIT_STRICT_BURST",
            "ratelimit_strict_burst",
            R::clamp_burst,
        ),
        (
            "RATE_LIMIT_UPLOAD_PER_SEC",
            "ratelimit_upload_per_sec",
            R::clamp_per_sec,
        ),
        (
            "RATE_LIMIT_UPLOAD_BURST",
            "ratelimit_upload_burst",
            R::clamp_burst,
        ),
        (
            "RATE_LIMIT_IMAGE_PER_SEC",
            "ratelimit_image_per_sec",
            R::clamp_per_sec,
        ),
        (
            "RATE_LIMIT_IMAGE_BURST",
            "ratelimit_image_burst",
            R::clamp_burst,
        ),
        (
            "RATE_LIMIT_COMMENT_PER_SEC",
            "ratelimit_comment_per_sec",
            R::clamp_per_sec,
        ),
        (
            "RATE_LIMIT_COMMENT_BURST",
            "ratelimit_comment_burst",
            R::clamp_burst,
        ),
        (
            "RATE_LIMIT_COMMENT_UPLOAD_PER_SEC",
            "ratelimit_comment_upload_per_sec",
            R::clamp_per_sec,
        ),
        (
            "RATE_LIMIT_COMMENT_UPLOAD_BURST",
            "ratelimit_comment_upload_burst",
            R::clamp_burst,
        ),
        (
            "RATE_LIMIT_COMMENT_UPLOAD_DAILY",
            "ratelimit_comment_upload_daily",
            R::clamp_daily,
        ),
        (
            "RATE_LIMIT_CODE_EXEC_PER_SEC",
            "ratelimit_code_exec_per_sec",
            R::clamp_per_sec,
        ),
        (
            "RATE_LIMIT_CODE_EXEC_BURST",
            "ratelimit_code_exec_burst",
            R::clamp_burst,
        ),
        (
            "RATE_LIMIT_CODE_EXEC_DAILY",
            "ratelimit_code_exec_daily",
            R::clamp_daily,
        ),
        (
            "RATE_LIMIT_UNKNOWN_PER_SEC",
            "ratelimit_unknown_per_sec",
            R::clamp_per_sec,
        ),
        (
            "RATE_LIMIT_UNKNOWN_BURST",
            "ratelimit_unknown_burst",
            R::clamp_burst,
        ),
        (
            "RATE_LIMIT_GC_INTERVAL_SECS",
            "ratelimit_gc_interval_secs",
            R::clamp_gc_interval,
        ),
    ];

    for &(env_key, db_key, clamp) in fields {
        if let Ok(v) = std::env::var(env_key) {
            match v.trim().parse::<u32>() {
                Ok(n) => seeds.push((db_key, clamp(n).to_string())),
                Err(_) => tracing::warn!("{env_key}={v:?} 非法（期望正整数），跳过"),
            }
        }
    }

    super::insert_env_seeds(client, seeds).await
}

/// 从 settings 表读取限流配置（缺键回退默认值）。
/// 启动时由 startup.rs 调用，将结果写入 `config::RATE_LIMIT_CFG`，供 rate_limit.rs
/// 的 LazyLock 在首次请求时读取。
#[cfg(feature = "server")]
pub(crate) async fn load_rate_limit_settings(
    client: &tokio_postgres::Client,
) -> Result<RateLimitSettings, AppError> {
    use crate::models::settings as m;
    use RateLimitSettings as R;

    let values = super::load_setting_values(
        client,
        &[
            "ratelimit_strict_per_sec",
            "ratelimit_strict_burst",
            "ratelimit_upload_per_sec",
            "ratelimit_upload_burst",
            "ratelimit_image_per_sec",
            "ratelimit_image_burst",
            "ratelimit_comment_per_sec",
            "ratelimit_comment_burst",
            "ratelimit_comment_upload_per_sec",
            "ratelimit_comment_upload_burst",
            "ratelimit_comment_upload_daily",
            "ratelimit_code_exec_per_sec",
            "ratelimit_code_exec_burst",
            "ratelimit_code_exec_daily",
            "ratelimit_unknown_per_sec",
            "ratelimit_unknown_burst",
            "ratelimit_gc_interval_secs",
        ],
    )
    .await?;
    let read_clamped = |key: &str, default: u32, clamp: fn(u32) -> u32| {
        values
            .get(key)
            .and_then(|v| v.parse().ok())
            .map(clamp)
            .unwrap_or(default)
    };

    Ok(RateLimitSettings {
        strict_per_sec: read_clamped(
            "ratelimit_strict_per_sec",
            m::DEFAULT_RATE_LIMIT_STRICT_PER_SEC,
            R::clamp_per_sec,
        ),
        strict_burst: read_clamped(
            "ratelimit_strict_burst",
            m::DEFAULT_RATE_LIMIT_STRICT_BURST,
            R::clamp_burst,
        ),
        upload_per_sec: read_clamped(
            "ratelimit_upload_per_sec",
            m::DEFAULT_RATE_LIMIT_UPLOAD_PER_SEC,
            R::clamp_per_sec,
        ),
        upload_burst: read_clamped(
            "ratelimit_upload_burst",
            m::DEFAULT_RATE_LIMIT_UPLOAD_BURST,
            R::clamp_burst,
        ),
        image_per_sec: read_clamped(
            "ratelimit_image_per_sec",
            m::DEFAULT_RATE_LIMIT_IMAGE_PER_SEC,
            R::clamp_per_sec,
        ),
        image_burst: read_clamped(
            "ratelimit_image_burst",
            m::DEFAULT_RATE_LIMIT_IMAGE_BURST,
            R::clamp_burst,
        ),
        comment_per_sec: read_clamped(
            "ratelimit_comment_per_sec",
            m::DEFAULT_RATE_LIMIT_COMMENT_PER_SEC,
            R::clamp_per_sec,
        ),
        comment_burst: read_clamped(
            "ratelimit_comment_burst",
            m::DEFAULT_RATE_LIMIT_COMMENT_BURST,
            R::clamp_burst,
        ),
        comment_upload_per_sec: read_clamped(
            "ratelimit_comment_upload_per_sec",
            m::DEFAULT_RATE_LIMIT_COMMENT_UPLOAD_PER_SEC,
            R::clamp_per_sec,
        ),
        comment_upload_burst: read_clamped(
            "ratelimit_comment_upload_burst",
            m::DEFAULT_RATE_LIMIT_COMMENT_UPLOAD_BURST,
            R::clamp_burst,
        ),
        comment_upload_daily: read_clamped(
            "ratelimit_comment_upload_daily",
            m::DEFAULT_RATE_LIMIT_COMMENT_UPLOAD_DAILY,
            R::clamp_daily,
        ),
        code_exec_per_sec: read_clamped(
            "ratelimit_code_exec_per_sec",
            m::DEFAULT_RATE_LIMIT_CODE_EXEC_PER_SEC,
            R::clamp_per_sec,
        ),
        code_exec_burst: read_clamped(
            "ratelimit_code_exec_burst",
            m::DEFAULT_RATE_LIMIT_CODE_EXEC_BURST,
            R::clamp_burst,
        ),
        code_exec_daily: read_clamped(
            "ratelimit_code_exec_daily",
            m::DEFAULT_RATE_LIMIT_CODE_EXEC_DAILY,
            R::clamp_daily,
        ),
        unknown_per_sec: read_clamped(
            "ratelimit_unknown_per_sec",
            m::DEFAULT_RATE_LIMIT_UNKNOWN_PER_SEC,
            R::clamp_per_sec,
        ),
        unknown_burst: read_clamped(
            "ratelimit_unknown_burst",
            m::DEFAULT_RATE_LIMIT_UNKNOWN_BURST,
            R::clamp_burst,
        ),
        gc_interval_secs: read_clamped(
            "ratelimit_gc_interval_secs",
            m::DEFAULT_RATE_LIMIT_GC_INTERVAL_SECS,
            R::clamp_gc_interval,
        ),
    })
}

/// 读取限流配置（面板用）。
#[server(GetRateLimitSettings, "/api")]
pub async fn get_rate_limit_settings() -> Result<RateLimitSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let s = load_rate_limit_settings(&client)
            .await
            .map_err(ServerFnError::from)?;
        Ok(s)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(RateLimitSettings::default())
    }
}

/// 更新限流配置。
///
/// 所有字段会被 clamp 后写入 DB。由于限流器是 LazyLock 静态量（首次请求构造即
/// 固化），修改后需**重启进程**生效——不做运行时缓存失效。
#[server(UpdateRateLimitSettings, "/api")]
pub async fn update_rate_limit_settings(
    strict_per_sec: u32,
    strict_burst: u32,
    upload_per_sec: u32,
    upload_burst: u32,
    image_per_sec: u32,
    image_burst: u32,
    comment_per_sec: u32,
    comment_burst: u32,
    comment_upload_per_sec: u32,
    comment_upload_burst: u32,
    comment_upload_daily: u32,
    code_exec_per_sec: u32,
    code_exec_burst: u32,
    code_exec_daily: u32,
    unknown_per_sec: u32,
    unknown_burst: u32,
    gc_interval_secs: u32,
) -> Result<RateLimitSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let strict_per_sec = RateLimitSettings::clamp_per_sec(strict_per_sec);
    let strict_burst = RateLimitSettings::clamp_burst(strict_burst);
    let upload_per_sec = RateLimitSettings::clamp_per_sec(upload_per_sec);
    let upload_burst = RateLimitSettings::clamp_burst(upload_burst);
    let image_per_sec = RateLimitSettings::clamp_per_sec(image_per_sec);
    let image_burst = RateLimitSettings::clamp_burst(image_burst);
    let comment_per_sec = RateLimitSettings::clamp_per_sec(comment_per_sec);
    let comment_burst = RateLimitSettings::clamp_burst(comment_burst);
    let comment_upload_per_sec = RateLimitSettings::clamp_per_sec(comment_upload_per_sec);
    let comment_upload_burst = RateLimitSettings::clamp_burst(comment_upload_burst);
    let comment_upload_daily = RateLimitSettings::clamp_daily(comment_upload_daily);
    let code_exec_per_sec = RateLimitSettings::clamp_per_sec(code_exec_per_sec);
    let code_exec_burst = RateLimitSettings::clamp_burst(code_exec_burst);
    let code_exec_daily = RateLimitSettings::clamp_daily(code_exec_daily);
    let unknown_per_sec = RateLimitSettings::clamp_per_sec(unknown_per_sec);
    let unknown_burst = RateLimitSettings::clamp_burst(unknown_burst);
    let gc_interval_secs = RateLimitSettings::clamp_gc_interval(gc_interval_secs);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        for (key, value) in [
            ("ratelimit_strict_per_sec", strict_per_sec.to_string()),
            ("ratelimit_strict_burst", strict_burst.to_string()),
            ("ratelimit_upload_per_sec", upload_per_sec.to_string()),
            ("ratelimit_upload_burst", upload_burst.to_string()),
            ("ratelimit_image_per_sec", image_per_sec.to_string()),
            ("ratelimit_image_burst", image_burst.to_string()),
            ("ratelimit_comment_per_sec", comment_per_sec.to_string()),
            ("ratelimit_comment_burst", comment_burst.to_string()),
            (
                "ratelimit_comment_upload_per_sec",
                comment_upload_per_sec.to_string(),
            ),
            (
                "ratelimit_comment_upload_burst",
                comment_upload_burst.to_string(),
            ),
            (
                "ratelimit_comment_upload_daily",
                comment_upload_daily.to_string(),
            ),
            ("ratelimit_code_exec_per_sec", code_exec_per_sec.to_string()),
            ("ratelimit_code_exec_burst", code_exec_burst.to_string()),
            ("ratelimit_code_exec_daily", code_exec_daily.to_string()),
            ("ratelimit_unknown_per_sec", unknown_per_sec.to_string()),
            ("ratelimit_unknown_burst", unknown_burst.to_string()),
            ("ratelimit_gc_interval_secs", gc_interval_secs.to_string()),
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
            "Rate limit settings updated (需重启生效): strict={}/{}, upload={}/{}, \
             image={}/{}, comment={}/{}, comment_upload={}/{}/{}, code_exec={}/{}/{}, unknown={}/{}, gc={}s",
            strict_per_sec,
            strict_burst,
            upload_per_sec,
            upload_burst,
            image_per_sec,
            image_burst,
            comment_per_sec,
            comment_burst,
            comment_upload_per_sec,
            comment_upload_burst,
            comment_upload_daily,
            code_exec_per_sec,
            code_exec_burst,
            code_exec_daily,
            unknown_per_sec,
            unknown_burst,
            gc_interval_secs
        );
    }

    Ok(RateLimitSettings {
        strict_per_sec,
        strict_burst,
        upload_per_sec,
        upload_burst,
        image_per_sec,
        image_burst,
        comment_per_sec,
        comment_burst,
        comment_upload_per_sec,
        comment_upload_burst,
        comment_upload_daily,
        code_exec_per_sec,
        code_exec_burst,
        code_exec_daily,
        unknown_per_sec,
        unknown_burst,
        gc_interval_secs,
    })
}
