#![allow(clippy::unused_unit, deprecated)]

//! 备份与恢复（读写，最高风险）。
//!
//! 备份：探测 pg_dump 可用性——可用则子进程生成完整 .sql（`--clean --if-exists`，
//! 脚本自带 `DROP ... IF EXISTS`，使恢复幂等），不可用则回退纯 SQL（仅数据）。
//! 备份文件含签名头。每次备份按设置附带 uploads 素材打包（`_uploads.tar.gz`，
//! 排除可重建的 `.cache/`），与 .sql 成对展示/下载/删除。
//!
//! 来源标记：手动备份文件名前缀 `backup_`，定时任务自动备份前缀 `auto_`；
//! 自动备份按 `backup_retention_count` 轮转（只删自动，手动永不自动删）。
//!
//! 恢复：仅接受本系统生成的备份（签名校验）+ 二次确认 + 路径穿越防护；
//! 仅恢复数据库——uploads 需从配对 tar.gz 手动还原（tar 覆盖文件风险高，不自动）。
//! `psql -v ON_ERROR_STOP=1` 确保任何 SQL 错误立即中止并报失败（不再假成功）；
//! 成功后全量失效文章缓存与 SSR 世代号。
//! 长耗时操作走后台任务 + 进度轮询（见 [`crate::api::database::tasks`]）。
//!
//! 导入：`POST /api/database/backups/import`（纯 Axum 路由，multipart 流式落盘）
//! 把本机备份回灌进 backups/。两步式——导入只入库，恢复仍走上述管线。
//! 导入时即强制签名校验（外来 SQL 拒收不留盘）；保留原始文件名、同名冲突拒绝；
//! `.tmp` 落盘 + 原子 rename，半截文件永不出现在列表。上限 `BACKUP_IMPORT_MAX_MB`
//! （默认 512MB），路由层 DefaultBodyLimit + 流式计数双保险。
//!
//! **兼容性提示**：`--clean --if-exists` 是后加的备份参数。本修复之前生成的备份
//! 文件不含 DROP，对其执行恢复会在第一条「relation already exists」处中止并报
//! 失败（行为正确，但无法恢复数据）——需重新创建备份才能恢复。

// Component/PathBuf/chrono::Utc 仅 server 构建的备份逻辑用到。
#[cfg(feature = "server")]
use std::path::{Component, Path, PathBuf};

#[cfg(feature = "server")]
use chrono::Utc;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

// admin 鉴权 + AppError + tasks 进度表仅在 server 构建里被 server function 体引用。
#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::database::tasks::{self, TaskKind, TaskStatus};
#[cfg(feature = "server")]
use crate::api::error::AppError;

// 以下常量仅被 server 构建的备份/恢复逻辑引用（WASM 构建里相关函数体被 cfg 剥掉，
// 故常量也需 gate，否则非 server 构建会报 dead_code）。

/// 备份目录（项目根，与 uploads/ 平级，gitignored）。
#[cfg(feature = "server")]
const BACKUP_DIR: &str = "backups";
/// 文件名白名单正则：仅字母数字下划线点连字符（防路径穿越）。
#[cfg(feature = "server")]
const FILENAME_RE: &str = r"^[a-zA-Z0-9_.\-]+$";
/// 备份文件签名头（恢复时校验，拒绝非本系统文件）。
#[cfg(feature = "server")]
const BACKUP_SIGNATURE: &str = "-- YGGDRASIL BACKUP v1";
/// 导入单文件上限默认值（MB），env `BACKUP_IMPORT_MAX_MB` 覆盖。
#[cfg(feature = "server")]
const DEFAULT_IMPORT_MAX_MB: u64 = 512;
/// multipart 框架开销宽限（boundary/头部），加在路由 body limit 上，
/// 避免恰好等于上限的文件被框架字节误杀（流式计数只计载荷字节，才是权威）。
#[cfg(feature = "server")]
pub(crate) const MULTIPART_FRAME_SLACK: u64 = 1024 * 1024;

/// 备份文件元信息（列表展示用）。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BackupInfo {
    pub filename: String,
    pub size_bytes: u64,
    /// 备份模式：pg_dump / sql-fallback（从签名头解析）。
    pub mode: String,
    pub created_at: Option<String>,
    /// 来源：manual / auto（从文件名前缀解析）。
    pub origin: String,
    /// 配对的 uploads 打包文件名与大小（无则 None——老备份或禁用了素材打包）。
    pub uploads_filename: Option<String>,
    pub uploads_size_bytes: Option<u64>,
}

/// 备份来源：决定文件名前缀与是否参与自动轮转。
#[cfg(feature = "server")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackupOrigin {
    /// admin 面板手动触发（`backup_` 前缀，永不自动删除）。
    Manual,
    /// 定时任务触发（`auto_` 前缀，按 retention_count 轮转）。
    Auto,
}

#[cfg(feature = "server")]
impl BackupOrigin {
    fn file_prefix(self) -> &'static str {
        match self {
            Self::Manual => "backup",
            Self::Auto => "auto",
        }
    }
}

/// 一次备份运行的结果（调度任务据此落库 last_run）。
#[cfg(feature = "server")]
#[derive(Debug)]
pub(crate) struct BackupRunOutcome {
    /// 数据库 SQL 文件名。
    pub sql_filename: String,
    /// uploads 打包文件名（禁用了素材打包时为 None）。
    pub uploads_filename: Option<String>,
    /// 非致命告警（如 DB 成功但 uploads 打包失败）。
    pub warning: Option<String>,
}

/// 发起备份，立即返回 task_id，后台任务执行。
#[server(CreateBackup, "/api")]
pub async fn create_backup() -> Result<String, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let task_id = uuid::Uuid::new_v4().to_string();
        tasks::insert(task_id.clone(), TaskKind::Backup);
        let tid = task_id.clone();
        tokio::spawn(async move {
            // 手动触发：进度经任务表轮询展示，返回值无人消费。
            let _ = run_backup(&tid, BackupOrigin::Manual).await;
        });
        Ok(task_id)
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(String::new())
    }
}

/// 定时任务入口：注册任务后执行自动备份，返回结果供调度器落库 last_run。
#[cfg(feature = "server")]
pub(crate) async fn run_auto_backup() -> Result<BackupRunOutcome, String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    tasks::insert(task_id.clone(), TaskKind::Backup);
    run_backup(&task_id, BackupOrigin::Auto).await
}

