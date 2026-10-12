//! 备份命名、签名、元信息、配对归档与自动轮转。

use super::BackupInfo;
use crate::api::error::AppError;
use chrono::Utc;
use std::path::{Component, Path, PathBuf};

/// 与 uploads/ 同级且被 Git 忽略的备份目录。
pub(super) const BACKUP_DIR: &str = "backups";
/// 文件名白名单禁止目录分隔符与 shell 元字符。
const FILENAME_RE: &str = r"^[a-zA-Z0-9_.\-]+$";
/// 导入与恢复共同要求的备份签名。
pub(super) const BACKUP_SIGNATURE: &str = "-- YGGDRASIL BACKUP v1";

/// SQL 文件名 → 配对 uploads 打包文件名（`X.sql` → `X_uploads.tar.gz`）。
pub(super) fn uploads_archive_name(sql_filename: &str) -> String {
    format!("{}_uploads.tar.gz", sql_filename.trim_end_matches(".sql"))
}
/// 从自动备份 SQL 文件名列表挑出超出保留份数、应删除的最旧者。
/// 文件名内嵌定宽时间戳，字典序即时间序。纯函数便于单测。
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
pub(super) fn rotate_auto_backups(keep: i32) {
    rotate_auto_backups_in(Path::new(BACKUP_DIR), keep);
}

fn rotate_auto_backups_in(directory: &Path, keep: i32) {
    let names: Vec<String> = match std::fs::read_dir(directory) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect(),
        Err(e) => {
            tracing::warn!("backup rotation: cannot read {}: {e}", directory.display());
            return;
        }
    };
    for sql_name in select_expired_auto_backups(&names, keep.max(0) as usize) {
        for path in [
            directory.join(&sql_name),
            directory.join(uploads_archive_name(&sql_name)),
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
pub(super) fn backup_path(filename: &str) -> PathBuf {
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
pub(super) fn is_valid_backup_filename(filename: &str) -> bool {
    // regex::Regex::new 在 FILENAME_RE 是常量正则,编译期可验证不会 panic。
    regex::Regex::new(FILENAME_RE)
        .map(|re| re.is_match(filename))
        .unwrap_or(false)
}
/// 查询 backups/ 所在文件系统的可用空间（取最长挂载点前缀的磁盘）。
/// 预检用途：探测失败返回 None，降级为跳过预检（写入失败仍会清理并报错）。
pub(super) fn backup_partition_free_space() -> Option<u64> {
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
pub(super) fn parse_backup_mode(content: &str) -> String {
    content
        .lines()
        .find(|l| l.starts_with("-- mode:"))
        .map(|l| l.trim_start_matches("-- mode:").trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// 校验备份文件首行是否含本系统签名头。
/// 提取为纯函数:把首行(或全文)作为参数传入,便于单测。
pub(super) fn has_valid_signature(content: &str) -> bool {
    content
        .lines()
        .next()
        .map(|l| l.trim().contains(BACKUP_SIGNATURE))
        .unwrap_or(false)
}

/// 仅读取文件首行（用于签名校验，避免把整个备份文件读入内存）。
pub(super) fn read_first_line(path: impl AsRef<Path>) -> std::io::Result<String> {
    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line)
}

/// 读取文件前 `n` 行（用于解析 `-- mode:` 等头部元信息，避免整文件读入内存）。
/// 不足 `n` 行时返回实际读到的行；任一行读取失败则整体返回该错误。
pub(super) fn read_first_lines(path: impl AsRef<Path>, n: usize) -> std::io::Result<Vec<String>> {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(std::fs::File::open(path)?);
    reader.lines().take(n).collect()
}

pub(super) fn list_backup_files() -> Vec<BackupInfo> {
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
    infos
}

pub(super) fn delete_backup_files(filename: &str) -> Result<(), AppError> {
    if !is_valid_backup_filename(filename) {
        return Err(AppError::BadRequest("无效的文件名".to_string()));
    }
    let path = backup_path(filename);
    if !path.exists() {
        return Err(AppError::NotFound("备份文件不存在"));
    }
    std::fs::remove_file(&path).map_err(|_| AppError::Internal("删除失败"))?;
    // 成对清理：传 .sql 删配对 tar.gz，传 tar.gz 删配对 .sql。
    let pair = if filename.ends_with(".sql") {
        Some(uploads_archive_name(filename))
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

#[cfg(test)]
mod tests {
    use super::*;

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
    #[test]
    fn rotation_removes_expired_sql_and_pair_but_preserves_manual_backups() {
        let dir = std::env::temp_dir().join(format!(
            "yggdrasil-backup-rotation-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&dir).expect("create rotation fixture directory");
        let names = [
            "backup_20260101_000000.sql",
            "auto_20260808_040000.sql",
            "auto_20260809_040000.sql",
            "auto_20260810_040000.sql",
        ];
        for name in names {
            std::fs::write(dir.join(name), b"backup").expect("write SQL fixture");
            std::fs::write(dir.join(uploads_archive_name(name)), b"uploads")
                .expect("write paired uploads fixture");
        }
        let incomplete = dir.join(".import-test.tmp");
        std::fs::write(&incomplete, b"partial").expect("write temporary import fixture");
        rotate_auto_backups_in(&dir, 2);
        for name in names {
            let should_remain = name != "auto_20260808_040000.sql";
            assert_eq!(dir.join(name).exists(), should_remain, "{name}");
            assert_eq!(
                dir.join(uploads_archive_name(name)).exists(),
                should_remain,
                "paired archive of {name}"
            );
        }
        assert!(
            incomplete.exists(),
            "rotation only removes completed automatic backups"
        );
        std::fs::remove_dir_all(&dir).expect("clean rotation fixture");
    }
}
