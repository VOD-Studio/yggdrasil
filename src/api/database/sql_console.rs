#![allow(clippy::unused_unit, deprecated)]

//! SQL 控制台执行（全读写 + 4 道护栏）。
//!
//! 护栏：
//! 1. 高危语句闸门：`DROP DATABASE`/`DROP SCHEMA`（字符串预检）绝禁；
//!    `DROP`/`TRUNCATE`/`ALTER` 需 `confirm_dangerous`。
//! 2. 无 WHERE 拦截：`UPDATE`/`DELETE` 无 `selection` 拒绝。
//! 3. 查询超时上限：复用 `STATEMENT_TIMEOUT_SECS`（pool 层已注入 GUC）。
//! 4. 前端二次确认（前端实现）。
//!
//! 默认禁止多语句（`allow_multi` 放开）。

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

// admin 鉴权 + DB 查询仅在 server 构建里被 server function 体引用。
#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub struct ExecuteSqlOpts {
    /// 是否允许多语句（`;` 分隔），默认 false。
    pub allow_multi: bool,
    /// 是否勾选「我了解后果」（放开 DROP/TRUNCATE/ALTER 等高危）。
    pub confirm_dangerous: bool,
    /// 是否带 EXPLAIN 执行计划。
    pub with_explain: bool,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, PartialEq)]
pub struct SqlResult {
    pub columns: Vec<String>,
    /// 每格用 JSON 表示（text/int/timestamp/bool/null）。
    pub rows: Vec<Vec<serde_json::Value>>,
    pub affected_rows: u64,
    pub elapsed_ms: u64,
    /// 语句类型（来自 AST，如 "Select"/"Update"/"CreateTable"）。
    pub statement_type: String,
    pub explain: Option<String>,
    /// SQL 已解析但执行失败时的脱敏/截断错误文本（admin-only 控制台可见）。
    pub error: Option<String>,
    /// 是否因 500 行上限截断。
    pub truncated: bool,
}

// 以下常量/枚举仅被 server function 体引用（WASM 构建里 server fn 体被 cfg 剥掉，
// 故这些符号也需 gate，否则非 server 构建会报 dead_code）。

/// 结果行数上限（超出截断 + 提示）。
#[cfg(feature = "server")]
const MAX_ROWS: usize = 500;

/// 绝对禁止的语句关键词（字符串预检，sqlparser 无 ObjectType::Database/Schema）。
/// 命中即拒，不可放行。
///
/// 注意：这里存的是「关键字序列」，由 [`is_absolutely_forbidden`] 做 token 级
/// 匹配——而非原始 `contains()` 子串。这样 `DROP   DATABASE`（多空格）、
/// `DROP\tDATABASE`、`DROP\nDATABASE` 等绕过单空格子串的写法都能命中。
/// 关键背景：sqlparser 的 PostgreSqlDialect 无法解析 DROP/CREATE DATABASE，
/// 这类语句【没有 AST 兜底】，字符串预检是唯一防线，故必须 token 级鲁棒。
#[cfg(feature = "server")]
const ABSOLUTELY_FORBIDDEN: &[&[&str]] = &[
    &["drop", "database"],
    &["drop", "schema"],
    &["create", "database"],
];

/// 护栏 1 的字符串预检（token 序列匹配）。
///
/// 把 SQL 按空白拆成 token（小写、去逗号/分号尾缀），再扫描是否出现
/// `ABSOLUTELY_FORBIDDEN` 里任一连续关键字序列。返回命中的序列描述供报错。
///
/// 用 token 序列而非 `contains()` 是为了拦多空格/制表符/换行绕过——
/// `drop   database` 在子串匹配下漏，在 token 序列下命中。
#[cfg(feature = "server")]
fn is_absolutely_forbidden(sql: &str) -> Option<&'static str> {
    // 规范化：小写 + 按任意空白拆分 + 去掉 token 尾部的 , ; ( )
    let lowered = sql.to_lowercase();
    let tokens: Vec<&str> = lowered
        .split_whitespace()
        .map(|t| t.trim_end_matches([',', ';', '(', ')']))
        .collect();
    for forbidden in ABSOLUTELY_FORBIDDEN {
        // 在 token 流里滑窗查连续序列
        for window in tokens.windows(forbidden.len()) {
            if window == *forbidden {
                return Some(match forbidden {
                    ["drop", "database"] => "DROP DATABASE",
                    ["drop", "schema"] => "DROP SCHEMA",
                    ["create", "database"] => "CREATE DATABASE",
                    _ => "未知高危操作",
                });
            }
        }
    }
    None
}

