//! 数据导出：Axum 流式路由（大文件不走 JSON 序列化）。
//!
//! 鉴权走 [`crate::api::auth::require_admin_from_headers`]：从 cookie 取 session，校验管理员。
//! 两种来源：按表导出（表名白名单）/ 按查询导出（强制只读，AST 校验）。
//! 两种格式：CSV（COPY TO STDOUT 流式）/ SQL（逐行拼 INSERT）。

#![allow(clippy::unused_unit)]

use axum::body::Body;
use axum::extract::Query;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use futures::StreamExt;
use serde::Deserialize;
use tokio_postgres::types::{FromSql, Type};

use super::read_only::is_read_only_statement;

#[derive(Deserialize)]
pub struct ExportParams {
    /// `table:<name>` 或 `query:<SELECT ...>`。
    pub source: String,
    /// `sql` 或 `csv`。
    pub format: String,
    /// 是否带表头（CSV）/ 列名（SQL INSERT）。默认 true。
    pub include_columns: Option<bool>,
}

/// GET /api/database/export —— 流式导出。
///
/// 前端用 `window.open` 触发下载，因此保持 GET；路由挂
/// [`crate::api::csrf::same_origin_read_middleware`] 拒绝跨站触发。
pub async fn export_data(
    headers: HeaderMap,
    Query(params): Query<ExportParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // 1. 鉴权：cookie → session → admin
    crate::api::auth::require_admin_from_headers(&headers)
        .await
        .map_err(|(status, msg)| (status, msg.to_string()))?;

    // 2. 解析来源 + 白名单/只读校验
    let include_columns = params.include_columns.unwrap_or(true);
    let source = parse_source(&params.source)?;

    match params.format.as_str() {
        "csv" => export_csv(source.sql, include_columns).await,
        "sql" => export_sql(source, include_columns).await,
        _ => Err((StatusCode::BAD_REQUEST, "不支持的格式".to_string())),
    }
    .map(|(body, content_type, filename)| {
        let disposition = format!("attachment; filename=\"{}\"", filename);
        let disposition_value = axum::http::HeaderValue::from_str(&disposition)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("attachment"));
        let content_type_value = axum::http::HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("application/octet-stream"));
        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, content_type_value),
                (header::CONTENT_DISPOSITION, disposition_value),
            ],
            body,
        )
    })
}

/// 解析后的导出来源。
#[derive(Debug)]
struct ExportSource {
    /// 内部 SQL。
    sql: String,
    /// SQL 导出的 INSERT 目标表名。
    table_name: String,
    /// 是否全是查询语句：查询能包进子查询逐列转 text，EXPLAIN 不能。
    query_only: bool,
}

/// 解析导出来源。
/// - `table:posts` → 校验表名合法后，`SELECT * FROM "posts"`（只读）+ 表名 "posts"。
/// - `query:SELECT ...` → 校验为只读语句后原样返回，表名为 "export"。
fn parse_source(source: &str) -> Result<ExportSource, (StatusCode, String)> {
    if let Some(table) = source.strip_prefix("table:") {
        // 表名白名单：仅允许标识符字符，防注入
        let t = table.trim();
        if t.is_empty() || !is_simple_ident(t) {
            return Err((StatusCode::BAD_REQUEST, "无效的表名".to_string()));
        }
        Ok(ExportSource {
            sql: format!("SELECT * FROM \"{}\"", t),
            table_name: t.to_string(),
            query_only: true,
        })
    } else if let Some(query) = source.strip_prefix("query:") {
        // 只读校验：sqlparser 解析后所有语句都必须是无副作用的查询或非 ANALYZE 的 EXPLAIN
        let dialect = sqlparser::dialect::PostgreSqlDialect {};
        let parsed = sqlparser::parser::Parser::parse_sql(&dialect, query)
            .map_err(|_| (StatusCode::BAD_REQUEST, "SQL 解析失败".to_string()))?;
        if parsed.iter().any(|s| !is_read_only_statement(s)) {
            return Err((
                StatusCode::BAD_REQUEST,
                "导出查询必须是只读（SELECT/EXPLAIN）".to_string(),
            ));
        }
        Ok(ExportSource {
            sql: query.to_string(),
            table_name: "export".to_string(),
            query_only: parsed
                .iter()
                .all(|s| matches!(s, sqlparser::ast::Statement::Query(_))),
        })
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "source 必须是 table:<name> 或 query:<sql>".to_string(),
        ))
    }
}

