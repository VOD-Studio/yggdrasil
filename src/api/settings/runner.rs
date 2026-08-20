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
use crate::models::settings::RunnerSettings;

// ============================================================================
// 代码运行器配置（需重启生效）
// ============================================================================

/// 启动时用 `CODE_RUNNER_*` 环境变量播种代码运行器配置。
#[cfg(feature = "server")]
pub(crate) async fn seed_runner_settings_from_env(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    use crate::models::settings as m;

    let mut seeds: Vec<(&'static str, String)> = Vec::new();

    if let Ok(v) = std::env::var("CODE_RUNNER_ALLOW_NETWORK") {
        let l = v.to_lowercase();
        let b = l == "true" || l == "1" || l == "yes";
        seeds.push(("runner_allow_network", b.to_string()));
    }
    if let Ok(v) = std::env::var("CODE_RUNNER_MAX_CONCURRENT") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "runner_max_concurrent",
                m::RunnerSettings::clamp_max_concurrent(n).to_string(),
            )),
            Err(_) => tracing::warn!("CODE_RUNNER_MAX_CONCURRENT={v:?} 非法，跳过"),
        }
    }
    if let Ok(v) = std::env::var("CODE_RUNNER_MAX_CPU_CORES") {
        match v.trim().parse::<f64>() {
            Ok(n) => seeds.push((
                "runner_max_cpu_cores",
                m::RunnerSettings::clamp_max_cpu_cores(n).to_string(),
            )),
            Err(_) => tracing::warn!("CODE_RUNNER_MAX_CPU_CORES={v:?} 非法，跳过"),
        }
    }
    if let Ok(v) = std::env::var("CODE_RUNNER_MAX_MEMORY_MB") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "runner_max_memory_mb",
                m::RunnerSettings::clamp_max_memory_mb(n).to_string(),
            )),
            Err(_) => tracing::warn!("CODE_RUNNER_MAX_MEMORY_MB={v:?} 非法，跳过"),
        }
    }
    if let Ok(v) = std::env::var("CODE_RUNNER_MAX_TIMEOUT_SECS") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "runner_max_timeout_secs",
                m::RunnerSettings::clamp_max_timeout_secs(n).to_string(),
            )),
            Err(_) => tracing::warn!("CODE_RUNNER_MAX_TIMEOUT_SECS={v:?} 非法，跳过"),
        }
    }
    if let Ok(v) = std::env::var("CODE_RUNNER_MAX_OUTPUT_BYTES") {
        match v.trim().parse::<u64>() {
            Ok(n) => seeds.push((
                "runner_max_output_bytes",
                m::RunnerSettings::clamp_max_output_bytes(n).to_string(),
            )),
            Err(_) => tracing::warn!("CODE_RUNNER_MAX_OUTPUT_BYTES={v:?} 非法，跳过"),
        }
    }
    if let Ok(v) = std::env::var("CODE_RUNNER_MAX_SOURCE_BYTES") {
        match v.trim().parse::<u64>() {
            Ok(n) => seeds.push((
                "runner_max_source_bytes",
                m::RunnerSettings::clamp_max_source_bytes(n).to_string(),
            )),
            Err(_) => tracing::warn!("CODE_RUNNER_MAX_SOURCE_BYTES={v:?} 非法，跳过"),
        }
    }
    if let Ok(v) = std::env::var("CODE_RUNNER_QUEUE_TIMEOUT_SECS") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "runner_queue_timeout_secs",
                m::RunnerSettings::clamp_queue_timeout_secs(n).to_string(),
            )),
            Err(_) => tracing::warn!("CODE_RUNNER_QUEUE_TIMEOUT_SECS={v:?} 非法，跳过"),
        }
    }
    if let Ok(v) = std::env::var("CODE_RUNNER_TASK_TTL_SECS") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "runner_task_ttl_secs",
                m::RunnerSettings::clamp_task_ttl_secs(n).to_string(),
            )),
            Err(_) => tracing::warn!("CODE_RUNNER_TASK_TTL_SECS={v:?} 非法，跳过"),
        }
    }
    if let Ok(v) = std::env::var("CODE_RUNNER_LANGUAGES") {
        if let Some(norm) = m::RunnerSettings::normalize_languages(&v) {
            seeds.push(("runner_languages", norm));
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
        tracing::info!("运行器配置已从环境变量播种: {key}={value}（仅键缺失时生效）");
    }
    Ok(())
}

