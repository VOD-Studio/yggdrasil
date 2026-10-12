//! COPY 感知的 SQL 预处理、原子恢复、临时文件清理与缓存失效。

use super::commands::psql_restore_command;
use super::files::backup_path;
use crate::api::database::tasks::{self, TaskStatus};
use std::path::{Path, PathBuf};

fn sql_line_without_eol(mut line: &[u8]) -> &[u8] {
    if let Some(stripped) = line.strip_suffix(b"\n") {
        line = stripped;
    }
    if let Some(stripped) = line.strip_suffix(b"\r") {
        line = stripped;
    }
    line
}

fn is_legacy_pg_dump_owner_statement(line: &[u8]) -> bool {
    const PREFIXES: [&[u8]; 3] = [b"ALTER FUNCTION ", b"ALTER SEQUENCE ", b"ALTER TABLE "];
    let line = sql_line_without_eol(line);
    PREFIXES.iter().any(|prefix| line.starts_with(prefix))
        && line
            .windows(b" OWNER TO ".len())
            .any(|window| window == b" OWNER TO ")
        && line.ends_with(b";")
}

/// 去除恢复脚本行首空白，确保带缩进的 psql 元命令也进入安全检查。
fn trim_restore_line_start(line: &[u8]) -> &[u8] {
    let line = sql_line_without_eol(line);
    let start = line
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(line.len());
    &line[start..]
}

fn is_allowed_restore_meta_command(line: &[u8]) -> bool {
    let line = trim_restore_line_start(line);
    let Some(rest) = line.strip_prefix(b"\\") else {
        return false;
    };
    let mut fields = rest
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|field| !field.is_empty());
    let Some(command) = fields.next() else {
        return false;
    };
    if command != b"restrict" && command != b"unrestrict" {
        return false;
    }
    let Some(token) = fields.next() else {
        return false;
    };
    !token.is_empty()
        && token
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        && fields.next().is_none()
}

fn write_owner_neutral_restore_sql(
    mut input: impl std::io::BufRead,
    mut output: impl std::io::Write,
) -> std::io::Result<usize> {
    let mut line = Vec::new();
    let mut in_copy_data = false;
    let mut removed = 0;
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        let sql_line = sql_line_without_eol(&line);
        if in_copy_data {
            output.write_all(&line)?;
            if sql_line == b"\\." {
                in_copy_data = false;
            }
            continue;
        }
        if sql_line.starts_with(b"COPY ") && sql_line.ends_with(b" FROM stdin;") {
            in_copy_data = true;
            output.write_all(&line)?;
        } else if is_legacy_pg_dump_owner_statement(&line) {
            removed += 1;
        } else if trim_restore_line_start(sql_line).first() == Some(&b'\\')
            && !is_allowed_restore_meta_command(&line)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unsupported psql meta-command in restore file",
            ));
        } else {
            output.write_all(&line)?;
        }
    }

    output.flush()?;
    Ok(removed)
}

struct PreparedRestoreSql {
    path: PathBuf,
    removed_owner_statements: usize,
}