/// 简单标识符校验（字母数字下划线，防 SQL 注入与路径穿越）。
fn is_simple_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// CSV 导出：用 COPY ... TO STDOUT WITH CSV 流式（大表不 OOM）。
///
/// 流式响应在函数返回后才消费完，无法在同一连接上以事务包住整个 COPY，
/// 这里只依赖 [`parse_source`] 的 AST 只读校验。
async fn export_csv(
    source_sql: String,
    include_columns: bool,
) -> Result<(Body, &'static str, String), (StatusCode, String)> {
    let client = crate::db::pool::get_conn()
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "数据库不可用".to_string()))?;

    let header_clause = if include_columns { "HEADER" } else { "" };
    let copy_stmt = format!("COPY ({}) TO STDOUT WITH CSV {}", source_sql, header_clause);
    let stream = client.copy_out(&copy_stmt).await.map_err(|e| {
        tracing::error!("Export COPY failed: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "COPY 失败".to_string())
    })?;

    // tokio_postgres 的 copy_out 流产出 Bytes，直接转 axum Body。
    let mapped = stream.map(|res| res.map_err(std::io::Error::other));
    let body = Body::from_stream(mapped);
    Ok((body, "text/csv; charset=utf-8", "export.csv".to_string()))
}

/// 单个响应块的字节上限；缓冲达到后即推给客户端。
const SQL_EXPORT_CHUNK_BYTES: usize = 64 * 1024;

/// SQL 导出：逐行拼 INSERT（纯数据，不含 DDL）。
///
/// 流式读取：行在只读事务中逐批产出、按块推送，大表不再整表进内存。与 CSV 相同，
/// 慢消费者可能触发 `statement_timeout` 导致下载被截断；客户端中途断开时任务结束，
/// 事务随之丢弃并回滚。
///
/// 单元格一律取数据库的文本表示（[`text_cast_query`]），timestamptz、uuid、数组、
/// jsonb、bytea 等类型原样导出，回灌时由目标列解析。
async fn export_sql(
    source: ExportSource,
    include_columns: bool,
) -> Result<(Body, &'static str, String), (StatusCode, String)> {
    let mut client = crate::db::pool::get_conn()
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "数据库不可用".to_string()))?;

    // 事务与行流都借用连接，必须在同一个任务里；任务起步结果经 oneshot 回报，
    // 这样查询失败仍能在产生响应前返回错误。
    let (ready_tx, ready_rx) =
        tokio::sync::oneshot::channel::<Result<(), (StatusCode, &'static str)>>();
    let (tx_chunks, rx_chunks) =
        tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(8);
    const QUERY_FAILED: (StatusCode, &str) = (StatusCode::INTERNAL_SERVER_ERROR, "查询失败");

    tokio::spawn(async move {
        let no_params = || std::iter::empty::<&(dyn tokio_postgres::types::ToSql + Sync)>();
        // 纵深防御：AST 校验之外再放进只读事务，查询里若混入写操作由数据库拒绝。
        // 事务未提交即被丢弃时 tokio_postgres 会发送 ROLLBACK，连接归还池时不残留事务。
        let tx = match client.build_transaction().read_only(true).start().await {
            Ok(tx) => tx,
            Err(e) => {
                tracing::error!("Export transaction failed: {}", e);
                let _ = ready_tx.send(Err(QUERY_FAILED));
                return;
            }
        };
        // 先只做预备拿到原始列名与类型：类型决定字面量写法，列名进 INSERT 列清单。
        let stmt = match tx.prepare(&source.sql).await {
            Ok(stmt) => stmt,
            Err(e) => {
                tracing::error!("Export query failed: {}", e);
                let _ = ready_tx.send(Err(QUERY_FAILED));
                return;
            }
        };
        let types: Vec<Type> = stmt.columns().iter().map(|c| c.type_().clone()).collect();
        let col_clause = if include_columns && !types.is_empty() {
            let names: Vec<&str> = stmt.columns().iter().map(|c| c.name()).collect();
            format!("({})", names.join(", "))
        } else {
            String::new()
        };
        let rows = if source.query_only && !types.is_empty() {
            tx.query_raw(
                text_cast_query(&source.sql, types.len()).as_str(),
                no_params(),
            )
            .await
        } else if types.iter().all(<&str as FromSql>::accepts) {
            // EXPLAIN 不能做子查询；它的 text 列可直接按文本读出。
            tx.query_raw(&stmt, no_params()).await
        } else {
            let _ = ready_tx.send(Err((
                StatusCode::BAD_REQUEST,
                "该语句的结果列无法导出为 SQL",
            )));
            return;
        };
        let rows = match rows {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("Export query failed: {}", e);
                let _ = ready_tx.send(Err(QUERY_FAILED));
                return;
            }
        };
        let _ = ready_tx.send(Ok(()));

        let mut rows = std::pin::pin!(rows);
        let mut buf = String::from("-- Yggdrasil 数据导出（仅数据，不含 schema）\n");
        while let Some(row) = rows.next().await {
            let vals = match row.and_then(|r| row_literals(&r, &types)) {
                Ok(vals) => vals,
                Err(e) => {
                    // 与 CSV 中途出错一样截断下载。
                    tracing::error!("Export row failed: {e}");
                    let _ = tx_chunks.send(Err(std::io::Error::other("查询失败"))).await;
                    return;
                }
            };
            buf.push_str(&format!(
                "INSERT INTO {} {} VALUES ({});\n",
                source.table_name,
                col_clause,
                vals.join(", ")
            ));
            if buf.len() >= SQL_EXPORT_CHUNK_BYTES {
                let chunk = axum::body::Bytes::from(std::mem::take(&mut buf));
                // 客户端断开：直接结束，事务随 drop 回滚。
                if tx_chunks.send(Ok(chunk)).await.is_err() {
                    return;
                }
            }
        }
        if !buf.is_empty()
            && tx_chunks
                .send(Ok(axum::body::Bytes::from(buf)))
                .await
                .is_err()
        {
            return;
        }
        if let Err(e) = tx.rollback().await {
            tracing::error!("Export transaction rollback failed: {e}");
        }
    });

    match ready_rx.await {
        Ok(Ok(())) => {}
        Ok(Err((status, msg))) => return Err((status, msg.to_string())),
        Err(_) => return Err((QUERY_FAILED.0, QUERY_FAILED.1.to_string())),
    }

    let body = Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx_chunks));
    Ok((
        body,
        "application/sql; charset=utf-8",
        "export.sql".to_string(),
    ))
}

