//! 数据库备份执行、进度更新与 uploads 素材打包。

use super::commands::pg_dump_command;
use super::files::{
    backup_path, backup_stamp, create_new_backup_file, rotate_auto_backups, uploads_archive_name,
    BACKUP_SIGNATURE,
};
use super::{BackupOrigin, BackupRunOutcome};
use crate::api::database::tasks::{self, TaskKind, TaskStatus};
use chrono::Utc;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// 已有备份在跑时拒绝新备份的提示（手动与自动共用）。
pub(super) const BACKUP_BUSY: &str = "已有备份正在进行，请等待完成后再试";

/// 进程内备份单飞标记：同一时刻只允许一个备份任务写 backups/。
static BACKUP_RUNNING: AtomicBool = AtomicBool::new(false);

/// 持有期间占住备份单飞标记；drop（含任务 panic 展开）时释放。
/// [`run_backup`] 按值接收，保证每个备份任务都先拿到它。
pub(super) struct BackupGuard(());

impl BackupGuard {
    /// 已有备份在跑时返回 None。
    pub(super) fn try_acquire() -> Option<Self> {
        BACKUP_RUNNING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(()))
    }
}

impl Drop for BackupGuard {
    fn drop(&mut self) {
        BACKUP_RUNNING.store(false, Ordering::Release);
    }
}

/// 定时任务入口：注册任务后执行自动备份，返回结果供调度器落库 last_run。
/// 手动备份仍在跑时跳过本次，不注册任务。
pub(crate) async fn run_auto_backup() -> Result<BackupRunOutcome, String> {
    let guard = BackupGuard::try_acquire().ok_or_else(|| BACKUP_BUSY.to_string())?;
    let task_id = uuid::Uuid::new_v4().to_string();
    tasks::insert(task_id.clone(), TaskKind::Backup);
    run_backup(&task_id, BackupOrigin::Auto, guard).await
}

