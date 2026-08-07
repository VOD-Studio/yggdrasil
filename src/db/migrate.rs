//! 数据库迁移运行器。
//!
//! 在服务器启动时（`dioxus::server::serve()` 之前）自动执行迁移。
//! 设计要点：
//! - 迁移 SQL 通过 `include_str!` 内联进二进制，部署只需单个二进制。
//! - `schema_migrations` 表记录已应用版本，避免重复执行。
//! - 每个迁移在独立事务里执行，失败自动回滚，版本行不会写入。
//! - 咨询锁（`pg_advisory_lock`）保证多实例启动时只有一个进程执行迁移。
//!
//! 前置条件：目标数据库的存在性由 [`crate::db::pool::ensure_database`] 在本模块
//! 被调用前保证（它先连 `postgres` 维护库做 `CREATE DATABASE IF NOT EXISTS` 等价逻辑）。
//! 本模块只负责 schema，不再关心"库不存在"。
//!
//! 仅在 `feature = "server"` 时编译。

use std::collections::HashSet;

/// 咨询锁的固定 key。Postgres 咨询锁是数据库级唯一的；
/// 这里用一个项目专属的大整数，避免与同库其它应用冲突。
/// 该值无语义，仅要求唯一性。
const ADVISORY_LOCK_KEY: i64 = 0x5947_4752_4153_494C;

/// 所有迁移的 (version, sql) 列表，按 version 升序排列。
///
/// 新增迁移时：
/// 1. 在 `migrations/` 下创建 `NNN_描述.sql`
/// 2. 在本数组末尾追加一行 `("NNN", include_str!("../../migrations/NNN_描述.sql"))`
///
/// version 用字符串而非整数，与文件名前缀直接对应，且未来可支持日期戳/语义版本。
const MIGRATIONS: &[(&str, &str)] = &[
    ("001", include_str!("../../migrations/001_init.sql")),
    ("002", include_str!("../../migrations/002_posts.sql")),
    ("003", include_str!("../../migrations/003_indexes.sql")),
    ("004", include_str!("../../migrations/004_search_trgm.sql")),
    ("005", include_str!("../../migrations/005_comments.sql")),
    ("006", include_str!("../../migrations/006_add_toc_html.sql")),
    ("007", include_str!("../../migrations/007_settings.sql")),
    (
        "008",
        include_str!("../../migrations/008_comments_cascade.sql"),
    ),
    (
        "009",
        include_str!("../../migrations/009_cleanup_duplicate_indexes.sql"),
    ),
    (
        "010",
        include_str!("../../migrations/010_post_word_counts.sql"),
    ),
    ("011", include_str!("../../migrations/011_perf_indexes.sql")),
    (
        "012",
        include_str!("../../migrations/012_session_generation.sql"),
    ),
    (
        "013",
        include_str!("../../migrations/013_comment_content_hash_index.sql"),
    ),
    (
        "014",
        include_str!("../../migrations/014_drop_ineffective_trgm_index.sql"),
    ),
    ("015", include_str!("../../migrations/015_assets.sql")),
    (
        "016",
        include_str!("../../migrations/016_assets_content_hash.sql"),
    ),
    ("017", include_str!("../../migrations/017_mcp_tokens.sql")),
    (
        "018",
        include_str!("../../migrations/018_session_generation_trigger.sql"),
    ),
    (
        "019",
        include_str!("../../migrations/019_restore_search_trgm_index.sql"),
    ),
    ("020", include_str!("../../migrations/020_friend_links.sql")),
    (
        "021",
        include_str!("../../migrations/021_site_settings.sql"),
    ),
    // 新增迁移在此追加，同时在 migrations/ 下创建对应 .sql 文件。
];

/// 迁移执行错误。
#[derive(Debug)]
pub enum MigrateError {
    /// 无法从连接池获取连接。
    Pool(deadpool_postgres::PoolError),
    /// 执行查询（建表、查版本、咨询锁等）失败。
    Query(tokio_postgres::Error),
    /// 某个具体迁移执行失败（包含版本号便于定位）。
    Apply {
        version: String,
        source: tokio_postgres::Error,
    },
}

impl From<deadpool_postgres::PoolError> for MigrateError {
    fn from(e: deadpool_postgres::PoolError) -> Self {
        MigrateError::Pool(e)
    }
}

impl From<tokio_postgres::Error> for MigrateError {
    fn from(e: tokio_postgres::Error) -> Self {
        MigrateError::Query(e)
    }
}