/// 后台执行备份：pg_dump 优先，不可用回退纯 SQL；成功后按设置打包 uploads，
/// 自动备份再做轮转。Ok/Err 均已同步进任务进度表（供前端轮询），返回值供
/// 调度器落库 last_run。
#[cfg(feature = "server")]
async fn run_backup(task_id: &str, origin: BackupOrigin) -> Result<BackupRunOutcome, String> {
    let _ = std::fs::create_dir_all(BACKUP_DIR);
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let prefix = origin.file_prefix();

    // 读取备份设置（include_uploads / retention_count）；读取失败用默认值——
    // DB 不可达时 pg_dump 自身也会失败并按原路径上报。
    let settings = match crate::db::pool::get_conn().await {
        Ok(conn) => crate::api::settings::load_backup_settings(&conn)
            .await
            .unwrap_or_default(),
        Err(_) => crate::models::settings::BackupSettings::default(),
    };

    // 探测 pg_dump（fork+exec+wait 仍是阻塞系统调用，移出 tokio worker 线程）
    let pg_dump_ok = tokio::task::spawn_blocking(|| {
        std::process::Command::new("pg_dump")
            .arg("--version")
            .output()
            .is_ok()
    })
    .await
    .unwrap_or(false);

    let sql_result = if pg_dump_ok {
        run_pg_dump_backup(task_id, prefix, &timestamp).await
    } else {
        run_sql_fallback_backup(task_id, prefix, &timestamp).await
    };
    let sql_filename = match sql_result {
        Ok(f) => f,
        // 子流程已把具体错误写进任务进度表，这里仅透传给调度器。
        Err(e) => return Err(e),
    };

    // uploads 素材打包：失败不拖垮已成功的 DB 备份，降级为告警。
    let mut uploads_filename = None;
    let mut warning = None;
    if settings.include_uploads {
        tasks::update(
            task_id,
            "正在打包 uploads 素材",
            92,
            TaskStatus::Running,
            None,
            None,
            None,
        );
        let tar_name = uploads_archive_name(&sql_filename);
        let tar_path = backup_path(&tar_name);
        match tokio::task::spawn_blocking(move || {
            create_uploads_tarball(Path::new("uploads"), &tar_path)
        })
        .await
        {
            Ok(Ok(())) => uploads_filename = Some(tar_name),
            Ok(Err(e)) => warning = Some(format!("uploads 打包失败: {e}")),
            Err(e) => warning = Some(format!("uploads 打包任务panic: {e}")),
        }
        if warning.is_some() {
            tracing::warn!("backup uploads tarball failed: {:?}", warning);
        }
    }

    // 自动备份轮转：只删 auto_ 前缀的最旧者，手动备份永不触碰。
    if origin == BackupOrigin::Auto {
        rotate_auto_backups(settings.retention_count);
    }

    tasks::update(
        task_id,
        "完成",
        100,
        TaskStatus::Done,
        warning.clone(),
        None,
        Some(sql_filename.clone()),
    );
    Ok(BackupRunOutcome {
        sql_filename,
        uploads_filename,
        warning,
    })
}

/// SQL 文件名 → 配对 uploads 打包文件名（`X.sql` → `X_uploads.tar.gz`）。
#[cfg(feature = "server")]
fn uploads_archive_name(sql_filename: &str) -> String {
    format!("{}_uploads.tar.gz", sql_filename.trim_end_matches(".sql"))
}

/// 把 uploads 目录打成 tar.gz（排除可重建的 .cache/ 与 VCS 占位 .gitkeep）。
/// 阻塞 IO + CPU（gzip），调用方须放 spawn_blocking。
/// uploads_dir 显式传入（而非写死相对路径）便于测试注入临时目录。
#[cfg(feature = "server")]
fn create_uploads_tarball(uploads_dir: &Path, out_path: &Path) -> std::io::Result<()> {
    let file = std::fs::File::create(out_path)?;
    let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(gz);
    if uploads_dir.is_dir() {
        for entry in std::fs::read_dir(uploads_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if name == ".cache" || name == ".gitkeep" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                builder.append_dir_all(&name, &path)?;
            } else if path.is_file() {
                builder.append_path_with_name(&path, &name)?;
            }
        }
    }
    let gz = builder.into_inner()?;
    gz.finish()?;
    Ok(())
}

/// 从自动备份 SQL 文件名列表挑出超出保留份数、应删除的最旧者。
/// 文件名内嵌定宽时间戳，字典序即时间序。纯函数便于单测。
#[cfg(feature = "server")]
fn select_expired_auto_backups(names: &[String], keep: usize) -> Vec<String> {
    let mut autos: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with("auto_") && n.ends_with(".sql"))
        .collect();
    autos.sort();
    let excess = autos.len().saturating_sub(keep);
    autos.into_iter().take(excess).cloned().collect()
}

/// 执行自动备份轮转：删除超龄自动备份的 .sql 与配对 tar.gz。
/// 单个删除失败只记日志，不中断其余。
#[cfg(feature = "server")]
fn rotate_auto_backups(keep: i32) {
    let names: Vec<String> = match std::fs::read_dir(BACKUP_DIR) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect(),
        Err(e) => {
            tracing::warn!("backup rotation: cannot read {BACKUP_DIR}: {e}");
            return;
        }
    };
    for sql_name in select_expired_auto_backups(&names, keep.max(0) as usize) {
        for path in [
            backup_path(&sql_name),
            backup_path(&uploads_archive_name(&sql_name)),
        ] {
            match std::fs::remove_file(&path) {
                Ok(()) => tracing::info!("backup rotation: deleted {}", path.display()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!("backup rotation: failed to delete {}: {e}", path.display())
                }
            }
        }
    }
}

