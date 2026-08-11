//! 回收站、自动备份、素材上传与站点配置接口。
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

// 与 posts 模块一致：Dioxus `#[server]` 宏触发 deprecated/unit 提示，按项目惯例放行。
#![allow(clippy::unused_unit, deprecated)]

use dioxus::prelude::*;

#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;
use crate::models::settings::{
    BackupSettingsView, ImageCacheSettings, SecuritySettings, SiteSettings, TrashSettings,
    UploadSettings,
};
// 仅 server 构建的函数体引用（WASM 端 server fn 体被宏剥离）。
#[cfg(feature = "server")]
use crate::cache::{invalidate_image_cache_settings, invalidate_security_settings};
#[cfg(feature = "server")]
use crate::models::settings::{BackupSettings, LastBackupRun};

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

// ============================================================================
// 自动备份配置
// ============================================================================

/// 从 settings 表读取自动备份配置（缺键回退默认值）。
///
/// 供 `get_backup_settings`、备份核心（include_uploads / retention_count）
/// 与定时调度任务三处共用。
#[cfg(feature = "server")]
pub(crate) async fn load_backup_settings(
    client: &tokio_postgres::Client,
) -> Result<BackupSettings, AppError> {
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

    let auto_enabled = read_key(client, "backup_auto_enabled")
        .await?
        .and_then(|v| v.parse().ok())
        .unwrap_or(crate::models::settings::DEFAULT_BACKUP_AUTO_ENABLED);
    let time_utc = read_key(client, "backup_time_utc")
        .await?
        .map(|v| BackupSettings::normalize_time_utc(&v))
        .unwrap_or_else(|| crate::models::settings::DEFAULT_BACKUP_TIME_UTC.to_string());
    let retention_count = read_key(client, "backup_retention_count")
        .await?
        .and_then(|v| v.parse().ok())
        .map(BackupSettings::clamp_retention)
        .unwrap_or(crate::models::settings::DEFAULT_BACKUP_RETENTION_COUNT);
    let include_uploads = read_key(client, "backup_include_uploads")
        .await?
        .and_then(|v| v.parse().ok())
        .unwrap_or(crate::models::settings::DEFAULT_BACKUP_INCLUDE_UPLOADS);

    Ok(BackupSettings {
        auto_enabled,
        time_utc,
        retention_count,
        include_uploads,
    })
}

/// 读取最近一次自动备份结果（未执行过 → None）。
#[cfg(feature = "server")]
async fn load_last_backup_run(
    client: &tokio_postgres::Client,
) -> Result<Option<LastBackupRun>, AppError> {
    let rows = client
        .query(
            "SELECT key, value FROM settings WHERE key = ANY(ARRAY[
                'backup_last_run_at', 'backup_last_run_ok',
                'backup_last_run_file', 'backup_last_run_error'])",
            &[],
        )
        .await
        .map_err(AppError::query)?;
    let get = |k: &str| {
        rows.iter()
            .find(|r| r.get::<_, String>("key") == k)
            .map(|r| r.get::<_, String>("value"))
            .filter(|v| !v.is_empty())
    };
    let (Some(at), Some(ok)) = (get("backup_last_run_at"), get("backup_last_run_ok")) else {
        return Ok(None);
    };
    Ok(Some(LastBackupRun {
        at,
        ok: ok.parse().unwrap_or(false),
        file: get("backup_last_run_file"),
        error: get("backup_last_run_error"),
    }))
}

/// 落库最近一次自动备份结果（调度任务执行后调用；空串表示 None）。
#[cfg(feature = "server")]
pub(crate) async fn save_last_backup_run(
    client: &tokio_postgres::Client,
    run: &LastBackupRun,
) -> Result<(), AppError> {
    let pairs: [(&str, String); 4] = [
        ("backup_last_run_at", run.at.clone()),
        ("backup_last_run_ok", run.ok.to_string()),
        ("backup_last_run_file", run.file.clone().unwrap_or_default()),
        (
            "backup_last_run_error",
            run.error.clone().unwrap_or_default(),
        ),
    ];
    for (key, value) in pairs {
        client
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ($1, $2, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                &[&key, &value],
            )
            .await
            .map_err(AppError::query)?;
    }
    Ok(())
}

