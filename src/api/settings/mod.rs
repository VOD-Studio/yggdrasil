//! 回收站、自动备份、素材上传与站点配置接口。
//!
//! 按子域拆分为目录模块：`trash`（回收站）、`backup`（自动备份）、`security`
//! （安全）、`image_cache`（图片磁盘缓存）、`asset_purge`（孤儿素材清理）、
//! `rate_limit`（限流）、`upload`（素材上传）、`site`（站点公开配置）、
//! `system`（系统启动信息）、`webp`（WebP 编码）、`image_limit`（图片尺寸限制）、
//! `runner`（代码运行器）。全部公开项经本模块 `pub use` 聚合，
//! `crate::api::settings::xxx` 路径保持不变，外部调用零改动。
//!
//! - 回收站配置：读取与更新自动清理设置，需要 admin 权限。
//! - 自动备份配置：读取与更新定时备份设置（含上次结果/下次执行时间），需要 admin。
//!   `load_backup_settings` / `save_last_backup_run` 同时供备份核心与调度任务复用。
//!   同名 `BACKUP_*` 环境变量仅在对应 settings 键缺失时播种初始值（首次部署），
//!   之后以面板写入的 DB 值为准。
//! - 素材上传配置：读取与更新上传弹窗并发数，需要 admin。`UPLOAD_CONCURRENCY`
//!   环境变量播种语义与 `BACKUP_*` 一致（仅键缺失时生效）。
//! - 站点公开配置：页脚 GitHub 链接等，`get_site_settings` 公开读取（前台页脚 SSR），
//!   `update_site_settings` 仅 admin。配置持久化到 settings 键值表。
//! Dioxus server function，注册在 `/api` 路径下。

mod asset_purge;
mod backup;
mod image_cache;
mod image_limit;
mod rate_limit;
mod runner;
mod security;
mod site;
mod system;
mod trash;
mod upload;
mod webp;

#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use std::collections::HashMap;

/// Batch-insert first-boot environment seeds.
///
/// Each settings group validates its own environment variables, then delegates
/// the write here so one group costs one round trip instead of one per key.
#[cfg(feature = "server")]
pub(crate) async fn insert_env_seeds(
    client: &tokio_postgres::Client,
    seeds: Vec<(&'static str, String)>,
) -> Result<(), AppError> {
    if seeds.is_empty() {
        return Ok(());
    }

    let keys: Vec<&str> = seeds.iter().map(|(key, _)| *key).collect();
    let values: Vec<&str> = seeds.iter().map(|(_, value)| value.as_str()).collect();
    client
        .execute(
            "INSERT INTO settings (key, value)
             SELECT * FROM UNNEST($1::text[], $2::text[])
             ON CONFLICT (key) DO NOTHING",
            &[&keys, &values],
        )
        .await
        .map_err(AppError::query)?;

    for (key, value) in seeds {
        tracing::info!("配置已从环境变量播种: {key}={value}（仅键缺失时生效）");
    }
    Ok(())
}

/// Load a settings group with one indexed query.
#[cfg(feature = "server")]
pub(crate) async fn load_setting_values(
    client: &tokio_postgres::Client,
    keys: &[&str],
) -> Result<HashMap<String, String>, AppError> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }

    let keys: Vec<&str> = keys.to_vec();
    let rows = client
        .query(
            "SELECT key, value FROM settings WHERE key = ANY($1::text[])",
            &[&keys],
        )
        .await
        .map_err(AppError::query)?;

    let mut values = HashMap::with_capacity(rows.len());
    for row in rows {
        values.insert(row.get::<_, String>("key"), row.get::<_, String>("value"));
    }
    Ok(values)
}

/// Seed all first-boot settings and bake Tier-B settings into process config.
///
/// Environment seeding is best-effort by design. Tier-B reads are startup
/// critical: silently falling back would permanently replace administrator-
/// configured limits until the next restart.
#[cfg(feature = "server")]
pub(crate) async fn bootstrap_startup_settings(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    let (backup, upload, security, image_cache, asset_purge, rate_limit, webp, image_limit, runner) = tokio::join!(
        seed_backup_settings_from_env(client),
        seed_upload_settings_from_env(client),
        seed_security_settings_from_env(client),
        seed_image_cache_settings_from_env(client),
        seed_asset_purge_settings_from_env(client),
        seed_rate_limit_from_env(client),
        seed_webp_settings_from_env(client),
        seed_image_limit_settings_from_env(client),
        seed_runner_settings_from_env(client),
    );

    for (name, result) in [
        ("backup", backup),
        ("upload", upload),
        ("security", security),
        ("image_cache", image_cache),
        ("asset_purge", asset_purge),
        ("rate_limit", rate_limit),
        ("webp", webp),
        ("image_limit", image_limit),
        ("runner", runner),
    ] {
        if let Err(error) = result {
            tracing::warn!(setting_group = name, error = ?error, "启动设置环境变量播种失败");
        }
    }

    let (rate_limit, webp, image_limit, runner) = tokio::join!(
        load_rate_limit_settings(client),
        load_webp_settings(client),
        load_image_limit_settings(client),
        load_runner_settings(client),
    );

    let rate_limit = rate_limit.map_err(|error| {
        tracing::error!(error = ?error, "限流启动配置加载失败，拒绝使用默认值");
        error
    })?;
    let webp = webp.map_err(|error| {
        tracing::error!(error = ?error, "WebP 启动配置加载失败，拒绝使用默认值");
        error
    })?;
    let image_limit = image_limit.map_err(|error| {
        tracing::error!(error = ?error, "图片限制启动配置加载失败，拒绝使用默认值");
        error
    })?;
    let runner = runner.map_err(|error| {
        tracing::error!(error = ?error, "代码运行器启动配置加载失败，拒绝使用默认值");
        error
    })?;

    crate::config::set_rate_limit(rate_limit);
    crate::config::set_webp(webp);
    crate::config::set_image_limit(image_limit);
    crate::config::set_runner(runner);
    Ok(())
}

pub use asset_purge::*;
pub use backup::*;
pub use image_cache::*;
pub use image_limit::*;
pub use rate_limit::*;
pub use runner::*;
pub use security::*;
pub use site::*;
// system 的唯一调用方是 system_section.rs 的 #[cfg(target_arch = "wasm32")] 导入，
// 原生构建下该 glob 无人消费；binary crate 里 pub 不豁免 unused_imports，故放行。
#[allow(unused_imports)]
pub use system::*;
pub use trash::*;
pub use upload::*;
pub use webp::*;