impl std::fmt::Display for MigrateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 主动展开 source 链：tokio_postgres::Error 的 Display 对 DB 侧错误只打印
        // 无信息量的 `db error`，真正的 message/SQLSTATE/约束名藏在 source() 里。
        match self {
            MigrateError::Pool(e) => {
                write!(
                    f,
                    "database pool error: {}",
                    crate::db::format_with_sources(e)
                )
            }
            MigrateError::Query(e) => {
                write!(
                    f,
                    "database query error: {}",
                    crate::db::format_with_sources(e)
                )
            }
            MigrateError::Apply { version, source } => {
                write!(
                    f,
                    "migration {} failed: {}",
                    version,
                    crate::db::format_with_sources(source)
                )
            }
        }
    }
}

impl std::error::Error for MigrateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MigrateError::Pool(e) => Some(e),
            MigrateError::Query(e) => Some(e),
            MigrateError::Apply { source, .. } => Some(source),
        }
    }
}

/// 在**已获取**的连接上执行迁移主体逻辑（咨询锁 + 建表 + 应用迁移 + 解锁）。
///
/// 调用方负责自行控制连接获取策略——例如 `main.rs` 启动时用
/// [`get_conn_for_startup`](crate::db::pool::get_conn_for_startup)（长重试窗口）
/// 拿到连接后再调用本函数，以应对 DB 尚未就绪的场景。
///
/// 流程：
/// 1. 抢咨询锁（多实例启动时串行化）
/// 2. 确保 `schema_migrations` 表存在
/// 3. 查询已应用版本集合
/// 4. 按序应用未应用的迁移（每个一个事务）
/// 5. 释放咨询锁
///
/// 失败时返回错误；调用方（`main.rs`）应让进程退出，避免启动半残服务。
pub async fn run_on_conn(conn: &mut deadpool_postgres::Object) -> Result<(), MigrateError> {
    // 抢咨询锁：多实例滚动发布时只有一个进程能进入迁移循环，
    // 其余实例在此等待；锁释放后它们查版本表发现已全部应用，直接返回。
    conn.execute("SELECT pg_advisory_lock($1)", &[&ADVISORY_LOCK_KEY])
        .await?;

    // 在已持锁连接上执行迁移主体逻辑。
    // 锁释放策略：
    // - 正常返回 / 返回 Err：下面的显式 pg_advisory_unlock 释放锁。
    // - 进程被强杀（SIGKILL 等）：连接断开，Postgres 在检测到会话断开后释放
    //   session 级咨询锁。
    // - `main.rs` 在迁移失败时用 `std::process::exit(1)` 终止进程（不再 panic）：
    //   `exit(1)` 不会 unwind，但同样会关闭进程持有的所有 socket / 池连接，
    //   效果等价于会话断开——Postgres 会释放 session 级咨询锁。
    //   因此把 `.expect()` 改成 `exit(1)` 不破坏原有的锁安全保证。
    let result = run_inner(conn).await;

    // 无论成功失败都尝试显式释放锁；释放失败不应掩盖原始错误，仅记录告警。
    if let Err(unlock_err) = conn
        .execute("SELECT pg_advisory_unlock($1)", &[&ADVISORY_LOCK_KEY])
        .await
    {
        tracing::warn!("failed to release migration advisory lock: {}", unlock_err);
    }

    result
}

/// 在已持有咨询锁的连接上执行迁移主体逻辑。
async fn run_inner(conn: &mut deadpool_postgres::Object) -> Result<(), MigrateError> {
    // 确保版本表存在（独立语句，不在事务里，否则建表失败无法记录）。
    ensure_versions_table(conn).await?;

    // 查询已应用的版本集合。
    let applied = applied_versions(conn).await?;

    // 按序应用未应用的迁移。
    let mut applied_count = 0usize;
    for (version, sql) in MIGRATIONS {
        if applied.contains(*version) {
            continue;
        }
        tracing::info!("applying migration {}", version);
        apply_one(conn, version, sql).await?;
        applied_count += 1;
    }

    if applied_count == 0 {
        tracing::info!("database is up to date, 0 migrations applied");
    } else {
        tracing::info!("successfully applied {} migration(s)", applied_count);
    }
    Ok(())
}

/// 创建 `schema_migrations` 表（若不存在）。
async fn ensure_versions_table(conn: &deadpool_postgres::Object) -> Result<(), MigrateError> {
    conn.batch_execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    TEXT PRIMARY KEY,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .await?;
    Ok(())
}