/// pg_dump 模式：子进程生成完整备份（含 schema），前置签名头。
///
/// `--clean --if-exists`：生成的脚本含 `DROP ... IF EXISTS`，使恢复幂等
/// （恢复前自动删除现有对象，避免「relation already exists」/主键冲突导致
/// 数据零写入）。详见 `run_restore`。
///
/// 成功返回 SQL 文件名（最终「完成」进度由调用方统一上报）。
#[cfg(feature = "server")]
async fn run_pg_dump_backup(
    task_id: &str,
    prefix: &str,
    timestamp: &str,
) -> Result<String, String> {
    tasks::update(
        task_id,
        "正在用 pg_dump 导出",
        10,
        TaskStatus::Running,
        None,
        None,
        None,
    );
    let filename = format!("{prefix}_{timestamp}.sql");
    let path = backup_path(&filename);
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            let msg = "pg_dump 备份需要 DATABASE_URL".to_string();
            tasks::update(
                task_id,
                "DATABASE_URL 未配置",
                100,
                TaskStatus::Failed,
                None,
                Some(msg.clone()),
                None,
            );
            return Err(msg);
        }
    };

    let mut header = String::new();
    header.push_str(&format!("{}\n", BACKUP_SIGNATURE));
    header.push_str(&format!("-- created_at: {}\n", Utc::now()));
    header.push_str("-- mode: pg_dump\n");

    // 先写签名头，再追加 pg_dump 输出。
    if let Err(e) = std::fs::write(&path, &header) {
        let msg = format!("无法写入备份目录: {e}");
        tasks::update(
            task_id,
            "写入备份文件失败",
            100,
            TaskStatus::Failed,
            None,
            Some(msg.clone()),
            None,
        );
        return Err(msg);
    }

    let stdout_file = match std::fs::OpenOptions::new().append(true).open(&path) {
        Ok(f) => f,
        Err(e) => {
            let msg = e.to_string();
            tasks::update(
                task_id,
                "pg_dump 启动失败",
                100,
                TaskStatus::Failed,
                None,
                Some(msg.clone()),
                None,
            );
            return Err(msg);
        }
    };
    // pg_dump 导出可持续数十秒到数分钟，整个子进程生命周期（spawn + wait_with_output）
    // 移入 spawn_blocking，避免阻塞 tokio worker 线程。注意 stdout 重定向到备份文件，
    // 故不能用 .output()（它会用 piped 覆盖 stdout 配置）；保留 spawn() + wait_with_output()
    // 两段式。闭包返回 Result<Output, (bool, io::Error)>：true=启动(spawn)阶段失败，
    // false=等待(wait)阶段失败，分别对应原有「启动失败」「执行失败」两条上报路径；
    // 闭包 panic（JoinError）按「执行失败」处理。
    let dump_result = tokio::task::spawn_blocking(
        move || -> Result<std::process::Output, (bool, std::io::Error)> {
            std::process::Command::new("pg_dump")
                .arg(db_url)
                // --clean --if-exists：生成 DROP ... IF EXISTS，让恢复幂等（先删后建），
                // 否则恢复时表已存在 → CREATE/COPY 全部失败、数据零写入。
                .arg("--clean")
                .arg("--if-exists")
                .stdout(std::process::Stdio::from(stdout_file))
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| (true, e))?
                .wait_with_output()
                .map_err(|e| (false, e))
        },
    )
    .await
    .unwrap_or_else(|join_e| Err((false, std::io::Error::other(join_e.to_string()))));
    match dump_result {
        Ok(o) if o.status.success() => Ok(filename),
        Ok(o) => {
            let msg = String::from_utf8_lossy(&o.stderr).to_string();
            tasks::update(
                task_id,
                "pg_dump 失败",
                100,
                TaskStatus::Failed,
                None,
                Some(msg.clone()),
                None,
            );
            Err(msg)
        }
        Err((true, e)) => {
            let msg = e.to_string();
            tasks::update(
                task_id,
                "pg_dump 启动失败",
                100,
                TaskStatus::Failed,
                None,
                Some(msg.clone()),
                None,
            );
            Err(msg)
        }
        Err((false, e)) => {
            let msg = e.to_string();
            tasks::update(
                task_id,
                "pg_dump 执行失败",
                100,
                TaskStatus::Failed,
                None,
                Some(msg.clone()),
                None,
            );
            Err(msg)
        }
    }
}

/// 纯 SQL 回退：仅备份数据（不含 schema），按表计数精确进度。
///
/// 注意：该产物不是合法 SQL 脚本（COPY TO STDOUT 的 CSV 原文拼接），
/// 不能经 psql 恢复——仅供极端环境下抢救数据。生产镜像自带 pg_dump，
/// 正常不会走到这里。
///
/// 成功返回 SQL 文件名（最终「完成」进度由调用方统一上报）。
#[cfg(feature = "server")]
async fn run_sql_fallback_backup(
    task_id: &str,
    prefix: &str,
    timestamp: &str,
) -> Result<String, String> {
    tasks::update(
        task_id,
        "pg_dump 不可用，使用纯 SQL 回退（仅数据）",
        10,
        TaskStatus::Running,
        Some("仅备份数据，不含 schema/索引/触发器，且不可经 psql 恢复".to_string()),
        None,
        None,
    );
    let filename = format!("{prefix}_{timestamp}_sqlfallback.sql");
    let path = backup_path(&filename);

    let client = match crate::db::pool::get_conn().await {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            tasks::update(
                task_id,
                "数据库连接失败",
                100,
                TaskStatus::Failed,
                None,
                Some(msg.clone()),
                None,
            );
            return Err(msg);
        }
    };

    // 取 public schema 下所有表名
    let tables: Vec<String> = match client
        .query(
            "SELECT tablename FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename",
            &[],
        )
        .await
    {
        Ok(rows) => rows.into_iter().map(|r| r.get(0)).collect(),
        Err(e) => {
            let msg = e.to_string();
            tasks::update(
                task_id,
                "读取表清单失败",
                100,
                TaskStatus::Failed,
                None,
                Some(msg.clone()),
                None,
            );
            return Err(msg);
        }
    };
    let total = tables.len().max(1);

    let mut out = String::new();
    out.push_str(&format!("{}\n", BACKUP_SIGNATURE));
    out.push_str(&format!("-- created_at: {}\n", Utc::now()));
    out.push_str("-- mode: sql-fallback\n\n");

    for (i, table) in tables.iter().enumerate() {
        out.push_str(&format!("\n-- table: {}\n", table));
        let copy_stmt = format!("COPY \"{}\" TO STDOUT WITH CSV", table);
        match client.copy_out(&copy_stmt).await {
            Ok(stream) => {
                use futures::StreamExt;
                // CopyOutStream 是 !Unpin，必须 pin 才能调 next。
                tokio::pin!(stream);
                while let Some(chunk) = stream.next().await {
                    if let Ok(bytes) = chunk {
                        out.push_str(&String::from_utf8_lossy(&bytes));
                    }
                }
            }
            Err(e) => {
                out.push_str(&format!("-- 导出失败: {}\n", e));
            }
        }
        // 按表更新进度（用 u32 避免大 schema 下的截断/溢出）
        tasks::update(
            task_id,
            &format!("导出表 {}/{}", i + 1, total),
            (10 + (i + 1) as u32 * 90 / total as u32).min(99) as u8,
            TaskStatus::Running,
            None,
            None,
            None,
        );
    }

    if let Err(e) = std::fs::write(&path, out) {
        let msg = format!("无法写入备份目录: {e}");
        tasks::update(
            task_id,
            "写入备份文件失败",
            100,
            TaskStatus::Failed,
            None,
            Some(msg.clone()),
            None,
        );
        return Err(msg);
    }
    Ok(filename)
}

