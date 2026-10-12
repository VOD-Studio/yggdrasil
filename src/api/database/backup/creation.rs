//! 数据库备份执行、进度更新与 uploads 素材打包。

use super::commands::pg_dump_command;
use super::files::{
    backup_path, rotate_auto_backups, uploads_archive_name, BACKUP_DIR, BACKUP_SIGNATURE,
};
use super::{BackupOrigin, BackupRunOutcome};
use crate::api::database::tasks::{self, TaskKind, TaskStatus};
use chrono::Utc;
use std::path::Path;

/// 定时任务入口：注册任务后执行自动备份，返回结果供调度器落库 last_run。
pub(crate) async fn run_auto_backup() -> Result<BackupRunOutcome, String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    tasks::insert(task_id.clone(), TaskKind::Backup);
    run_backup(&task_id, BackupOrigin::Auto).await
}

/// 后台执行备份：pg_dump 优先，不可用回退纯 SQL；成功后按设置打包 uploads，
/// 自动备份再做轮转。Ok/Err 均已同步进任务进度表（供前端轮询），返回值供
/// 调度器落库 last_run。
pub(super) async fn run_backup(
    task_id: &str,
    origin: BackupOrigin,
) -> Result<BackupRunOutcome, String> {
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
/// 把 uploads 目录打成 tar.gz（排除可重建的 .cache/ 与 VCS 占位 .gitkeep）。
/// 阻塞 IO + CPU（gzip），调用方须放 spawn_blocking。
/// uploads_dir 显式传入（而非写死相对路径）便于测试注入临时目录。
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
            let mut command = pg_dump_command(&db_url).map_err(|error| (true, error))?;
            command
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

#[cfg(test)]
mod tests {
    use super::*;

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