/// 启动时用 `BACKUP_*` 环境变量播种自动备份配置。
///
/// 仅当对应 settings 键**不存在**时插入（`ON CONFLICT DO NOTHING`）——
/// 即 env 只决定首次部署的初始值，之后以后台面板写入的 DB 值为准，
/// 重启不会被 env 覆盖。单个变量非法只告警跳过，不影响其他变量与启动。
#[cfg(feature = "server")]
pub(crate) async fn seed_backup_settings_from_env(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    let mut seeds: Vec<(&'static str, String)> = Vec::new();

    if let Ok(v) = std::env::var("BACKUP_AUTO_ENABLED") {
        match v.trim().parse::<bool>() {
            Ok(b) => seeds.push(("backup_auto_enabled", b.to_string())),
            Err(_) => tracing::warn!("BACKUP_AUTO_ENABLED={v:?} 非法（期望 true/false），跳过"),
        }
    }
    if let Ok(v) = std::env::var("BACKUP_TIME_UTC") {
        if BackupSettings::is_valid_time_utc(&v) {
            seeds.push(("backup_time_utc", BackupSettings::normalize_time_utc(&v)));
        } else {
            tracing::warn!("BACKUP_TIME_UTC={v:?} 非法（期望 HH:MM，UTC），跳过");
        }
    }
    if let Ok(v) = std::env::var("BACKUP_RETENTION_COUNT") {
        match v.trim().parse::<i32>() {
            Ok(n) => seeds.push((
                "backup_retention_count",
                BackupSettings::clamp_retention(n).to_string(),
            )),
            Err(_) => tracing::warn!("BACKUP_RETENTION_COUNT={v:?} 非法（期望整数），跳过"),
        }
    }
    if let Ok(v) = std::env::var("BACKUP_INCLUDE_UPLOADS") {
        match v.trim().parse::<bool>() {
            Ok(b) => seeds.push(("backup_include_uploads", b.to_string())),
            Err(_) => tracing::warn!("BACKUP_INCLUDE_UPLOADS={v:?} 非法（期望 true/false），跳过"),
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
        tracing::info!("备份配置已从环境变量播种: {key}={value}（仅键缺失时生效）");
    }
    Ok(())
}

/// 组装面板视图：设置 + 上次结果 + 下次执行时间。
#[cfg(feature = "server")]
async fn load_backup_settings_view(
    client: &tokio_postgres::Client,
) -> Result<BackupSettingsView, AppError> {
    let settings = load_backup_settings(client).await?;
    let last_run = load_last_backup_run(client).await?;
    let next_run_at = if settings.auto_enabled {
        settings
            .next_run_after(chrono::Utc::now())
            .map(|dt| dt.to_rfc3339())
    } else {
        None
    };
    Ok(BackupSettingsView {
        settings,
        last_run,
        next_run_at,
    })
}

/// 读取自动备份配置（面板用）。
#[server(GetBackupSettings, "/api")]
pub async fn get_backup_settings() -> Result<BackupSettingsView, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        Ok(load_backup_settings_view(&client).await?)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(BackupSettingsView {
            settings: BackupSettings::default(),
            last_run: None,
            next_run_at: None,
        })
    }
}