/// 查询已应用的版本集合。
async fn applied_versions(
    conn: &deadpool_postgres::Object,
) -> Result<HashSet<String>, MigrateError> {
    let rows = conn
        .query("SELECT version FROM schema_migrations", &[])
        .await?;
    let mut set = HashSet::with_capacity(rows.len());
    for row in rows {
        set.insert(row.get::<_, String>(0));
    }
    Ok(set)
}

/// 在一个事务内应用单个迁移：执行 SQL + 写入版本行，失败则回滚。
async fn apply_one(
    conn: &mut deadpool_postgres::Object,
    version: &str,
    sql: &str,
) -> Result<(), MigrateError> {
    let tx = conn.transaction().await.map_err(MigrateError::Query)?;

    // batch_execute 执行整段 SQL（可含多条语句）。
    if let Err(e) = tx.batch_execute(sql).await {
        // 显式回滚以尽早释放事务（而非等 Transaction drop 的隐式回滚）；
        // 回滚本身的错误丢弃，因为已有更具信息量的 apply 错误要上报。
        let _ = tx.rollback().await;
        return Err(MigrateError::Apply {
            version: version.to_string(),
            source: e,
        });
    }

    // 记录版本行。显式构造 Apply 错误（不能用 ?，否则会被 blanket From 映射成 Query）。
    if let Err(e) = tx
        .execute(
            "INSERT INTO schema_migrations (version) VALUES ($1)",
            &[&version],
        )
        .await
    {
        let _ = tx.rollback().await;
        return Err(MigrateError::Apply {
            version: version.to_string(),
            source: e,
        });
    }

    tx.commit().await.map_err(|e| MigrateError::Apply {
        version: version.to_string(),
        source: e,
    })?;
    Ok(())
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_sorted_ascending() {
        let mut sorted = MIGRATIONS.iter().map(|(v, _)| *v).collect::<Vec<_>>();
        sorted.sort_unstable();
        let original: Vec<&str> = MIGRATIONS.iter().map(|(v, _)| *v).collect();
        assert_eq!(
            original, sorted,
            "MIGRATIONS must be in ascending version order"
        );
    }

    #[test]
    fn migrations_have_unique_versions() {
        let mut versions: Vec<&str> = MIGRATIONS.iter().map(|(v, _)| *v).collect();
        let total = versions.len();
        versions.sort_unstable();
        versions.dedup();
        assert_eq!(
            versions.len(),
            total,
            "MIGRATIONS has duplicate version strings"
        );
    }

    #[test]
    fn migrations_non_empty() {
        assert!(!MIGRATIONS.is_empty(), "MIGRATIONS must not be empty");
    }

    /// 防止"新建了 .sql 但忘记在 MIGRATIONS 加行"的脚枪。
    /// 扫描 migrations/ 目录，断言每个 .sql 文件都在 MIGRATIONS 里有对应版本行。
    /// 仅在 server feature + test 下运行（WASM 无文件系统）。
    #[test]
    fn migrations_match_files_on_disk() {
        use std::collections::HashSet;
        use std::fs;

        // CARGO_MANIFEST_DIR 指向 crate 根目录（yggdrasil/）。
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let migrations_dir = std::path::Path::new(manifest_dir).join("migrations");

        let mut files_on_disk: HashSet<String> = HashSet::new();
        for entry in fs::read_dir(&migrations_dir)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", migrations_dir.display(), e))
        {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("sql") {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_else(|| panic!("non-utf8 filename: {}", path.display()));
                // 文件名形如 "001_init.sql"，取前 3 位数字作为 version。
                let version = filename
                    .split('_')
                    .next()
                    .unwrap_or_else(|| panic!("filename has no '_' separator: {}", filename));
                files_on_disk.insert(version.to_string());
            }
        }

        let versions_in_array: HashSet<String> =
            MIGRATIONS.iter().map(|(v, _)| v.to_string()).collect();

        // 磁盘上有但数组里没有 → 忘记加行（会静默不执行该迁移）。
        let missing_in_array: Vec<&String> = files_on_disk.difference(&versions_in_array).collect();
        assert!(
            missing_in_array.is_empty(),
            "migrations/*.sql files not registered in MIGRATIONS: {:?}. \
             Add a row for each in src/db/migrate.rs.",
            missing_in_array
        );

        // 数组里有但磁盘上没有 → include_str! 本就会编译失败，这里只是双保险。
        let missing_on_disk: Vec<&String> = versions_in_array.difference(&files_on_disk).collect();
        assert!(
            missing_on_disk.is_empty(),
            "MIGRATIONS rows without a corresponding .sql file: {:?}",
            missing_on_disk
        );
    }
}