/// 护栏检查返回值。
#[cfg(feature = "server")]
#[derive(Debug)]
enum GuardResult {
    Allowed,
    /// 需 confirm_dangerous 才放行。
    NeedsConfirm,
    /// 不可放行（附带原因）。
    Forbidden(String),
}

/// 护栏 1+2：sqlparser 解析后遍历 AST，检查高危语句与无 WHERE 的 UPDATE/DELETE。
#[cfg(feature = "server")]
fn check_guards(asts: &[sqlparser::ast::Statement], confirm_dangerous: bool) -> GuardResult {
    use sqlparser::ast::{ObjectType, Statement};

    for stmt in asts {
        match stmt {
            // 护栏 1（绝禁）：DROP SCHEMA 和 DROP DATABASE 永远禁止。
            // 这里在 AST 层结构性禁止，防 SQL 注释/空白绕过字符串预检。
            Statement::Drop {
                object_type: ObjectType::Schema,
                ..
            } => {
                return GuardResult::Forbidden("禁止 DROP SCHEMA".to_string());
            }
            Statement::Drop {
                object_type: ObjectType::Database,
                ..
            } => {
                return GuardResult::Forbidden("禁止 DROP DATABASE".to_string());
            }
            // 护栏 1（绝禁）：CREATE DATABASE 永远禁止。sqlparser 能解析它，
            // 故在 AST 层补上结构禁止——这是字符串预检之外的第二道防线，
            // 防多空格/注释绕过 is_absolutely_forbidden 的 token 匹配。
            Statement::CreateDatabase { .. } => {
                return GuardResult::Forbidden("禁止 CREATE DATABASE".to_string());
            }
            // 护栏 1：需确认的高危语句（DROP TABLE/VIEW/INDEX 等、TRUNCATE、ALTER）
            Statement::Drop { .. } | Statement::Truncate { .. } | Statement::AlterTable { .. } => {
                if !confirm_dangerous {
                    return GuardResult::NeedsConfirm;
                }
            }
            // 护栏 2：UPDATE 无 WHERE
            Statement::Update(sqlparser::ast::Update {
                selection: None, ..
            }) => {
                return GuardResult::Forbidden(
                    "UPDATE 缺少 WHERE 子句，将影响全表。请加 WHERE 条件。".to_string(),
                );
            }
            // 护栏 2：DELETE 无 WHERE
            Statement::Delete(sqlparser::ast::Delete {
                selection: None, ..
            }) => {
                return GuardResult::Forbidden(
                    "DELETE 缺少 WHERE 子句，将影响全表。请加 WHERE 条件。".to_string(),
                );
            }
            _ => {}
        }
    }
    GuardResult::Allowed
}

/// 提取语句类型名（AST 变体名，如 "Select"/"Insert"/"Update"）。
#[cfg(feature = "server")]
fn statement_type_name(stmt: &sqlparser::ast::Statement) -> String {
    use sqlparser::ast::Statement;
    let name = match stmt {
        Statement::Query(_) => "Select",
        Statement::Insert(_) => "Insert",
        Statement::Update(_) => "Update",
        Statement::Delete(_) => "Delete",
        Statement::CreateTable { .. } => "CreateTable",
        Statement::AlterTable { .. } => "AlterTable",
        Statement::Drop { .. } => "Drop",
        Statement::Truncate { .. } => "Truncate",
        Statement::Explain { .. } => "Explain",
        _ => "Other",
    };
    name.to_string()
}