/// 发起恢复：校验签名 + 路径穿越防护 + 二次确认，立即返回 task_id。
#[server(RestoreBackup, "/api")]
pub async fn restore_backup(filename: String, confirm: bool) -> Result<String, ServerFnError> {
    let _user = get_current_admin_user().await?;

    // 全部校验都在 server cfg 块内：confirm/regex/backup_path/std::fs 都是 server-only。
    // WASM 侧的 server-function 客户端桩只返回 Ok(String::new())。
    #[cfg(feature = "server")]
    {
        if !confirm {
            return Err(AppError::BadRequest("需确认恢复（会覆盖现有数据）".to_string()).into());
        }
        // 路径穿越防护
        if !is_valid_backup_filename(&filename) {
            return Err(AppError::BadRequest("无效的文件名".to_string()).into());
        }
        let path = backup_path(&filename);
        if !path.exists() {
            return Err(AppError::NotFound("备份文件不存在").into());
        }

        // 签名校验：仅读取首行（备份文件可达数十 MB，无需整文件读入内存）。
        let first_line = read_first_line(&path).unwrap_or_default();
        if !has_valid_signature(&first_line) {
            return Err(
                AppError::BadRequest("非本系统生成的备份文件，拒绝恢复".to_string()).into(),
            );
        }

        let task_id = uuid::Uuid::new_v4().to_string();
        tasks::insert(task_id.clone(), TaskKind::Restore);
        let tid = task_id.clone();
        let f = filename;
        tokio::spawn(async move {
            run_restore(&tid, &f).await;
        });
        Ok(task_id)
    }
    #[cfg(not(feature = "server"))]
    {
        // WASM 客户端桩：忽略参数，返回空 task_id。
        let _ = (filename, confirm);
        Ok(String::new())
    }
}

/// 后台执行恢复：探测 psql，可用则 psql -f，不可用则报告。
///
/// **幂等恢复**：备份由 `pg_dump --clean --if-exists` 生成，脚本自带
/// `DROP ... IF EXISTS`，恢复时会先删后建，数据完全回到备份时刻。
///
/// **错误中止**：`-v ON_ERROR_STOP=1` 让 psql 在第一条 SQL 错误时立即退出
/// （退出码 3）。否则 psql 即使满屏 ERROR 也返回 0，导致 `status.success()`
/// 误判为成功——这是「恢复完成却无任何数据变更」假成功的根因。
///
/// **恢复成功后失效全量缓存**：恢复会用备份时刻的数据重建 posts 等表，
/// 现有 moka 缓存（列表/标签/单篇/统计/搜索）与 SSR 世代号必须一并冲刷，
/// 否则前端仍读旧数据。
#[cfg(feature = "server")]
async fn run_restore(task_id: &str, filename: &str) {
    let path = backup_path(filename);
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            tasks::update(
                task_id,
                "DATABASE_URL 未配置",
                100,
                TaskStatus::Failed,
                None,
                Some("恢复需要 DATABASE_URL".to_string()),
                None,
            );
            return;
        }
    };
    let psql_ok = tokio::task::spawn_blocking(|| {
        std::process::Command::new("psql")
            .arg("--version")
            .output()
            .is_ok()
    })
    .await
    .unwrap_or(false);
    if !psql_ok {
        tasks::update(
            task_id,
            "psql 不可用",
            100,
            TaskStatus::Failed,
            None,
            Some("恢复需要 psql，但当前环境未安装 psql".to_string()),
            None,
        );
        return;
    }
    tasks::update(
        task_id,
        "正在用 psql 恢复",
        50,
        TaskStatus::Running,
        None,
        None,
        None,
    );
    // psql 恢复可持续数十秒到数分钟，整段 .output() 移入 spawn_blocking，避免阻塞
    // tokio worker 线程。db_url/path 按值移入闭包（闭包外不再使用）；闭包 panic
    // （JoinError）按「启动失败」上报。
    let restore_result = tokio::task::spawn_blocking(move || {
        std::process::Command::new("psql")
            .arg(db_url)
            // ON_ERROR_STOP=1：遇 SQL 错误立即中止（退出码 3）。
            // 不加这个，psql 即使满屏 ERROR 也返回 0，status.success() 误报成功。
            .arg("-v")
            .arg("ON_ERROR_STOP=1")
            .arg("-f")
            .arg(path)
            .stderr(std::process::Stdio::piped())
            .output()
    })
    .await
    .unwrap_or_else(|join_e| Err(std::io::Error::other(join_e.to_string())));
    match restore_result {
        Ok(o) if o.status.success() => {
            // 恢复用备份时刻的数据重建了 posts 等表，必须冲刷全部文章相关缓存
            // 与 SSR 世代号，否则前端仍读旧数据（被删的文章不会重新出现）。
            crate::cache::invalidate_all_post_caches();
            crate::cache::invalidate_search_results();
            crate::ssr_cache::bump_global_generation();
            tasks::update(task_id, "恢复完成", 100, TaskStatus::Done, None, None, None);
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            tasks::update(
                task_id,
                "恢复失败",
                100,
                TaskStatus::Failed,
                None,
                Some(stderr),
                None,
            );
        }
        Err(e) => {
            tasks::update(
                task_id,
                "psql 启动失败",
                100,
                TaskStatus::Failed,
                None,
                Some(e.to_string()),
                None,
            );
        }
    }
}

/// 列出 backups/ 目录下的备份文件元信息。
///
/// uploads 打包（`*_uploads.tar.gz`）不单独成行，按文件名配对挂到对应 .sql 上。
#[server(ListBackups, "/api")]
pub async fn list_backups() -> Result<Vec<BackupInfo>, ServerFnError> {
    let _user = get_current_admin_user().await?;
    #[cfg(feature = "server")]
    {
        let mut infos: Vec<BackupInfo> = Vec::new();
        // 先收集 uploads 包：文件名 → 大小，供配对查询。
        let mut tarballs: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        if let Ok(entries) = std::fs::read_dir(BACKUP_DIR) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with("_uploads.tar.gz") {
                    if let Ok(meta) = entry.metadata() {
                        tarballs.insert(name, meta.len());
                    }
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(BACKUP_DIR) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.ends_with(".sql") {
                    continue;
                }
                let meta = match entry.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                // 仅读取前 3 行（签名/created_at/mode），避免把每个（可能数十 MB
                // 的）备份文件整文件读入内存只为取 `-- mode:` 行。
                let mode = read_first_lines(entry.path(), 3)
                    .map(|lines| parse_backup_mode(&lines.join("\n")))
                    .unwrap_or_else(|_| "unknown".to_string());
                let created_at = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| {
                        chrono::DateTime::<Utc>::from_timestamp(d.as_secs() as i64, 0)
                            .map(|dt| dt.to_rfc3339())
                            .unwrap_or_default()
                    });
                let tar_name = uploads_archive_name(&name);
                let (uploads_filename, uploads_size_bytes) = tarballs
                    .get_key_value(&tar_name)
                    .map(|(k, v)| (Some(k.clone()), Some(*v)))
                    .unwrap_or((None, None));
                infos.push(BackupInfo {
                    origin: if name.starts_with("auto_") {
                        "auto"
                    } else {
                        "manual"
                    }
                    .to_string(),
                    filename: name,
                    size_bytes: meta.len(),
                    mode,
                    created_at,
                    uploads_filename,
                    uploads_size_bytes,
                });
            }
        }
        // 按创建时间降序（新的在前）
        infos.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(infos)
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(vec![])
    }
}