/// 更新自动备份配置。
///
/// time_utc 会被规范化（非法回退默认），retention_count 会被 clamp。
/// 写入后唤醒调度任务立即重排，无需等待下一次 tick。
#[server(UpdateBackupSettings, "/api")]
pub async fn update_backup_settings(
    auto_enabled: bool,
    time_utc: String,
    retention_count: i32,
    include_uploads: bool,
) -> Result<BackupSettingsView, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let time_utc = BackupSettings::normalize_time_utc(&time_utc);
    let retention_count = BackupSettings::clamp_retention(retention_count);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let pairs = [
            ("backup_auto_enabled", auto_enabled.to_string()),
            ("backup_time_utc", time_utc.clone()),
            ("backup_retention_count", retention_count.to_string()),
            ("backup_include_uploads", include_uploads.to_string()),
        ];
        for (key, value) in pairs {
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
            "Backup settings updated: auto_enabled={}, time_utc={}, retention={}, include_uploads={}",
            auto_enabled,
            time_utc,
            retention_count,
            include_uploads
        );
        // 唤醒调度器立即重排（设置变更不应等到原定的下次触发才生效）。
        crate::tasks::backup::notify_settings_changed();
        Ok(load_backup_settings_view(&client).await?)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(BackupSettingsView {
            settings: BackupSettings {
                auto_enabled,
                time_utc,
                retention_count,
                include_uploads,
            },
            last_run: None,
            next_run_at: None,
        })
    }
}

// ============================================================================
// 安全配置（即时生效）
// ============================================================================

/// 启动时用 `APP_BASE_URL` / `COOKIE_SECURE` / `TRUSTED_PROXY_COUNT` /
/// `MAX_SESSIONS_PER_USER` 环境变量播种安全配置。
///
/// 语义与 [`seed_backup_settings_from_env`] 一致：仅当对应 settings 键**不存在**时
/// 插入（首次部署），之后以「站点配置 → 安全」面板写入的 DB 值为准，重启不被
/// env 覆盖。单个变量非法只告警跳过，不影响其他变量与启动。
#[cfg(feature = "server")]
pub(crate) async fn seed_security_settings_from_env(
    client: &tokio_postgres::Client,
) -> Result<(), AppError> {
    let mut seeds: Vec<(&'static str, String)> = Vec::new();

    if let Ok(v) = std::env::var("APP_BASE_URL") {
        let url = crate::models::settings::SecuritySettings::normalize_app_base_url(&v);
        if !url.is_empty() {
            seeds.push(("security_app_base_url", url));
        }
    }
    if let Ok(v) = std::env::var("COOKIE_SECURE") {
        match v.trim().parse::<bool>() {
            Ok(b) => seeds.push(("security_cookie_secure", b.to_string())),
            Err(_) => tracing::warn!("COOKIE_SECURE={v:?} 非法（期望 true/false），跳过"),
        }
    }
    if let Ok(v) = std::env::var("TRUSTED_PROXY_COUNT") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "security_trusted_proxy_count",
                crate::models::settings::SecuritySettings::clamp_trusted_proxy_count(n)
                    .to_string(),
            )),
            Err(_) => tracing::warn!("TRUSTED_PROXY_COUNT={v:?} 非法（期望非负整数），跳过"),
        }
    }
    if let Ok(v) = std::env::var("MAX_SESSIONS_PER_USER") {
        match v.trim().parse::<u32>() {
            Ok(n) => seeds.push((
                "security_max_sessions_per_user",
                crate::models::settings::SecuritySettings::clamp_max_sessions(n).to_string(),
            )),
            Err(_) => tracing::warn!("MAX_SESSIONS_PER_USER={v:?} 非法（期望正整数），跳过"),
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
        tracing::info!("安全配置已从环境变量播种: {key}={value}（仅键缺失时生效）");
    }
    Ok(())
}