/// 判断语句是否只读（SELECT/WITH...SELECT/EXPLAIN/SHOW 系列）。
#[cfg(feature = "server")]
fn is_read_only(stmt: &sqlparser::ast::Statement) -> bool {
    use sqlparser::ast::Statement;
    matches!(
        stmt,
        // SELECT 与 WITH ... SELECT 均解析为 Query。
        Statement::Query(_)
            | Statement::Explain { .. }
            // SHOW 系列只读取元数据/配置参数，不会写入业务表。
            | Statement::ShowVariable { .. }
            | Statement::ShowVariables { .. }
            | Statement::ShowStatus { .. }
            | Statement::ShowCreate { .. }
            | Statement::ShowColumns { .. }
            | Statement::ShowCatalogs { .. }
            | Statement::ShowDatabases { .. }
            | Statement::ShowProcessList { .. }
            | Statement::ShowSchemas { .. }
            | Statement::ShowCharset(_)
            | Statement::ShowObjects(_)
            | Statement::ShowTables { .. }
            | Statement::ShowViews { .. }
            | Statement::ShowFunctions { .. }
            | Statement::ShowCollation { .. }
    )
}

/// 判断一组语句中是否含有写操作（非只读语句）。
///
/// SQL 控制台执行写操作（INSERT/UPDATE/DELETE/TRUNCATE/ALTER/DROP 等）可能直改
/// posts/comments/tags 等业务表，绕过 server function 的正常缓存失效路径。
/// 抽成纯函数便于单测「哪些语句集合应触发兜底失效」。
#[cfg(feature = "server")]
fn writes_affect_cache(stmts: &[sqlparser::ast::Statement]) -> bool {
    stmts.iter().any(|s| !is_read_only(s))
}

/// 把一列的值转成 JSON（按 PG 类型名分发）。
#[cfg(feature = "server")]
fn col_to_json(row: &tokio_postgres::Row, idx: usize) -> serde_json::Value {
    use serde_json::json;
    let ty = row
        .columns()
        .get(idx)
        .map(|c| c.type_().name())
        .unwrap_or("");
    match ty {
        "int2" => row
            .try_get::<_, Option<i16>>(idx)
            .ok()
            .flatten()
            .map(|v| json!(v))
            .unwrap_or(serde_json::Value::Null),
        "int4" => row
            .try_get::<_, Option<i32>>(idx)
            .ok()
            .flatten()
            .map(|v| json!(v))
            .unwrap_or(serde_json::Value::Null),
        "int8" => row
            .try_get::<_, Option<i64>>(idx)
            .ok()
            .flatten()
            .map(|v| json!(v))
            .unwrap_or(serde_json::Value::Null),
        "float4" => row
            .try_get::<_, Option<f32>>(idx)
            .ok()
            .flatten()
            .map(|v| json!(v))
            .unwrap_or(serde_json::Value::Null),
        "float8" => row
            .try_get::<_, Option<f64>>(idx)
            .ok()
            .flatten()
            .map(|v| json!(v))
            .unwrap_or(serde_json::Value::Null),
        "bool" => row
            .try_get::<_, Option<bool>>(idx)
            .ok()
            .flatten()
            .map(|v| json!(v))
            .unwrap_or(serde_json::Value::Null),
        // 其余（text/varchar/timestamp/jsonb/...）一律按字符串取，失败则 null
        _ => row
            .try_get::<_, Option<String>>(idx)
            .ok()
            .flatten()
            .map(|v| json!(v))
            .unwrap_or(serde_json::Value::Null),
    }
}