impl PreparedRestoreSql {
    fn prepare(source: &Path) -> std::io::Result<Self> {
        let input = std::io::BufReader::new(std::fs::File::open(source)?);
        let path =
            std::env::temp_dir().join(format!("yggdrasil-restore-{}.sql", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let output = std::io::BufWriter::new(options.open(&path)?);
        match write_owner_neutral_restore_sql(input, output) {
            Ok(removed_owner_statements) => Ok(Self {
                path,
                removed_owner_statements,
            }),
            Err(error) => {
                let _ = std::fs::remove_file(&path);
                Err(error)
            }
        }
    }
}

impl Drop for PreparedRestoreSql {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            tracing::warn!(
                path = %self.path.display(),
                "restore: failed to delete prepared SQL: {error}"
            );
        }
    }
}
/// 后台执行恢复：探测 psql，可用则原子恢复，不可用则报告。
///
/// **跨环境恢复**：旧备份可能含 `ALTER ... OWNER TO ...`，而目标集群未必存在
/// 源库角色。恢复前流式生成临时副本，只移除 pg_dump 生成的属主语句；COPY 数据
/// 原样保留。新备份已由 `--no-owner --no-privileges` 从源头去掉角色元数据。
///
/// **原子恢复**：备份由 `pg_dump --clean --if-exists` 生成，脚本自带
/// `DROP ... IF EXISTS`；psql 使用 `--single-transaction`，任一 SQL 错误都会
/// 回滚先前的 DROP/CREATE/COPY，不留下半恢复数据库。
///
/// **错误中止**：`-v ON_ERROR_STOP=1` 让 psql 在第一条 SQL 错误时立即退出
/// （退出码 3）。否则 psql 即使满屏 ERROR 也返回 0，导致 `status.success()`
/// 误判为成功——这是「恢复完成却无任何数据变更」假成功的根因。
///
/// **恢复成功后失效全量缓存**：恢复会用备份时刻的数据重建 posts 等表，
/// 现有 moka 缓存（列表/标签/单篇/统计/搜索）与 SSR 世代号必须一并冲刷，
/// 否则前端仍读旧数据。
pub(super) async fn run_restore(task_id: &str, filename: &str) {
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
    // 预处理备份与 psql 恢复均为阻塞文件/子进程操作，整体移出 tokio worker。
    // 临时 SQL 由 RAII 守卫在成功、失败和启动错误路径统一删除。
    let restore_result = tokio::task::spawn_blocking(move || {
        let prepared = PreparedRestoreSql::prepare(&path)?;
        if prepared.removed_owner_statements > 0 {
            tracing::info!(
                removed = prepared.removed_owner_statements,
                "restore: removed legacy pg_dump owner statements"
            );
        }

        let mut command = psql_restore_command(&db_url, &prepared.path)?;
        command.output()
    })
    .await
    .unwrap_or_else(|join_e| Err(std::io::Error::other(join_e.to_string())));
    match restore_result {
        Ok(o) if o.status.success() => {
            // 恢复会覆盖文章、评论、用户、设置与素材元数据；所有内存/磁盘缓存
            // 必须一起冲刷。generation 仅用于观测，不能替代物理 SSR 删除。
            crate::cache::invalidate_all_post_caches();
            crate::cache::invalidate_search_results();
            crate::cache::invalidate_all_comments();
            crate::cache::invalidate_friend_links();
            crate::cache::invalidate_site_settings();
            crate::cache::invalidate_security_settings();
            crate::cache::invalidate_image_cache_settings();
            crate::cache::invalidate_log_targets();
            crate::cache::SESSION_CACHE.invalidate_all();
            crate::api::image::invalidate_all_caches().await;
            crate::ssr_cache::invalidate_ssr_all_public();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_owner_statements_are_removed_without_touching_copy_data() {
        let sql = concat!(
            "-- YGGDRASIL BACKUP v1\n",
            "ALTER FUNCTION public.f() OWNER TO yggdrasil;\n",
            "COPY public.lines (value) FROM stdin;\n",
            "ALTER TABLE public.literal OWNER TO text;\n",
            "\\.\n",
            "ALTER TABLE public.users OWNER TO yggdrasil;\n",
            "ALTER SEQUENCE public.users_id_seq OWNER TO yggdrasil;\n",
            "ALTER TABLE public.users ENABLE ROW LEVEL SECURITY;\n",
        );
        let mut output = Vec::new();
        let removed = write_owner_neutral_restore_sql(std::io::Cursor::new(sql), &mut output)
            .expect("过滤测试 SQL 应成功");
        let output = String::from_utf8(output).expect("测试 SQL 是 UTF-8");

        assert_eq!(removed, 3);
        assert!(
            output.contains("ALTER TABLE public.literal OWNER TO text;"),
            "COPY 数据不得被当成 SQL 删除: {output}"
        );
        assert!(!output.contains("OWNER TO yggdrasil"), "{output}");
        assert!(
            output.contains("ALTER TABLE public.users ENABLE ROW LEVEL SECURITY;"),
            "{output}"
        );
    }

    #[test]
    fn restore_rejects_shell_meta_commands() {
        let sql = concat!(
            "-- YGGDRASIL BACKUP v1\n",
            "\\! touch /tmp/yggdrasil-pwned\n",
        );
        let mut output = Vec::new();
        let error =
            write_owner_neutral_restore_sql(std::io::Cursor::new(sql), &mut output).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn restore_keeps_generated_restrict_meta_commands() {
        let sql = concat!(
            "\\restrict abc123\n",
            "SELECT 1;\n",
            "\\unrestrict abc123\n",
        );
        let mut output = Vec::new();
        write_owner_neutral_restore_sql(std::io::Cursor::new(sql), &mut output)
            .expect("generated restrict commands are allowed");
        let output = String::from_utf8(output).expect("test SQL is UTF-8");
        assert!(output.contains("\\restrict abc123"));
        assert!(output.contains("\\unrestrict abc123"));
    }
    /// Real pg_dump/psql regression. Requires a disposable ygg_backup_test database;
    /// uses its own URL so it can run independently of the application's global pool.
    #[tokio::test]
    #[ignore = "requires pg_dump, psql, and disposable YGG_BACKUP_TEST_DATABASE_URL"]
    async fn backup_round_trip_and_failed_restore_rolls_back_database() {
        use super::super::commands::pg_dump_command;
        use super::super::files::{has_valid_signature, read_first_line, BACKUP_SIGNATURE};

        let db_url = std::env::var("YGG_BACKUP_TEST_DATABASE_URL")
            .expect("set YGG_BACKUP_TEST_DATABASE_URL to a disposable ygg_backup_test database");
        let config: tokio_postgres::Config = db_url.parse().expect("valid test database URL");
        assert_eq!(
            config.get_dbname(),
            Some("ygg_backup_test"),
            "backup regression only runs on the disposable ygg_backup_test database"
        );
        let (client, connection) = config
            .connect(tokio_postgres::NoTls)
            .await
            .expect("connect to disposable backup database");
        tokio::spawn(async move {
            connection.await.expect("backup regression connection");
        });
        client
            .batch_execute(
                "DROP TABLE IF EXISTS public.ygg_backup_regression; \
                 CREATE TABLE public.ygg_backup_regression (id integer PRIMARY KEY, value text NOT NULL)",
            )
            .await
            .expect("create backup fixture");
        let values = [
            "ALTER TABLE public.literal OWNER TO absent_role;",
            "\\! COPY data stays literal\n正文保持换行",
        ];
        for (index, value) in values.iter().enumerate() {
            client
                .execute(
                    "INSERT INTO public.ygg_backup_regression VALUES ($1, $2)",
                    &[&(index as i32), value],
                )
                .await
                .expect("write backup fixture");
        }

        struct TestDirectory(PathBuf);
        impl Drop for TestDirectory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let dir = TestDirectory(std::env::temp_dir().join(format!(
            "yggdrasil-backup-regression-{}",
            uuid::Uuid::new_v4()
        )));
        std::fs::create_dir(&dir.0).expect("create regression directory");
        let source = dir.0.join("backup.sql");
        let dump = pg_dump_command(&db_url)
            .expect("build pg_dump command")
            .output()
            .expect("run pg_dump");
        assert!(dump.status.success(), "pg_dump must succeed");
        let mut sql = format!("{BACKUP_SIGNATURE}\n-- mode: pg_dump\n").into_bytes();
        sql.extend_from_slice(&dump.stdout);
        // Exercise legacy cross-role compatibility even on a new owner-neutral dump.
        sql.extend_from_slice(
            b"\nALTER TABLE public.ygg_backup_regression OWNER TO ygg_nonexistent_restore_role;\n",
        );
        std::fs::write(&source, &sql).expect("write signed backup");
        assert!(has_valid_signature(
            &read_first_line(&source).expect("read backup signature")
        ));
        client
            .execute(
                "UPDATE public.ygg_backup_regression SET value = 'changed'",
                &[],
            )
            .await
            .expect("change fixture before restoring");
        let prepared = PreparedRestoreSql::prepare(&source).expect("prepare real dump");
        assert_eq!(prepared.removed_owner_statements, 1);
        let prepared_path = prepared.path.clone();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&prepared_path)
                    .expect("prepared file metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let restored = psql_restore_command(&db_url, &prepared_path)
            .expect("build restore command")
            .output()
            .expect("run psql restore");
        assert!(restored.status.success(), "valid backup must restore");
        drop(prepared);
        assert!(
            !prepared_path.exists(),
            "successful restore cleans temporary SQL"
        );
        let actual: Vec<String> = client
            .query(
                "SELECT value FROM public.ygg_backup_regression ORDER BY id",
                &[],
            )
            .await
            .expect("read restored fixture")
            .iter()
            .map(|row| row.get(0))
            .collect();
        assert_eq!(actual, values);

        // A SQL failure after DROP/CREATE/COPY must preserve the current live state.
        client
            .execute(
                "UPDATE public.ygg_backup_regression SET value = 'live after backup'",
                &[],
            )
            .await
            .expect("change live fixture");
        sql.extend_from_slice(b"SELECT * FROM public.ygg_backup_missing_table;\n");
        std::fs::write(&source, &sql).expect("write failing restore fixture");
        let prepared = PreparedRestoreSql::prepare(&source).expect("prepare failing SQL");
        let prepared_path = prepared.path.clone();
        let failed = psql_restore_command(&db_url, &prepared_path)
            .expect("build failing restore command")
            .output()
            .expect("run failing restore");
        assert!(!failed.status.success(), "SQL error must stop restore");
        drop(prepared);
        assert!(
            !prepared_path.exists(),
            "failed restore cleans temporary SQL"
        );
        let actual: Vec<String> = client
            .query(
                "SELECT value FROM public.ygg_backup_regression ORDER BY id",
                &[],
            )
            .await
            .expect("read fixture after failed restore")
            .iter()
            .map(|row| row.get(0))
            .collect();
        assert_eq!(actual, vec!["live after backup"; values.len()]);
        client
            .batch_execute("DROP TABLE public.ygg_backup_regression")
            .await
            .expect("clean backup fixture");
    }
}