/// 删除备份文件（连同配对的 uploads 打包；配对缺失不视为错误）。
#[server(DeleteBackup, "/api")]
pub async fn delete_backup(filename: String) -> Result<(), ServerFnError> {
    let _user = get_current_admin_user().await?;
    #[cfg(feature = "server")]
    {
        if !is_valid_backup_filename(&filename) {
            return Err(AppError::BadRequest("无效的文件名".to_string()).into());
        }
        let path = backup_path(&filename);
        if !path.exists() {
            return Err(AppError::NotFound("备份文件不存在").into());
        }
        std::fs::remove_file(&path).map_err(|_| AppError::Internal("删除失败"))?;
        // 成对清理：传 .sql 删配对 tar.gz，传 tar.gz 删配对 .sql。
        let pair = if filename.ends_with(".sql") {
            Some(uploads_archive_name(&filename))
        } else {
            filename
                .strip_suffix("_uploads.tar.gz")
                .map(|stem| format!("{stem}.sql"))
        };
        if let Some(pair_name) = pair {
            match std::fs::remove_file(backup_path(&pair_name)) {
                Ok(()) | Err(_) => {} // 配对缺失/删除失败不影响主文件删除结果
            }
        }
        Ok(())
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(())
    }
}

/// 构造 backups/ 下的安全路径（额外防御：校验规范化后仍在 BACKUP_DIR 内）。
///
/// 纵深防御：即便第一道白名单 `is_valid_backup_filename` 被绕过，这里也要
/// 保证结果不逃出 BACKUP_DIR。直接对 filename 做 components 检查——
/// 含 `..`（ParentDir）、绝对路径前缀（RootDir/Prefix，如 `/etc` 或 `C:\`）
/// 的 filename 一律降级为 BACKUP_DIR 本身。
///
/// 注意：不能用 `[BACKUP_DIR, filename].collect::<PathBuf>()` 后再检——
/// 当 filename 是绝对路径时，PathBuf 语义会丢弃 BACKUP_DIR 前缀（如
/// `["backups", "/etc/passwd"]` → `/etc/passwd`），导致 components 检查
/// 在错位的路径上运行而漏判。必须先检 filename 本身。
#[cfg(feature = "server")]
fn backup_path(filename: &str) -> PathBuf {
    // 直接检查 filename 的 components：只允许 Normal 段。
    let filename_is_safe = std::path::Path::new(filename)
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    if filename_is_safe {
        let mut p = PathBuf::from(BACKUP_DIR);
        p.push(filename);
        p
    } else {
        // 命中 ParentDir/RootDir/Prefix/CurDir → 降级为 BACKUP_DIR
        PathBuf::from(BACKUP_DIR)
    }
}

/// 校验备份文件名是否符合白名单（仅字母数字下划线点连字符）。
/// 返回 true 表示安全可用。提取为纯函数便于单测覆盖路径穿越边界。
#[cfg(feature = "server")]
fn is_valid_backup_filename(filename: &str) -> bool {
    // regex::Regex::new 在 FILENAME_RE 是常量正则,编译期可验证不会 panic。
    regex::Regex::new(FILENAME_RE)
        .map(|re| re.is_match(filename))
        .unwrap_or(false)
}

/// 导入单文件上限（字节）：env `BACKUP_IMPORT_MAX_MB`（MB 为单位），默认 512MB。
/// main.rs 路由 DefaultBodyLimit 与 handler 流式计数共用此值。
#[cfg(feature = "server")]
pub(crate) fn import_max_bytes() -> u64 {
    std::env::var("BACKUP_IMPORT_MAX_MB")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|mb| *mb > 0)
        .unwrap_or(DEFAULT_IMPORT_MAX_MB)
        .saturating_mul(1024 * 1024)
}

/// 校验并清洗导入文件名：剥离路径分量（旧浏览器可能带 `C:\fakepath\` 前缀），
/// 要求 `.sql` 后缀 + 非隐藏文件 + 备份文件名白名单。纯函数便于单测。
#[cfg(feature = "server")]
fn sanitize_import_filename(raw: &str) -> Option<String> {
    let name = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    if name.len() > 255
        || !name.ends_with(".sql")
        || name.starts_with('.')
        || !is_valid_backup_filename(name)
    {
        return None;
    }
    Some(name.to_string())
}