/// 执行 SQL（全读写，管理员）。护栏见模块文档。
#[server(ExecuteSql, "/api")]
pub async fn execute_sql(sql: String, opts: ExecuteSqlOpts) -> Result<SqlResult, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        use sqlparser::dialect::PostgreSqlDialect;
        use sqlparser::parser::Parser;

        // 护栏 1（绝禁）：字符串预检 DROP/CREATE DATABASE、DROP SCHEMA
        // （token 序列匹配，防多空格绕过；这类语句无 AST 兜底）
        if let Some(name) = is_absolutely_forbidden(&sql) {
            return Err(AppError::BadRequest(format!("禁止的操作：{}", name)).into());
        }

        // 解析 SQL
        let dialect = PostgreSqlDialect {};
        let asts = Parser::parse_sql(&dialect, &sql)
            .map_err(|e| AppError::BadRequest(format!("SQL 解析失败：{e}")))?;
        if asts.is_empty() {
            return Err(AppError::BadRequest("空的 SQL 语句".into()).into());
        }

        // 多语句检查（默认禁止）
        if asts.len() > 1 && !opts.allow_multi {
            return Err(AppError::BadRequest(
                "检测到多条语句，请勾选「允许多语句」后再执行".into(),
            )
            .into());
        }

        // 护栏 1+2：AST 检查
        match check_guards(&asts, opts.confirm_dangerous) {
            GuardResult::Forbidden(msg) => {
                return Err(AppError::BadRequest(msg).into());
            }
            GuardResult::NeedsConfirm => {
                return Err(AppError::BadRequest(
                    "高危操作（DROP/TRUNCATE/ALTER），需勾选「我了解后果」".into(),
                )
                .into());
            }
            GuardResult::Allowed => {}
        }

        let client = get_conn().await.map_err(AppError::db_conn)?;
        let start = std::time::Instant::now;

        // 逐条执行：每条用其 AST 重序列化的形式（stmt.to_string()），
        // 而非原始整段 SQL——保证执行的语句与护栏检查的 AST 完全一致，
        // 避免 allow_multi 时整段 SQL 被重复执行，也杜绝读/写分类与实际执行解耦。
        let mut last_result = SqlResult::default();
        for stmt in &asts {
            // 重序列化单条语句为可执行 SQL（去掉末尾分号，避免与 execute 的隐式分号冲突）
            let stmt_sql = stmt.to_string();
            last_result = execute_one(&client, stmt, &stmt_sql, opts.with_explain, start).await?;
            if last_result.error.is_some() {
                break;
            }
        }
        // 是否含写语句：抽成纯函数便于单测（见 writes_affect_cache）。
        let has_write = writes_affect_cache(&asts);
        // SQL 控制台写操作可能直改 posts/comments/tags，绕过了 server function 的
        // 正常失效路径（delete_post/create_post 等已在内部失效）。此处全量兜底失效，
        // 避免最长 10 分钟（TTL_SINGLE_POST=600s）内继续吐出陈旧数据（含已删除文章）。
        if has_write {
            crate::cache::invalidate_all_post_caches();
            crate::cache::invalidate_search_results();
            crate::cache::invalidate_all_comments();
            crate::ssr_cache::invalidate_ssr_all_public();
            crate::ssr_cache::bump_global_generation();
        }
        Ok(last_result)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (sql, opts);
        Ok(SqlResult::default())
    }
}

/// 执行单条语句，返回结果。
///
/// `stmt_sql` 必须是 `stmt` 重序列化后的**单条**语句 SQL（不含其他语句），
/// 保证护栏检查的 AST 与实际执行的语句一致。

/// 把 admin-only SQL 执行错误限制在可控长度，避免异常文本撑大 server function 响应。
#[cfg(feature = "server")]
fn format_sql_error(error: impl std::fmt::Display) -> String {
    error.to_string().chars().take(2_000).collect()
}