/// 后台执行备份：pg_dump 成功后按设置打包 uploads，自动备份再做轮转。
/// Ok/Err 均已同步进任务进度表（供前端轮询），返回值供调度器落库 last_run。
pub(super) async fn run_backup(
    task_id: &str,
    origin: BackupOrigin,
    _guard: BackupGuard,
) -> Result<BackupRunOutcome, String> {
    let stamp = backup_stamp(Utc::now());
    let prefix = origin.file_prefix();

    // 读取备份设置（include_uploads / retention_count）；读取失败用默认值——
    // DB 不可达时 pg_dump 自身也会失败并按原路径上报。
    let settings = match crate::db::pool::get_conn().await {
        Ok(conn) => crate::api::settings::load_backup_settings(&conn)
            .await
            .unwrap_or_default(),
        Err(_) => crate::models::settings::BackupSettings::default(),
    };

    let sql_filename = match run_pg_dump_backup(task_id, prefix, &stamp).await {
        Ok(filename) => filename,
        Err(msg) => {
            tasks::update(
                task_id,
                "数据库备份失败",
                100,
                TaskStatus::Failed,
                None,
                Some(msg.clone()),
                None,
            );
            return Err(msg);
        }
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
            Err(e) => warning = Some(format!("uploads 打包任务失败: {e}")),
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

/// 文件写完整后用硬链接独占发布；暂存目录不对备份列表和下载接口可见。
fn publish_backup_file(
    path: &Path,
    write: impl FnOnce(File) -> Result<(), String>,
) -> Result<(), String> {
    struct StagedBackup(PathBuf);

    impl Drop for StagedBackup {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_file(&self.0) {
                tracing::warn!(path = %self.0.display(), "清理暂存备份文件失败: {error}");
            }
        }
    }

    let staging = path.with_file_name(".staging");
    std::fs::create_dir_all(&staging).map_err(|e| format!("创建备份暂存目录失败: {e}"))?;
    let temp = staging.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    let file = create_new_backup_file(&temp).map_err(|e| format!("创建暂存备份文件失败: {e}"))?;
    let staged = StagedBackup(temp);
    write(file)?;
    std::fs::hard_link(&staged.0, path).map_err(|e| format!("发布备份文件失败: {e}"))
}

/// 把 uploads 目录打成 tar.gz（排除可重建的 .cache/ 与 VCS 占位 .gitkeep）。
/// 阻塞 IO + CPU（gzip），调用方须放 spawn_blocking。
/// uploads_dir 显式传入（而非写死相对路径）便于测试注入临时目录。
fn create_uploads_tarball(uploads_dir: &Path, out_path: &Path) -> Result<(), String> {
    publish_backup_file(out_path, |file| {
        write_uploads_tarball(uploads_dir, file).map_err(|e| e.to_string())
    })
}

fn write_uploads_tarball(uploads_dir: &Path, file: File) -> std::io::Result<()> {
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
async fn run_pg_dump_backup(task_id: &str, prefix: &str, stamp: &str) -> Result<String, String> {
    tasks::update(
        task_id,
        "正在用 pg_dump 导出",
        10,
        TaskStatus::Running,
        None,
        None,
        None,
    );
    let filename = format!("{prefix}_{stamp}.sql");
    let path = backup_path(&filename);
    let db_url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .ok_or_else(|| "pg_dump 备份需要 DATABASE_URL".to_string())?;

    tokio::task::spawn_blocking(move || {
        let mut command =
            pg_dump_command(&db_url).map_err(|e| format!("构造 pg_dump 命令失败: {e}"))?;
        publish_backup_file(&path, |mut file| {
            writeln!(
                file,
                "{BACKUP_SIGNATURE}\n-- created_at: {}\n-- mode: pg_dump",
                Utc::now()
            )
            .map_err(|e| format!("写入备份文件头失败: {e}"))?;

            // 子进程从文件头之后续写；仅捕获 stderr，不将 SQL 读入内存。
            let output = command
                .stdout(file)
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        "数据库备份需要 pg_dump，请安装 PostgreSQL 客户端".to_string()
                    } else {
                        format!("启动 pg_dump 失败: {e}")
                    }
                })?
                .wait_with_output()
                .map_err(|e| format!("等待 pg_dump 失败: {e}"))?;
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stderr = stderr.trim();
                if stderr.is_empty() {
                    Err(format!("pg_dump 导出失败: {}", output.status))
                } else {
                    Err(format!("pg_dump 导出失败: {}: {stderr}", output.status))
                }
            }
        })
    })
    .await
    .map_err(|e| format!("备份 SQL 后台任务失败: {e}"))??;
    Ok(filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_guard_is_single_flight_until_dropped() {
        let guard = BackupGuard::try_acquire().expect("空闲时应能拿到备份标记");
        assert!(
            BackupGuard::try_acquire().is_none(),
            "已有备份在跑时必须拒绝第二个"
        );
        drop(guard);
        assert!(
            BackupGuard::try_acquire().is_some(),
            "上一个备份结束后应能再次发起"
        );
    }

    #[test]
    fn publication_hides_partial_writes_and_preserves_existing_file() {
        let dir =
            std::env::temp_dir().join(format!("yggdrasil-backup-publish-{}", uuid::Uuid::new_v4()));
        let path = dir.join("backup.sql");
        let staging = dir.join(".staging");

        let error = publish_backup_file(&path, |mut file| {
            file.write_all(b"partial").expect("写入部分备份");
            assert!(!path.exists(), "写入过程中不得暴露最终文件");
            Err("模拟写入失败".to_string())
        })
        .expect_err("失败的写入不得发布");
        assert_eq!(error, "模拟写入失败");
        assert!(!path.exists());
        assert!(std::fs::read_dir(&staging).unwrap().next().is_none());

        publish_backup_file(&path, |mut file| {
            file.write_all(b"complete").map_err(|e| e.to_string())
        })
        .expect("完整备份应发布");
        let error = publish_backup_file(&path, |mut file| {
            file.write_all(b"replacement").map_err(|e| e.to_string())
        })
        .expect_err("发布时不得覆盖已有文件");
        assert!(error.starts_with("发布备份文件失败"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), b"complete");
        assert!(std::fs::read_dir(&staging).unwrap().next().is_none());
        std::fs::remove_dir_all(&dir).expect("清理测试目录");
    }

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
}