/// 从 settings 表读取安全配置（缺键回退默认值）。
///
/// 供面板 `get_security_settings` 与运行时读取点（CSRF / cookie / 真实 IP /
/// 登录会话上限）共用：先查 moka 缓存，未命中查 DB 并回填。
#[cfg(feature = "server")]
pub(crate) async fn load_security_settings(
    client: &tokio_postgres::Client,
) -> Result<crate::models::settings::SecuritySettings, AppError> {
    async fn read_key(client: &tokio_postgres::Client, key: &str) -> Result<Option<String>, AppError> {
        let row = client
            .query_opt("SELECT value FROM settings WHERE key = $1", &[&key])
            .await
            .map_err(AppError::query)?;
        Ok(row.map(|r| r.get::<_, String>("value")))
    }

    let app_base_url = read_key(client, "security_app_base_url")
        .await?
        .map(|v| crate::models::settings::SecuritySettings::normalize_app_base_url(&v))
        .unwrap_or_default();
    let cookie_secure = read_key(client, "security_cookie_secure")
        .await?
        .and_then(|v| v.parse().ok())
        .unwrap_or(crate::models::settings::DEFAULT_COOKIE_SECURE);
    let trusted_proxy_count = read_key(client, "security_trusted_proxy_count")
        .await?
        .and_then(|v| v.parse().ok())
        .map(crate::models::settings::SecuritySettings::clamp_trusted_proxy_count)
        .unwrap_or(crate::models::settings::DEFAULT_TRUSTED_PROXY_COUNT);
    let max_sessions_per_user = read_key(client, "security_max_sessions_per_user")
        .await?
        .and_then(|v| v.parse().ok())
        .map(crate::models::settings::SecuritySettings::clamp_max_sessions)
        .unwrap_or(crate::models::settings::DEFAULT_MAX_SESSIONS_PER_USER);

    Ok(crate::models::settings::SecuritySettings {
        app_base_url,
        cookie_secure,
        trusted_proxy_count,
        max_sessions_per_user,
    })
}

/// 运行时读取安全配置：先查 moka 缓存，未命中查 DB 并回填，全失败回退默认值。
///
/// 供运行时读取点（CSRF / cookie / 真实 IP / 登录会话上限）调用，
/// 替代原来的 `std::env::var` 每请求读取。
#[cfg(feature = "server")]
pub(crate) async fn runtime_security_settings() -> crate::models::settings::SecuritySettings {
    if let Some(s) = crate::cache::get_security_settings().await {
        return s;
    }
    let fallback = crate::models::settings::SecuritySettings::default();
    // 无 DATABASE_URL 时（如单元测试环境）直接回退默认值，
    // 避免触发 DB_POOL LazyLock 的防御性 panic。
    if std::env::var("DATABASE_URL").is_err() {
        return fallback;
    }
    match get_conn().await {
        Ok(client) => match load_security_settings(&client).await {
            Ok(s) => {
                crate::cache::set_security_settings(s.clone()).await;
                s
            }
            Err(e) => {
                tracing::warn!("读取安全配置失败，回退默认值：{e:?}");
                fallback
            }
        },
        Err(e) => {
            tracing::warn!("获取连接读取安全配置失败，回退默认值：{e:?}");
            fallback
        }
    }
}

/// 读取安全配置（面板用）。
#[server(GetSecuritySettings, "/api")]
pub async fn get_security_settings() -> Result<SecuritySettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let s = load_security_settings(&client).await.map_err(ServerFnError::from)?;
        crate::cache::set_security_settings(s.clone()).await;
        Ok(s)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(SecuritySettings::default())
    }
}

/// 更新安全配置。
///
/// 所有字段会被规范化/clamp 后写入。写入后失效 moka 缓存，数秒内全链路生效。
#[server(UpdateSecuritySettings, "/api")]
pub async fn update_security_settings(
    app_base_url: String,
    cookie_secure: bool,
    trusted_proxy_count: u32,
    max_sessions_per_user: u32,
) -> Result<SecuritySettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let app_base_url = SecuritySettings::normalize_app_base_url(&app_base_url);
    let trusted_proxy_count = SecuritySettings::clamp_trusted_proxy_count(trusted_proxy_count);
    let max_sessions_per_user = SecuritySettings::clamp_max_sessions(max_sessions_per_user);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        for (key, value) in [
            ("security_app_base_url", app_base_url.clone()),
            ("security_cookie_secure", cookie_secure.to_string()),
            (
                "security_trusted_proxy_count",
                trusted_proxy_count.to_string(),
            ),
            (
                "security_max_sessions_per_user",
                max_sessions_per_user.to_string(),
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

        invalidate_security_settings();
        tracing::info!(
            "Security settings updated: app_base_url={:?}, cookie_secure={}, proxy_count={}, max_sessions={}",
            app_base_url,
            cookie_secure,
            trusted_proxy_count,
            max_sessions_per_user
        );
    }

    Ok(SecuritySettings {
        app_base_url,
        cookie_secure,
        trusted_proxy_count,
        max_sessions_per_user,
    })
}