#[cfg(feature = "server")]
async fn execute_one(
    client: &deadpool_postgres::Object,
    stmt: &sqlparser::ast::Statement,
    stmt_sql: &str,
    with_explain: bool,
    start: impl Fn() -> std::time::Instant + Copy,
) -> Result<SqlResult, ServerFnError> {
    let statement_type = statement_type_name(stmt);
    let read_only = is_read_only(stmt);

    if with_explain && read_only {
        // EXPLAIN 模式：包裹单条语句取执行计划，取首列文本拼接
        let explain_sql = format!("EXPLAIN {}", stmt_sql.trim_end_matches(';'));
        let rows = match client.query(&explain_sql, &[]).await {
            Ok(rows) => rows,
            Err(error) => {
                return Ok(SqlResult {
                    statement_type,
                    error: Some(format_sql_error(error)),
                    elapsed_ms: start().elapsed().as_millis() as u64,
                    ..Default::default()
                });
            }
        };
        let explain = rows
            .iter()
            .filter_map(|r| r.try_get::<_, String>(0).ok())
            .collect::<Vec<_>>()
            .join("\n");
        return Ok(SqlResult {
            statement_type,
            explain: Some(explain),
            elapsed_ms: start().elapsed().as_millis() as u64,
            ..Default::default()
        });
    }

    if read_only {
        // 只读：取结果集。列名从第一行取（空结果集时无列名，前端容错）。
        // 包成子查询并追加 LIMIT MAX_ROWS+1，让 DB 只回传这么多行——避免
        // `SELECT * FROM big_table` 把整张表物化进内存（仅 statement_timeout 兜底，M11）。
        // +1 行供客户端检测截断：实际行数 > MAX_ROWS 时 truncated=true。
        // 内层 SQL 已带 LIMIT 时外层 LIMIT 取更小值，结果正确；列名随子查询透传。
        let limited_sql = format!(
            "SELECT * FROM ({}) AS q LIMIT {}",
            stmt_sql.trim_end_matches(';'),
            MAX_ROWS + 1
        );
        let rows = match client.query(&limited_sql, &[]).await {
            Ok(rows) => rows,
            Err(error) => {
                return Ok(SqlResult {
                    statement_type,
                    error: Some(format_sql_error(error)),
                    elapsed_ms: start().elapsed().as_millis() as u64,
                    ..Default::default()
                });
            }
        };
        let columns: Vec<String> = rows
            .first()
            .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
            .unwrap_or_default();
        let mut data: Vec<Vec<serde_json::Value>> = Vec::new();
        let mut truncated = false;
        for r in &rows {
            if data.len() >= MAX_ROWS {
                truncated = true;
                break;
            }
            let row: Vec<serde_json::Value> = (0..r.len()).map(|i| col_to_json(r, i)).collect();
            data.push(row);
        }
        Ok(SqlResult {
            columns,
            rows: data,
            truncated,
            statement_type,
            elapsed_ms: start().elapsed().as_millis() as u64,
            ..Default::default()
        })
    } else {
        // 写操作：返回影响行数
        let affected = match client.execute(stmt_sql, &[]).await {
            Ok(affected) => affected,
            Err(error) => {
                return Ok(SqlResult {
                    statement_type,
                    error: Some(format_sql_error(error)),
                    elapsed_ms: start().elapsed().as_millis() as u64,
                    ..Default::default()
                });
            }
        };
        Ok(SqlResult {
            affected_rows: affected,
            statement_type,
            elapsed_ms: start().elapsed().as_millis() as u64,
            ..Default::default()
        })
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    /// 用 PostgreSql 方言解析 SQL 为 AST 列表,供测试复用。
    fn parse(sql: &str) -> Vec<sqlparser::ast::Statement> {
        use sqlparser::dialect::PostgreSqlDialect;
        use sqlparser::parser::Parser;
        Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap_or_default()
    }

    // ── 护栏 1:绝对禁止(token 序列字符串预检层) ──────────────────
    // is_absolutely_forbidden 做 token 级匹配,防多空格/制表符/换行绕过。
    // 关键背景:sqlparser 无法解析 DROP/CREATE DATABASE,这一层是唯一防线。

    #[test]
    fn absolutely_forbidden_targets_database_and_schema() {
        // 锁定关键字序列集合:任何改动都应是有意识的。
        assert_eq!(
            ABSOLUTELY_FORBIDDEN,
            &[
                &["drop", "database"],
                &["drop", "schema"],
                &["create", "database"],
            ]
        );
    }

    #[test]
    fn precheck_catches_forbidden_regardless_of_case() {
        for sql in [
            "DROP DATABASE yggdrasil",
            "drop schema public",
            "CREATE DATABASE evil",
            "Drop Database x",
        ] {
            assert!(is_absolutely_forbidden(sql).is_some(), "应拦截: {sql:?}");
        }
    }

    #[test]
    fn precheck_catches_multi_space_bypass() {
        // token 序列匹配必须命中多空格/制表符/换行绕过——
        // 这是改用 is_absolutely_forbidden(替代旧 contains)的核心动机:
        // sqlparser 无法解析 DROP/CREATE DATABASE,这一层是唯一防线。
        for sql in [
            "DROP   DATABASE x",
            "drop\tdatabase\tx",
            "DROP\nDATABASE\nx",
            "DROP\t\tDATABASE x;",
        ] {
            assert!(
                is_absolutely_forbidden(sql).is_some(),
                "多空格绕过应被拦截: {sql:?}"
            );
        }
    }

    #[test]
    fn precheck_returns_canonical_name_for_error_message() {
        assert_eq!(
            is_absolutely_forbidden("DROP DATABASE x"),
            Some("DROP DATABASE")
        );
        assert_eq!(
            is_absolutely_forbidden("drop schema public"),
            Some("DROP SCHEMA")
        );
        assert_eq!(
            is_absolutely_forbidden("CREATE DATABASE evil"),
            Some("CREATE DATABASE")
        );
    }

    #[test]
    fn create_database_is_blocked_at_both_string_and_ast_layers() {
        // CREATE DATABASE 能被 sqlparser 解析(→ AST),故有双重防线:
        // 1. is_absolutely_forbidden(字符串 token 预检)
        // 2. check_guards 的 Statement::CreateDatabase 分支(AST 层绝禁)
        // 本测试锁定两道防线都生效,且确认 CREATE DATABASE 确实可被解析
        // (若未来它变得不可解析,字符串预检就成唯一防线,需另作加固)。
        assert!(is_absolutely_forbidden("CREATE DATABASE x").is_some());
        let asts = parse("CREATE DATABASE x");
        assert!(!asts.is_empty(), "CREATE DATABASE 应可被 sqlparser 解析");
        assert!(matches!(
            check_guards(&asts, true),
            GuardResult::Forbidden(_)
        ));
    }

    #[test]
    fn drop_database_is_guarded_by_both_precheck_and_ast_check() {
        let asts = parse("DROP DATABASE x");
        assert!(!asts.is_empty(), "DROP DATABASE 应可被 sqlparser 解析");
        assert!(matches!(
            check_guards(&asts, true),
            GuardResult::Forbidden(_)
        ));
        assert!(is_absolutely_forbidden("DROP DATABASE x").is_some());
    }

    #[test]
    fn precheck_ignores_benign_statements() {
        for sql in [
            "SELECT * FROM users",
            "DROP TABLE old_logs",
            "CREATE TABLE t (id int)",
            "DELETE FROM t WHERE id = 1",
            "CREATE INDEX idx ON t (col)",
        ] {
            assert!(is_absolutely_forbidden(sql).is_none(), "不应误拦: {sql:?}");
        }
    }

    // ── 护栏 1:AST 层 DROP SCHEMA 绝禁(防注释/空白绕过字符串预检) ──

    #[test]
    fn guard_forbids_drop_schema_even_though_string_precheck_is_bypassable() {
        // 即使字符串预检被某种方式绕过,AST 层仍绝禁 DROP SCHEMA。
        let asts = parse("DROP SCHEMA public");
        match check_guards(&asts, true) {
            GuardResult::Forbidden(msg) => {
                assert!(msg.contains("SCHEMA"), "DROP SCHEMA 应被禁止, 得到: {msg}")
            }
            other => panic!("DROP SCHEMA 应 Forbidden, 得到 {other:?}"),
        }
    }

    #[test]
    fn guard_drop_schema_ignores_confirm_flag() {
        // 即便勾选了 confirm_dangerous,DROP SCHEMA 仍绝禁。
        let asts = parse("DROP SCHEMA public");
        assert!(matches!(
            check_guards(&asts, true),
            GuardResult::Forbidden(_)
        ));
    }

    // ── 护栏 1:DROP/TRUNCATE/ALTER 需确认 ─────────────────────────

    #[test]
    fn guard_drop_table_needs_confirm() {
        let asts = parse("DROP TABLE old_logs");
        assert!(matches!(
            check_guards(&asts, false),
            GuardResult::NeedsConfirm
        ));
    }

    #[test]
    fn guard_drop_table_allowed_with_confirm() {
        let asts = parse("DROP TABLE old_logs");
        assert!(matches!(check_guards(&asts, true), GuardResult::Allowed));
    }

    #[test]
    fn guard_truncate_needs_confirm() {
        let asts = parse("TRUNCATE TABLE sessions");
        assert!(matches!(
            check_guards(&asts, false),
            GuardResult::NeedsConfirm
        ));
    }

    #[test]
    fn guard_alter_table_needs_confirm() {
        let asts = parse("ALTER TABLE posts ADD COLUMN foo text");
        assert!(matches!(
            check_guards(&asts, false),
            GuardResult::NeedsConfirm
        ));
    }

    #[test]
    fn guard_alter_table_allowed_with_confirm() {
        let asts = parse("ALTER TABLE posts ADD COLUMN foo text");
        assert!(matches!(check_guards(&asts, true), GuardResult::Allowed));
    }

    // ── 护栏 2:UPDATE/DELETE 无 WHERE 绝禁 ────────────────────────

    #[test]
    fn guard_update_without_where_is_forbidden() {
        let asts = parse("UPDATE posts SET title = 'x'");
        match check_guards(&asts, true) {
            GuardResult::Forbidden(msg) => assert!(msg.contains("WHERE")),
            other => panic!("无 WHERE 的 UPDATE 应 Forbidden, 得到 {other:?}"),
        }
    }

    #[test]
    fn guard_delete_without_where_is_forbidden() {
        let asts = parse("DELETE FROM posts");
        match check_guards(&asts, true) {
            GuardResult::Forbidden(msg) => assert!(msg.contains("WHERE")),
            other => panic!("无 WHERE 的 DELETE 应 Forbidden, 得到 {other:?}"),
        }
    }

    #[test]
    fn guard_update_with_where_allowed() {
        let asts = parse("UPDATE posts SET title = 'x' WHERE id = 1");
        assert!(matches!(check_guards(&asts, false), GuardResult::Allowed));
    }

    #[test]
    fn guard_delete_with_where_allowed() {
        let asts = parse("DELETE FROM posts WHERE id = 1");
        assert!(matches!(check_guards(&asts, false), GuardResult::Allowed));
    }

    #[test]
    fn guard_update_with_where_not_rescued_by_confirm() {
        // confirm_dangerous 不应放行无 WHERE 的 UPDATE/DELETE——这是不可协商的安全护栏。
        let asts = parse("UPDATE posts SET title = 'x'");
        assert!(matches!(
            check_guards(&asts, true),
            GuardResult::Forbidden(_)
        ));
    }

    // ── 正常语句 ───────────────────────────────────────────────────

    #[test]
    fn guard_allows_select_insert_create() {
        for sql in [
            "SELECT * FROM posts",
            "INSERT INTO posts (title) VALUES ('x')",
            "CREATE TABLE t (id int)",
        ] {
            let asts = parse(sql);
            assert!(
                matches!(check_guards(&asts, false), GuardResult::Allowed),
                "应放行: {sql}"
            );
        }
    }

    // ── 多语句:任一命中即拒 ───────────────────────────────────────

    #[test]
    fn guard_checks_all_statements_in_batch() {
        // 第二条是无 WHERE 的 DELETE,即便第一条正常,整体也应被拦。
        let asts = parse("SELECT 1; DELETE FROM posts");
        assert!(matches!(
            check_guards(&asts, false),
            GuardResult::Forbidden(_)
        ));
    }

    #[test]
    fn guard_first_dangerous_short_circuits() {
        // 第一条 DROP TABLE 未确认,应在 NeedsConfirm 处停下。
        let asts = parse("DROP TABLE a; SELECT 1");
        assert!(matches!(
            check_guards(&asts, false),
            GuardResult::NeedsConfirm
        ));
    }

    // ── statement_type_name ───────────────────────────────────────

    #[test]
    fn statement_type_name_maps_variants() {
        assert_eq!(statement_type_name(&parse("SELECT 1")[0]), "Select");
        assert_eq!(
            statement_type_name(&parse("INSERT INTO t (a) VALUES (1)")[0]),
            "Insert"
        );
        assert_eq!(
            statement_type_name(&parse("UPDATE t SET a = 1 WHERE id = 1")[0]),
            "Update"
        );
        assert_eq!(
            statement_type_name(&parse("DELETE FROM t WHERE id = 1")[0]),
            "Delete"
        );
        assert_eq!(
            statement_type_name(&parse("CREATE TABLE t (id int)")[0]),
            "CreateTable"
        );
        assert_eq!(
            statement_type_name(&parse("ALTER TABLE t ADD COLUMN x int")[0]),
            "AlterTable"
        );
        assert_eq!(statement_type_name(&parse("DROP TABLE t")[0]), "Drop");
        assert_eq!(statement_type_name(&parse("TRUNCATE t")[0]), "Truncate");
    }

    // ── is_read_only ──────────────────────────────────────────────

    #[test]
    fn is_read_only_classifies_correctly() {
        assert!(is_read_only(&parse("SELECT 1")[0]));
        assert!(is_read_only(&parse("EXPLAIN SELECT 1")[0]));
        assert!(!is_read_only(&parse("UPDATE t SET a = 1 WHERE id = 1")[0]));
        assert!(!is_read_only(&parse("DELETE FROM t WHERE id = 1")[0]));
        assert!(!is_read_only(&parse("INSERT INTO t (a) VALUES (1)")[0]));
    }

    // ── writes_affect_cache（SQL 控制台兜底失效开关） ─────────────
    // 任何写语句（非只读）都应触发全量缓存失效，兜底绕过 server function 的直改 DB。

    #[test]
    fn writes_affect_cache_true_for_write_statements() {
        for sql in [
            "UPDATE posts SET deleted_at = NOW() WHERE id = 648",
            "DELETE FROM posts WHERE id = 648",
            "INSERT INTO posts (title) VALUES ('x')",
            "TRUNCATE posts",
            "ALTER TABLE posts ADD COLUMN x int",
            "DROP TABLE posts",
        ] {
            assert!(
                writes_affect_cache(&parse(sql)),
                "写语句应触发兜底失效：{sql:?}"
            );
        }
    }

    #[test]
    fn writes_affect_cache_false_for_read_only_statements() {
        for sql in [
            "SELECT 1",
            "EXPLAIN SELECT * FROM posts",
            "SELECT id FROM posts WHERE id = 1",
        ] {
            assert!(
                !writes_affect_cache(&parse(sql)),
                "只读语句不应触发兜底失效：{sql:?}"
            );
        }
    }

    #[test]
    fn writes_affect_cache_mixed_statements_flagged() {
        // 多语句中混入任一写语句即应触发（与 allow_multi 语义一致）。
        let stmts = parse("SELECT 1; UPDATE posts SET deleted_at = NULL WHERE id = 648");
        assert!(writes_affect_cache(&stmts));
    }
    #[test]
    fn format_sql_error_truncates_long_messages() {
        let message = format_sql_error("x".repeat(2_500));
        assert_eq!(message.chars().count(), 2_000);
    }
}