/// 把查询包进子查询，按位置给列起别名后逐列 `::text`，由数据库输出任意类型的文本表示；
/// 按位置取列，重名列也不冲突。
fn text_cast_query(sql: &str, columns: usize) -> String {
    let aliases: Vec<String> = (1..=columns).map(|i| format!("c{i}")).collect();
    let casts: Vec<String> = aliases.iter().map(|a| format!("q.{a}::text")).collect();
    format!(
        "SELECT {} FROM ({}) AS q({})",
        casts.join(", "),
        trim_statement_tail(sql),
        aliases.join(", ")
    )
}

/// 去掉语句末尾的分号、空白与注释：嵌进子查询后，末尾的 `;` 或行注释会破坏外层语法。
fn trim_statement_tail(sql: &str) -> &str {
    use sqlparser::tokenizer::{Token, Tokenizer};
    let dialect = sqlparser::dialect::PostgreSqlDialect {};
    let Ok(tokens) = Tokenizer::new(&dialect, sql).tokenize_with_location() else {
        return sql;
    };
    let Some(last) = tokens
        .iter()
        .rev()
        .find(|t| !matches!(t.token, Token::Whitespace(_) | Token::SemiColon))
    else {
        return "";
    };
    // 分词器的位置按字符计列、遇 '\n' 换行，结束位置不含该字符。
    let end = (last.span.end.line, last.span.end.column);
    let (mut line, mut column) = (1, 1);
    for (i, c) in sql.char_indices() {
        if (line, column) == end {
            return &sql[..i];
        }
        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    sql
}

/// 一行文本单元格转成 SQL 字面量。
fn row_literals(
    row: &tokio_postgres::Row,
    types: &[Type],
) -> Result<Vec<String>, tokio_postgres::Error> {
    types
        .iter()
        .enumerate()
        .map(|(i, ty)| Ok(sql_literal(ty, row.try_get::<_, Option<&str>>(i)?)))
        .collect()
}

/// 把单元格的文本表示转成 SQL 字面量：NULL 原样；布尔写 TRUE/FALSE；有限数值不加引号；
/// 其余（含 NaN/Infinity）单引号转义，插入时由目标列类型解析。
fn sql_literal(ty: &Type, text: Option<&str>) -> String {
    let Some(s) = text else {
        return "NULL".into();
    };
    match ty.name() {
        // `bool::text` 输出 true/false，布尔输出函数为 t/f。
        "bool" => if s.starts_with('t') { "TRUE" } else { "FALSE" }.into(),
        "int2" | "int4" | "int8" | "oid" | "float4" | "float8" | "numeric"
            if s.parse::<f64>().is_ok_and(f64::is_finite) =>
        {
            s.into()
        }
        _ => format!("'{}'", s.replace('\'', "''")),
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    // ---- is_simple_ident：表名白名单（防 SQL 注入与路径穿越）----

    #[test]
    fn simple_ident_accepts_legal_names() {
        assert!(is_simple_ident("posts"));
        assert!(is_simple_ident("user_posts"));
        assert!(is_simple_ident("table1"));
        assert!(is_simple_ident("a"));
        assert!(is_simple_ident("_private"));
        assert!(is_simple_ident("ABC123xyz"));
    }

    #[test]
    fn simple_ident_rejects_empty_and_whitespace() {
        assert!(!is_simple_ident(""));
        assert!(!is_simple_ident("   "));
    }

    #[test]
    fn simple_ident_rejects_injection_vectors() {
        // SQL 注入：引号、分号、注释、空格分隔
        assert!(!is_simple_ident("posts; DROP TABLE users"));
        assert!(!is_simple_ident("posts' OR '1'='1"));
        assert!(!is_simple_ident("posts--"));
        assert!(!is_simple_ident("public.posts"));
        assert!(!is_simple_ident("posts where 1=1"));
        // 路径穿越
        assert!(!is_simple_ident("../etc/passwd"));
        assert!(!is_simple_ident("..\\windows"));
        assert!(!is_simple_ident("/etc/passwd"));
        // 反引号 / 双引号标识符语法
        assert!(!is_simple_ident("`posts`"));
        assert!(!is_simple_ident("\"posts\""));
        // 连字符（常见合法表名片段，但本白名单禁止，防 `a-b` 被 SQL 解析为减法）
        assert!(!is_simple_ident("my-posts"));
    }

    #[test]
    fn simple_ident_rejects_unicode_and_non_ascii() {
        assert!(!is_simple_ident("文章"));
        assert!(!is_simple_ident("posts²"));
        assert!(!is_simple_ident("café"));
    }

    // ---- parse_source：来源解析 + 只读校验（安全入口）----

    #[test]
    fn parse_table_source_builds_select_star() {
        let source = parse_source("table:posts").unwrap();
        assert_eq!(source.sql, "SELECT * FROM \"posts\"");
        assert_eq!(source.table_name, "posts");
        assert!(source.query_only);
    }

    #[test]
    fn parse_table_source_trims_whitespace() {
        let source = parse_source("table:  posts  ").unwrap();
        assert_eq!(source.sql, "SELECT * FROM \"posts\"");
        assert_eq!(source.table_name, "posts");
    }

    #[test]
    fn parse_table_source_rejects_bad_name() {
        // 非法表名（注入/穿越/空）必须在拼进 SQL 前被拒，防 SQL 注入。
        for bad in [
            "table:",
            "table:   ",
            "table:posts; DROP TABLE users",
            "table:../secret",
            "table:a b",
            "table:\"x\"",
        ] {
            let (code, msg) = parse_source(bad).expect_err(bad);
            assert_eq!(code, StatusCode::BAD_REQUEST);
            assert!(msg.contains("表名"), "{bad}: {msg}");
        }
    }

    #[test]
    fn parse_query_source_accepts_select_and_explain() {
        for query in [
            "query:SELECT * FROM posts",
            "query:SELECT id, title FROM posts WHERE id > 0",
            "query:SELECT 1",
            // 复杂但只读的查询
            "query:WITH t AS (SELECT 1) SELECT * FROM t",
        ] {
            assert!(parse_source(query).unwrap().query_only, "{query}");
        }
        // EXPLAIN 放行，但不能包进子查询做逐列文本转换。
        assert!(
            !parse_source("query:EXPLAIN SELECT * FROM posts")
                .unwrap()
                .query_only
        );
    }

    #[test]
    fn parse_query_source_rejects_all_write_operations() {
        // 每一种写/破坏操作都必须被 AST 校验拦截——这是数据导出的核心安全不变量。
        for evil in [
            "query:INSERT INTO posts VALUES (1)",
            "query:UPDATE posts SET title='x'",
            "query:DELETE FROM posts",
            "query:DROP TABLE posts",
            "query:TRUNCATE posts",
            "query:CREATE TABLE evil (id int)",
            "query:ALTER TABLE posts ADD COLUMN x int",
            "query:GRANT SELECT ON posts TO public",
            // 即便嵌在 SELECT 里，子查询的写操作也要拒
            "query:INSERT INTO logs VALUES (1) RETURNING 1",
        ] {
            let (code, msg) = parse_source(evil).expect_err(evil);
            assert_eq!(code, StatusCode::BAD_REQUEST, "{evil}");
            // 写操作要么被 AST 校验拒（"只读"），要么 sqlparser 解析阶段就失败
            assert!(
                msg.contains("只读") || msg.contains("解析失败"),
                "{evil}: {msg}"
            );
        }
    }

    #[test]
    fn parse_query_source_rejects_explain_analyze_and_hidden_writes() {
        // EXPLAIN ANALYZE 会真正执行内层语句；data-modifying CTE、SELECT INTO、
        // 行锁子句外形都是 SELECT，但都有副作用。
        for evil in [
            "query:EXPLAIN ANALYZE DELETE FROM posts",
            "query:EXPLAIN ANALYZE DELETE FROM posts WHERE id=1",
            "query:EXPLAIN (ANALYZE) DELETE FROM posts WHERE id = 1",
            "query:EXPLAIN (ANALYZE true) SELECT 1",
            "query:EXPLAIN DELETE FROM posts WHERE id = 1",
            "query:WITH d AS (DELETE FROM posts RETURNING *) SELECT * FROM d",
            "query:SELECT * INTO t FROM posts",
            "query:SELECT * FROM posts FOR UPDATE",
        ] {
            let (code, msg) = parse_source(evil).expect_err(evil);
            assert_eq!(code, StatusCode::BAD_REQUEST, "{evil}");
            assert!(msg.contains("只读"), "{evil}: {msg}");
        }
    }

    #[test]
    fn parse_query_source_accepts_plain_explain_options_and_ctes() {
        for ok in [
            "query:SELECT 1",
            "query:EXPLAIN SELECT * FROM posts",
            "query:EXPLAIN (FORMAT JSON) SELECT * FROM posts",
            "query:WITH t AS (SELECT id FROM posts) SELECT * FROM t",
        ] {
            assert!(parse_source(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn parse_query_source_rejects_unparseable_sql() {
        for bad in [
            "query:这不是SQL",
            "query:SELECT FROM",
            "query:@#$%",
            "query:1+1",
        ] {
            assert!(parse_source(bad).is_err(), "{bad} 应解析失败或被拒");
        }
    }

    #[test]
    fn parse_query_source_allows_empty_result_set() {
        // sqlparser 容忍纯分号（解析为零语句），`any()` 在空集上为 false → 放行。
        // 锁定该契约：这只产生空导出，无数据泄露，属可接受行为；
        // 若未来想收紧为"空查询拒绝"，此测试会提醒你更新。
        assert!(parse_source("query:;;;").is_ok());
    }

    #[test]
    fn parse_source_rejects_unknown_prefix() {
        let (code, msg) = parse_source("unknown:posts").expect_err("未知前缀");
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(msg.contains("source"));
        // 完全无前缀的裸字符串也应被拒
        assert!(parse_source("posts").is_err());
        assert!(parse_source("").is_err());
    }

    // ---- SQL 导出：单元格字面量 ----

    #[test]
    fn sql_literal_keeps_values_parseable() {
        let lit = |ty: Type, s: Option<&str>| sql_literal(&ty, s);
        assert_eq!(lit(Type::TIMESTAMPTZ, None), "NULL");
        assert_eq!(lit(Type::BOOL, Some("true")), "TRUE");
        assert_eq!(lit(Type::BOOL, Some("f")), "FALSE");
        assert_eq!(lit(Type::INT8, Some("-42")), "-42");
        assert_eq!(lit(Type::FLOAT8, Some("1e+20")), "1e+20");
        assert_eq!(lit(Type::NUMERIC, Some("1.50")), "1.50");
        // 非有限数值不是合法的裸字面量，必须加引号。
        assert_eq!(lit(Type::FLOAT8, Some("NaN")), "'NaN'");
        assert_eq!(lit(Type::FLOAT4, Some("-Infinity")), "'-Infinity'");
        assert_eq!(lit(Type::NUMERIC, Some("Infinity")), "'Infinity'");
        assert_eq!(lit(Type::TEXT, Some("it's")), "'it''s'");
        assert_eq!(lit(Type::TEXT, Some("42")), "'42'");
        assert_eq!(
            lit(Type::TIMESTAMPTZ, Some("2026-11-03 04:34:56.789+00")),
            "'2026-11-03 04:34:56.789+00'"
        );
    }

    #[test]
    fn trim_statement_tail_drops_only_trailing_terminators() {
        for (sql, want) in [
            ("SELECT 1", "SELECT 1"),
            ("SELECT 1;", "SELECT 1"),
            ("SELECT 1 ; ;\n", "SELECT 1"),
            ("SELECT 1; -- 注释\n", "SELECT 1"),
            ("SELECT 1 /* 尾注 */ ;", "SELECT 1"),
            ("SELECT '中文;'\n  FROM t -- x", "SELECT '中文;'\n  FROM t"),
            // 字符串里的分号、注释符号不算语句结尾。
            ("SELECT ';' AS s;", "SELECT ';' AS s"),
            ("SELECT 'a--b'", "SELECT 'a--b'"),
            (";;", ""),
        ] {
            assert_eq!(trim_statement_tail(sql), want, "{sql:?}");
        }
    }

    #[test]
    #[ignore = "requires DATABASE_URL; uses a connection-local temporary table"]
    fn export_sql_database_round_trips_typed_values() {
        crate::db::TEST_DATABASE_RUNTIME.block_on(export_sql_round_trip_impl());
    }

    async fn export_sql_body(
        source: &str,
        include_columns: bool,
    ) -> Result<String, (StatusCode, String)> {
        let (body, _, _) = export_sql(parse_source(source).unwrap(), include_columns).await?;
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        Ok(String::from_utf8(bytes.to_vec()).unwrap())
    }

    /// 导出的 INSERT 回灌到同结构的表后逐列不变；非标量列不能被写成 NULL。
    async fn export_sql_round_trip_impl() {
        let _guard = crate::db::TEST_DATABASE_LOCK.lock().await;
        const ROW: &str = "SELECT TIMESTAMPTZ '2026-11-03 12:34:56.789+08' AS created_at, \
             '0f8fad5b-d9cb-469f-a165-70867728950e'::uuid AS id, \
             ARRAY['a', 'b''c', NULL] AS tags, '{\"k\": [1, 2]}'::jsonb AS meta, \
             '\\x00ff'::bytea AS raw, DATE '2026-11-03' AS day, \
             INTERVAL '1 day 2 hours' AS span, 'NaN'::float8 AS nan, 0.1::float4 AS f4, \
             12345678901234567890.5 AS big, true AS ok, 'it''s' AS title, NULL::int AS missing";
        let client = crate::db::pool::get_conn().await.unwrap();
        client
            .batch_execute(&format!("CREATE TEMP TABLE export AS {ROW} WITH NO DATA"))
            .await
            .unwrap();
        // 末尾分号与行注释不影响子查询包装。
        let dump = export_sql_body(&format!("query:{ROW}; -- 注释\n"), true)
            .await
            .unwrap();
        client.batch_execute(&dump).await.unwrap();
        let exported: String = client
            .query_one("SELECT e::text FROM export e", &[])
            .await
            .unwrap()
            .get(0);
        let expected: String = client
            .query_one(&format!("SELECT o::text FROM ({ROW}) o"), &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(exported, expected, "{dump}");
        client.batch_execute("DROP TABLE export").await.unwrap();

        // 重名列按位置取值。
        let dump = export_sql_body("query:SELECT 1 AS a, 'x' AS a", false)
            .await
            .unwrap();
        assert!(dump.contains(" VALUES (1, 'x');\n"), "{dump}");

        // EXPLAIN 不能包成子查询：文本格式直接导出，非文本格式明确拒绝。
        let plan = export_sql_body("query:EXPLAIN SELECT 1", true)
            .await
            .unwrap();
        assert!(plan.contains(" VALUES ('Result"), "{plan}");
        let (code, _) = export_sql_body("query:EXPLAIN (FORMAT JSON) SELECT 1", true)
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }
}