/// 从 settings 表读取代码运行器配置（缺键回退默认值）。
#[cfg(feature = "server")]
pub(crate) async fn load_runner_settings(
    client: &tokio_postgres::Client,
) -> Result<RunnerSettings, AppError> {
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

    let allow_network = read_key(client, "runner_allow_network")
        .await?
        .and_then(|v| v.parse().ok())
        .unwrap_or(m::DEFAULT_RUNNER_ALLOW_NETWORK);
    let max_concurrent = read_key(client, "runner_max_concurrent")
        .await?
        .and_then(|v| v.parse().ok())
        .map(m::RunnerSettings::clamp_max_concurrent)
        .unwrap_or(m::DEFAULT_RUNNER_MAX_CONCURRENT);
    let max_cpu_cores = read_key(client, "runner_max_cpu_cores")
        .await?
        .and_then(|v| v.parse().ok())
        .map(m::RunnerSettings::clamp_max_cpu_cores)
        .unwrap_or(m::DEFAULT_RUNNER_MAX_CPU_CORES);
    let max_memory_mb = read_key(client, "runner_max_memory_mb")
        .await?
        .and_then(|v| v.parse().ok())
        .map(m::RunnerSettings::clamp_max_memory_mb)
        .unwrap_or(m::DEFAULT_RUNNER_MAX_MEMORY_MB);
    let max_timeout_secs = read_key(client, "runner_max_timeout_secs")
        .await?
        .and_then(|v| v.parse().ok())
        .map(m::RunnerSettings::clamp_max_timeout_secs)
        .unwrap_or(m::DEFAULT_RUNNER_MAX_TIMEOUT_SECS);
    let max_output_bytes = read_key(client, "runner_max_output_bytes")
        .await?
        .and_then(|v| v.parse().ok())
        .map(m::RunnerSettings::clamp_max_output_bytes)
        .unwrap_or(m::DEFAULT_RUNNER_MAX_OUTPUT_BYTES);
    let max_source_bytes = read_key(client, "runner_max_source_bytes")
        .await?
        .and_then(|v| v.parse().ok())
        .map(m::RunnerSettings::clamp_max_source_bytes)
        .unwrap_or(m::DEFAULT_RUNNER_MAX_SOURCE_BYTES);
    let queue_timeout_secs = read_key(client, "runner_queue_timeout_secs")
        .await?
        .and_then(|v| v.parse().ok())
        .map(m::RunnerSettings::clamp_queue_timeout_secs)
        .unwrap_or(m::DEFAULT_RUNNER_QUEUE_TIMEOUT_SECS);
    let task_ttl_secs = read_key(client, "runner_task_ttl_secs")
        .await?
        .and_then(|v| v.parse().ok())
        .map(m::RunnerSettings::clamp_task_ttl_secs)
        .unwrap_or(m::DEFAULT_RUNNER_TASK_TTL_SECS);
    let languages = read_key(client, "runner_languages")
        .await?
        .and_then(|v| m::RunnerSettings::normalize_languages(&v));

    Ok(RunnerSettings {
        allow_network,
        max_concurrent,
        max_cpu_cores,
        max_memory_mb,
        max_timeout_secs,
        max_output_bytes,
        max_source_bytes,
        queue_timeout_secs,
        task_ttl_secs,
        languages,
    })
}

/// 读取代码运行器配置（面板用）。
#[server(GetRunnerSettings, "/api")]
pub async fn get_runner_settings() -> Result<RunnerSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let s = load_runner_settings(&client)
            .await
            .map_err(ServerFnError::from)?;
        Ok(s)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(RunnerSettings::default())
    }
}

/// 更新代码运行器配置。
///
/// 字段会被 clamp / 规范化后写入 DB。配置烘焙进 LazyLock，修改后需**重启进程**生效。
#[server(UpdateRunnerSettings, "/api")]
pub async fn update_runner_settings(
    allow_network: bool,
    max_concurrent: u32,
    max_cpu_cores: f64,
    max_memory_mb: u32,
    max_timeout_secs: u32,
    max_output_bytes: u64,
    max_source_bytes: u64,
    queue_timeout_secs: u32,
    task_ttl_secs: u32,
    languages: Option<String>,
) -> Result<RunnerSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let max_concurrent = RunnerSettings::clamp_max_concurrent(max_concurrent);
    let max_cpu_cores = RunnerSettings::clamp_max_cpu_cores(max_cpu_cores);
    let max_memory_mb = RunnerSettings::clamp_max_memory_mb(max_memory_mb);
    let max_timeout_secs = RunnerSettings::clamp_max_timeout_secs(max_timeout_secs);
    let max_output_bytes = RunnerSettings::clamp_max_output_bytes(max_output_bytes);
    let max_source_bytes = RunnerSettings::clamp_max_source_bytes(max_source_bytes);
    let queue_timeout_secs = RunnerSettings::clamp_queue_timeout_secs(queue_timeout_secs);
    let task_ttl_secs = RunnerSettings::clamp_task_ttl_secs(task_ttl_secs);
    let languages = languages.and_then(|s| RunnerSettings::normalize_languages(&s));

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        let lang_str = languages.clone().unwrap_or_default();
        for (key, value) in [
            ("runner_allow_network", allow_network.to_string()),
            ("runner_max_concurrent", max_concurrent.to_string()),
            ("runner_max_cpu_cores", max_cpu_cores.to_string()),
            ("runner_max_memory_mb", max_memory_mb.to_string()),
            ("runner_max_timeout_secs", max_timeout_secs.to_string()),
            ("runner_max_output_bytes", max_output_bytes.to_string()),
            ("runner_max_source_bytes", max_source_bytes.to_string()),
            ("runner_queue_timeout_secs", queue_timeout_secs.to_string()),
            ("runner_task_ttl_secs", task_ttl_secs.to_string()),
            ("runner_languages", lang_str),
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
            "Runner settings updated (需重启生效): allow_network={}, max_concurrent={}, \
             cpu={}, mem={}MB, timeout={}s, languages={:?}",
            allow_network,
            max_concurrent,
            max_cpu_cores,
            max_memory_mb,
            max_timeout_secs,
            languages
        );
    }

    Ok(RunnerSettings {
        allow_network,
        max_concurrent,
        max_cpu_cores,
        max_memory_mb,
        max_timeout_secs,
        max_output_bytes,
        max_source_bytes,
        queue_timeout_secs,
        task_ttl_secs,
        languages,
    })
}