/// 查询 backups/ 所在文件系统的可用空间（取最长挂载点前缀的磁盘）。
/// 预检用途：探测失败返回 None，降级为跳过预检（写入失败仍会清理并报错）。
#[cfg(feature = "server")]
fn backup_partition_free_space() -> Option<u64> {
    let dir = std::fs::canonicalize(BACKUP_DIR).ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|d| dir.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

/// 从备份文件全文提取 `-- mode: <value>` 行的值（如 "pg_dump"/"sql-fallback"）。
/// 提取为纯函数:把文件内容作为参数传入,便于单测。
/// 缺失或格式不符返回 "unknown"。
#[cfg(feature = "server")]
fn parse_backup_mode(content: &str) -> String {
    content
        .lines()
        .find(|l| l.starts_with("-- mode:"))
        .map(|l| l.trim_start_matches("-- mode:").trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// 校验备份文件首行是否含本系统签名头。
/// 提取为纯函数:把首行(或全文)作为参数传入,便于单测。
#[cfg(feature = "server")]
fn has_valid_signature(content: &str) -> bool {
    content
        .lines()
        .next()
        .map(|l| l.trim().contains(BACKUP_SIGNATURE))
        .unwrap_or(false)
}

/// 仅读取文件首行（用于签名校验，避免把整个备份文件读入内存）。
#[cfg(feature = "server")]
fn read_first_line(path: impl AsRef<Path>) -> std::io::Result<String> {
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line)
}

/// 读取文件前 `n` 行（用于解析 `-- mode:` 等头部元信息，避免整文件读入内存）。
/// 不足 `n` 行时返回实际读到的行；任一行读取失败则整体返回该错误。
#[cfg(feature = "server")]
fn read_first_lines(path: impl AsRef<Path>, n: usize) -> std::io::Result<Vec<String>> {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(std::fs::File::open(path)?);
    reader.lines().take(n).collect()
}

/// Axum 处理器：导入备份（multipart 流式落盘，admin 鉴权 + 签名校验 + 同名拒绝）。
///
/// 设计要点（见 `docs/adr/0001-backup-import.md`）：
/// - 两步式：导入只入库 backups/ 并出现在列表，恢复仍走 [`restore_backup`] 管线；
/// - 导入时即校验签名（拒绝外来 SQL 入库），与恢复时的签名校验是两层独立防线；
/// - 流式写 `.tmp` + 原子 rename：半截文件永不入库，任一失败路径清理 tmp；
/// - 体积双保险：路由 `DefaultBodyLimit`（上限 + [`MULTIPART_FRAME_SLACK`]）
///   兜整个请求体，此处流式计数只计载荷字节，超限 413 早断；
/// - 仅 server 构建：纯 Axum 路由（在 main.rs 注册），无 WASM 消费者。
#[cfg(feature = "server")]
pub async fn import_backup(
    connect_info: Option<
        axum::extract::Extension<axum::extract::ConnectInfo<std::net::SocketAddr>>,
    >,
    headers: axum::http::HeaderMap,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, axum::Json<serde_json::Value>)>
{
    use crate::api::upload::upload_error;
    use axum::http::StatusCode;
    use tokio::io::AsyncWriteExt;

    // 0. 限流：复用 UPLOAD 桶（同为 admin 文件上传，不为低频导入单开配置面）。
    let peer = connect_info.map(|axum::extract::Extension(axum::extract::ConnectInfo(addr))| addr);
    let ip = crate::api::rate_limit::get_client_ip_with_peer(&headers, peer).await;
    if let Err(msg) = crate::api::rate_limit::check_upload_limit(&ip) {
        return Err(upload_error(StatusCode::TOO_MANY_REQUESTS, msg));
    }

    // 1. cookie session → admin（与 upload_image 同一校验链）。
    let cookie_header = headers
        .get("cookie")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let token = match crate::auth::session::parse_session_token(cookie_header) {
        Some(t) => t,
        None => return Err(upload_error(StatusCode::UNAUTHORIZED, "未登录")),
    };
    let user = match crate::api::auth::get_user_by_token(token).await {
        Ok(Some(u)) => u,
        _ => return Err(upload_error(StatusCode::UNAUTHORIZED, "会话已过期")),
    };
    if user.role != crate::models::user::UserRole::Admin {
        return Err(upload_error(StatusCode::FORBIDDEN, "权限不足"));
    }

    let max_bytes = import_max_bytes();

    // 2. Content-Length 早拒：明显超限不读 body 直接 413。
    let content_length = headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    if let Some(cl) = content_length {
        if cl > max_bytes + MULTIPART_FRAME_SLACK {
            return Err(upload_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "文件超过导入上限",
            ));
        }
    }

    // 3. 取 multipart 字段并清洗文件名。
    let mut field = match multipart.next_field().await {
        Ok(Some(f)) => f,
        Ok(None) => return Err(upload_error(StatusCode::BAD_REQUEST, "未找到文件")),
        Err(e) => {
            tracing::error!("backup import multipart error: {e:?}");
            return Err(upload_error(StatusCode::BAD_REQUEST, "文件读取失败"));
        }
    };
    let filename = match sanitize_import_filename(field.file_name().unwrap_or_default()) {
        Some(n) => n,
        None => {
            return Err(upload_error(
                StatusCode::BAD_REQUEST,
                "文件名不合法：仅接受以 .sql 结尾的备份文件名（字母/数字/下划线/点/连字符）",
            ))
        }
    };

    // 4. 同名冲突拒绝（不覆盖——覆盖是破坏性操作；用户可先删除旧文件再导入）。
    if let Err(e) = std::fs::create_dir_all(BACKUP_DIR) {
        tracing::error!("backup import: create dir failed: {e}");
        return Err(upload_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "无法创建备份目录",
        ));
    }
    let final_path = backup_path(&filename);
    if final_path.exists() {
        return Err(upload_error(StatusCode::CONFLICT, "已存在同名备份文件"));
    }

    // 5. 磁盘空间预检（Content-Length 已知且探测成功时；否则降级跳过）。
    if let (Some(cl), Some(free)) = (content_length, backup_partition_free_space()) {
        if cl > free {
            return Err(upload_error(
                StatusCode::INSUFFICIENT_STORAGE,
                "磁盘空间不足",
            ));
        }
    }

    // 6. 流式落盘 tmp：按载荷字节精确计数，超限/写失败/读失败都清理半截文件。
    let tmp_name = format!(
        ".import-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let tmp_path = backup_path(&tmp_name);
    let mut out = match tokio::fs::File::create(&tmp_path).await {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("backup import: create tmp failed: {e}");
            return Err(upload_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "无法写入备份目录",
            ));
        }
    };
    let mut written: u64 = 0;
    let stream_result: Result<(), (StatusCode, &'static str)> = loop {
        match field.chunk().await {
            Ok(Some(chunk)) => {
                written += chunk.len() as u64;
                if written > max_bytes {
                    break Err((StatusCode::PAYLOAD_TOO_LARGE, "文件超过导入上限"));
                }
                if let Err(e) = out.write_all(&chunk).await {
                    tracing::error!("backup import: write failed: {e}");
                    break Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "写入失败（磁盘可能已满）",
                    ));
                }
            }
            Ok(None) => break Ok(()),
            Err(e) => {
                tracing::error!("backup import: chunk error: {e:?}");
                break Err((StatusCode::BAD_REQUEST, "文件读取失败"));
            }
        }
    };
    drop(out);
    if let Err((status, msg)) = stream_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(upload_error(status, msg));
    }

    // 7. 导入时即签名校验：非本系统文件拒收且不留盘。
    let first_line = read_first_line(&tmp_path).unwrap_or_default();
    if !has_valid_signature(&first_line) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(upload_error(
            StatusCode::BAD_REQUEST,
            "非本系统生成的备份文件，拒绝导入",
        ));
    }

    // 8. 原子 rename 入库；rename 前二次冲突检查，收窄两个并发同名导入的竞态窗。
    if final_path.exists() {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(upload_error(StatusCode::CONFLICT, "已存在同名备份文件"));
    }
    if let Err(e) = std::fs::rename(&tmp_path, &final_path) {
        let _ = std::fs::remove_file(&tmp_path);
        tracing::error!("backup import: rename failed: {e}");
        return Err(upload_error(StatusCode::INTERNAL_SERVER_ERROR, "入库失败"));
    }

    tracing::info!(
        operator = %user.username,
        filename = %filename,
        size_bytes = written,
        "备份导入成功"
    );
    Ok(axum::Json(
        serde_json::json!({ "success": true, "filename": filename }),
    ))
}