// ============================================================================
// 图片磁盘缓存配置（即时生效）
// ============================================================================

/// 启动时用 `IMAGE_DISK_CACHE_MAX_MB` / `_MAX_AGE_HOURS` 环境变量播种。
///
/// 语义与 [`seed_backup_settings_from_env`] 一致。
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
    async fn read_key(client: &tokio_postgres::Client, key: &str) -> Result<Option<String>, AppError> {
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
        let s = load_image_cache_settings(&client).await.map_err(ServerFnError::from)?;
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
    let disk_cache_max_age_hours = ImageCacheSettings::clamp_max_age_hours(disk_cache_max_age_hours);

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

// ============================================================================
// 素材上传配置
// ============================================================================

/// 启动时用 `UPLOAD_CONCURRENCY` 环境变量播种素材上传并发数。
///
/// 语义与 [`seed_backup_settings_from_env`] 一致：仅当 settings 键**不存在**时
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
    client
        .execute(
            "INSERT INTO settings (key, value) VALUES ('upload_concurrency', $1) ON CONFLICT (key) DO NOTHING",
            &[&value.to_string()],
        )
        .await
        .map_err(AppError::query)?;
    tracing::info!("素材上传配置已从环境变量播种: upload_concurrency={value}（仅键缺失时生效）");
    Ok(())
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

        let concurrency: i32 = client
            .query_opt(
                "SELECT value FROM settings WHERE key = 'upload_concurrency'",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .and_then(|r| r.get::<_, String>("value").parse().ok())
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

// ============================================================================
// 站点公开配置
// ============================================================================

/// 读取站点公开配置。
///
/// **公开接口**（无需登录）：供前台页脚 SSR 读取 GitHub 链接等。先查 moka 缓存，
/// 未命中查 settings 表，缺失键回退默认值（空串=不展示图标）。
#[server(GetSiteSettings, "/api")]
pub async fn get_site_settings() -> Result<SiteSettings, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::models::settings::DEFAULT_SITE_GITHUB_URL;

        if let Some(cached) = crate::cache::get_site_settings().await {
            return Ok(cached);
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;

        let github_url: String = client
            .query_opt(
                "SELECT value FROM settings WHERE key = 'site_github_url'",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .map(|r| r.get::<_, String>("value"))
            .unwrap_or_else(|| DEFAULT_SITE_GITHUB_URL.to_string());

        let settings = SiteSettings { github_url };
        crate::cache::set_site_settings(settings.clone()).await;
        Ok(settings)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(SiteSettings::default())
    }
}

/// 更新站点公开配置。
///
/// 仅 admin。写入前规范化（trim / 截断 / 补 https://）；写后失效 moka 站点配置缓存
/// 与全部公开页 SSR 缓存（页脚出现在每个前台页面，无法定向失效）。
#[server(UpdateSiteSettings, "/api")]
pub async fn update_site_settings(github_url: String) -> Result<SiteSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    let github_url = SiteSettings::normalize_github_url(&github_url);

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        client
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES ('site_github_url', $1, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
                &[&github_url],
            )
            .await
            .map_err(AppError::query)?;

        crate::cache::invalidate_site_settings();
        crate::ssr_cache::invalidate_ssr_all_public();
        tracing::info!("Site settings updated: github_url={}", github_url);
    }

    Ok(SiteSettings { github_url })
}