/// Axum 处理器：下载备份文件（admin 鉴权 + 路径白名单）。
/// 仅 server 构建：纯 Axum 路由（在 main.rs 注册），无 WASM 消费者。
#[cfg(feature = "server")]
pub async fn download_backup(
    axum::extract::Path(filename): axum::extract::Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<impl axum::response::IntoResponse, (axum::http::StatusCode, String)> {
    use axum::http::{header, StatusCode};

    // 鉴权
    let cookie_header = headers
        .get("cookie")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let token = crate::auth::session::parse_session_token(cookie_header).map(str::to_string);
    let token = match token {
        Some(t) => t,
        None => return Err((StatusCode::UNAUTHORIZED, "未登录".to_string())),
    };
    let user = match crate::api::auth::get_user_by_token(&token).await {
        Ok(Some(u)) => u,
        _ => return Err((StatusCode::UNAUTHORIZED, "会话已过期".to_string())),
    };
    if user.role != crate::models::user::UserRole::Admin {
        return Err((StatusCode::FORBIDDEN, "权限不足".to_string()));
    }

    // 路径白名单
    if !is_valid_backup_filename(&filename) {
        return Err((StatusCode::BAD_REQUEST, "无效的文件名".to_string()));
    }
    let path = backup_path(&filename);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "文件不存在".to_string()))?;
    let disposition = format!("attachment; filename=\"{}\"", filename);
    let content_type = if filename.ends_with(".tar.gz") {
        "application/gzip"
    } else {
        "application/sql; charset=utf-8"
    };
    Ok((
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static(content_type),
            ),
            (
                header::CONTENT_DISPOSITION,
                axum::http::HeaderValue::from_str(&disposition)
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("attachment")),
            ),
        ],
        axum::body::Body::from(bytes),
    ))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    // ── is_valid_backup_filename:文件名白名单(路径穿越第一道防线) ──

    #[test]
    fn filename_accepts_normal_names() {
        for name in [
            "backup_20260702_120000.sql",
            "backup_20260702_120000_sqlfallback.sql",
            "a.sql",
            "A-B_C.123",
        ] {
            assert!(is_valid_backup_filename(name), "正常文件名应通过: {name}");
        }
    }

    #[test]
    fn filename_rejects_path_traversal() {
        // 路径穿越:白名单只允许字母数字下划线点连字符,/ 和 .. 都应被拒。
        for evil in [
            "../etc/passwd",
            "..\\windows\\win.ini",
            "/etc/passwd",
            "a/../../b",
            "backup.sql/../../etc",
        ] {
            assert!(!is_valid_backup_filename(evil), "路径穿越应被拒: {evil}");
        }
    }

    #[test]
    fn filename_rejects_spaces_and_special_chars() {
        // 空格、中文、shell 元字符等都不在白名单。
        for evil in [
            "backup with space.sql",
            "备份.sql",
            "a;rm -rf.sql",
            r"a\$b.sql",
            "a`b`.sql",
            "",
        ] {
            assert!(!is_valid_backup_filename(evil), "特殊字符应被拒: {evil:?}");
        }
    }

    // ── backup_path:路径穿越纵深防御(白名单之外的二次防御) ────────

    #[test]
    fn backup_path_stays_in_backup_dir_for_normal_name() {
        let p = backup_path("backup_20260702.sql");
        assert!(p.starts_with(BACKUP_DIR), "应在 {BACKUP_DIR}/ 下");
        assert_eq!(
            p.file_name().and_then(|n| n.to_str()),
            Some("backup_20260702.sql")
        );
    }

    #[test]
    fn backup_path_collapses_traversal_to_backup_dir() {
        // 即便绕过白名单调用 backup_path(纵深防御),../ 也应被规约回 BACKUP_DIR,
        // 而非指向 backups/ 之外。Component::ParentDir / RootDir 命中即降级。
        for evil in ["../etc/passwd", "../../etc/shadow"] {
            let p = backup_path(evil);
            // 不应逃出 BACKUP_DIR(应为 BACKUP_DIR 本身,不含文件名)
            assert_eq!(
                p,
                PathBuf::from(BACKUP_DIR),
                "穿越应被规约回 {BACKUP_DIR}: {evil}"
            );
        }
    }

    #[test]
    fn backup_path_rejects_absolute_path() {
        // Component::RootDir 命中也应降级。
        let p = backup_path("/etc/passwd");
        assert_eq!(p, PathBuf::from(BACKUP_DIR));
    }

    // ── has_valid_signature:备份签名校验(拒绝非本系统文件) ───────

    #[test]
    fn signature_matches_exact_header() {
        let content = "-- YGGDRASIL BACKUP v1\n-- mode: pg_dump\nSELECT 1;\n";
        assert!(has_valid_signature(content));
    }

    #[test]
    fn signature_matches_with_leading_whitespace() {
        // 首行允许前导空白(trim 后匹配),容忍编辑器缩进。
        let content = "  -- YGGDRASIL BACKUP v1\nrest\n";
        assert!(has_valid_signature(content));
    }

    #[test]
    fn signature_rejects_non_system_file() {
        // 普通 SQL 文件首行不含签名 → 拒绝恢复(防任意文件读取/执行)。
        let content = "SELECT * FROM users;\n-- YGGDRASIL BACKUP v1\n";
        // 注意:签名必须在首行。第二行有签名不算。
        assert!(!has_valid_signature(content));
    }

    #[test]
    fn signature_rejects_empty_and_garbage() {
        assert!(!has_valid_signature(""));
        assert!(!has_valid_signature("garbage\n"));
        assert!(!has_valid_signature("\n\n-- YGGDRASIL BACKUP v1"));
    }

    // ── parse_backup_mode:模式解析(列表展示用) ───────────────────

    #[test]
    fn parse_mode_pg_dump() {
        let content = "-- YGGDRASIL BACKUP v1\n-- mode: pg_dump\n...\n";
        assert_eq!(parse_backup_mode(content), "pg_dump");
    }

    #[test]
    fn parse_mode_sql_fallback() {
        let content = "-- YGGDRASIL BACKUP v1\n-- mode: sql-fallback\n\n-- table: posts\n";
        assert_eq!(parse_backup_mode(content), "sql-fallback");
    }

    #[test]
    fn parse_mode_unknown_when_absent() {
        let content = "-- YGGDRASIL BACKUP v1\nSELECT 1;\n";
        assert_eq!(parse_backup_mode(content), "unknown");
    }

    #[test]
    fn parse_mode_unknown_when_empty_value() {
        // "-- mode:" 后无值 → unknown(防空字符串显示)
        let content = "-- mode:\nrest\n";
        assert_eq!(parse_backup_mode(content), "unknown");
    }

    #[test]
    fn parse_mode_only_matches_first_occurrence() {
        // 多个 -- mode: 行取第一个。
        let content = "-- mode: pg_dump\n-- mode: sql-fallback\n";
        assert_eq!(parse_backup_mode(content), "pg_dump");
    }

    // ── uploads_archive_name:SQL ↔ uploads 包配对规则 ─────────────

    #[test]
    fn uploads_archive_name_pairs_with_sql() {
        assert_eq!(
            uploads_archive_name("backup_20260810_040000.sql"),
            "backup_20260810_040000_uploads.tar.gz"
        );
        assert_eq!(
            uploads_archive_name("auto_20260810_040000_sqlfallback.sql"),
            "auto_20260810_040000_sqlfallback_uploads.tar.gz"
        );
    }

    // ── select_expired_auto_backups:轮转选择（只动 auto，保最新 N 份） ──

    #[test]
    fn rotation_keeps_newest_and_deletes_oldest() {
        let names: Vec<String> = [
            "auto_20260808_040000.sql",
            "auto_20260810_040000.sql",
            "auto_20260809_040000.sql",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        // 文件名时间戳定宽，乱序输入也应按时间排序。
        assert_eq!(
            select_expired_auto_backups(&names, 2),
            vec!["auto_20260808_040000.sql".to_string()]
        );
    }

    #[test]
    fn rotation_ignores_manual_backups_and_tarballs() {
        let names: Vec<String> = [
            "backup_20260101_000000.sql",          // 手动：永不轮转
            "auto_20260808_040000_uploads.tar.gz", // tar.gz：随配对 sql 删除，不单独计数
            "auto_20260808_040000.sql",
            "auto_20260809_040000.sql",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert!(select_expired_auto_backups(&names, 2).is_empty());
        assert_eq!(
            select_expired_auto_backups(&names, 1),
            vec!["auto_20260808_040000.sql".to_string()]
        );
    }

    #[test]
    fn rotation_empty_and_exact_keep() {
        assert!(select_expired_auto_backups(&[], 5).is_empty());
        let names: Vec<String> = ["auto_20260808_040000.sql"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(select_expired_auto_backups(&names, 1).is_empty());
        // keep=0 全删
        assert_eq!(select_expired_auto_backups(&names, 0).len(), 1);
    }

    // ── create_uploads_tarball:打包排除 .cache/.gitkeep ───────────

    #[test]
    fn tarball_excludes_cache_and_gitkeep() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("系统时间必然晚于 UNIX_EPOCH")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "yggdrasil_backup_tar_test_{}_{}",
            nanos,
            std::process::id()
        ));
        let uploads = dir.join("uploads");
        std::fs::create_dir_all(uploads.join("2026")).expect("创建测试目录");
        std::fs::create_dir_all(uploads.join(".cache")).expect("创建测试目录");
        std::fs::write(uploads.join("2026/pic.webp"), b"img").expect("写测试文件");
        std::fs::write(uploads.join(".cache/x.webp"), b"cache").expect("写测试文件");
        std::fs::write(uploads.join(".gitkeep"), b"").expect("写测试文件");

        let out = dir.join("out.tar.gz");
        create_uploads_tarball(&uploads, &out).expect("打包应成功");

        let file = std::fs::File::open(&out).expect("打开打包产物");
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
        let entries: Vec<String> = archive
            .entries()
            .expect("读取 tar 条目")
            .map(|e| {
                e.expect("tar 条目有效")
                    .path()
                    .expect("路径有效")
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert!(
            entries.iter().any(|p| p.contains("2026/pic.webp")),
            "应包含素材文件: {entries:?}"
        );
        assert!(
            !entries
                .iter()
                .any(|p| p.contains(".cache") || p.contains(".gitkeep")),
            "应排除 .cache 与 .gitkeep: {entries:?}"
        );
        std::fs::remove_dir_all(&dir).expect("清理测试目录");
    }

    // ── sanitize_import_filename:导入文件名校验 ─────────────────

    #[test]
    fn import_filename_accepts_plain_backup_name() {
        assert_eq!(
            sanitize_import_filename("backup_20260816_200000.sql"),
            Some("backup_20260816_200000.sql".to_string())
        );
        // 255 字节边界接受
        let max_ok = format!("{}.sql", "a".repeat(251));
        assert!(sanitize_import_filename(&max_ok).is_some());
    }

    #[test]
    fn import_filename_strips_path_components() {
        // 旧浏览器可能送 `C:\fakepath\` 前缀；POSIX 路径同样剥掉，只留 basename。
        assert_eq!(
            sanitize_import_filename("C:\\fakepath\\auto_20260816_200000.sql"),
            Some("auto_20260816_200000.sql".to_string())
        );
        assert_eq!(
            sanitize_import_filename("/tmp/x/backup_1.sql"),
            Some("backup_1.sql".to_string())
        );
    }

    #[test]
    fn import_filename_rejects_non_sql_and_hidden() {
        // 配对 tar.gz 不可经导入入口塞进 backups/
        assert_eq!(sanitize_import_filename("x_uploads.tar.gz"), None);
        assert_eq!(sanitize_import_filename("noext"), None);
        // 隐藏文件与裸 ".sql" 拒绝（避开 tmp 文件命名空间）
        assert_eq!(sanitize_import_filename(".hidden.sql"), None);
        assert_eq!(sanitize_import_filename(".sql"), None);
        assert_eq!(sanitize_import_filename(""), None);
    }

    #[test]
    fn import_filename_rejects_illegal_chars_and_overlong() {
        assert_eq!(sanitize_import_filename("带中文.sql"), None);
        assert_eq!(sanitize_import_filename("has space.sql"), None);
        let long = format!("{}.sql", "a".repeat(252));
        assert_eq!(sanitize_import_filename(&long), None);
    }
}
