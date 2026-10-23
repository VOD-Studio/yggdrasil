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
    let (source_sql, table_name) = parse_source(&params.source)?;

    match params.format.as_str() {
        "csv" => export_csv(source_sql, include_columns).await,
        "sql" => export_sql(source_sql, include_columns, table_name).await,
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

/// 解析导出来源，返回（内部 SQL，表名）。
/// - `table:posts` → 校验表名合法后，`SELECT * FROM "posts"`（只读）+ 表名 "posts"。
/// - `query:SELECT ...` → 校验为只读语句后原样返回，表名为 "export"。
fn parse_source(source: &str) -> Result<(String, String), (StatusCode, String)> {
    if let Some(table) = source.strip_prefix("table:") {
        // 表名白名单：仅允许标识符字符，防注入
        let t = table.trim();
        if t.is_empty() || !is_simple_ident(t) {
            return Err((StatusCode::BAD_REQUEST, "无效的表名".to_string()));
        }
        Ok((format!("SELECT * FROM \"{}\"", t), t.to_string()))
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
        Ok((query.to_string(), "export".to_string()))
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
async fn export_sql(
    source_sql: String,
    include_columns: bool,
    table_name: String,
) -> Result<(Body, &'static str, String), (StatusCode, String)> {
    let mut client = crate::db::pool::get_conn()
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "数据库不可用".to_string()))?;

    // 事务与行流都借用连接，必须在同一个任务里；任务起步结果经 oneshot 回报，
    // 这样查询失败仍能在产生响应前返回 500。
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<Result<(), ()>>();
    let (tx_chunks, rx_chunks) =
        tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(8);

    tokio::spawn(async move {
        // 纵深防御：AST 校验之外再放进只读事务，查询里若混入写操作由数据库拒绝。
        // 事务未提交即被丢弃时 tokio_postgres 会发送 ROLLBACK，连接归还池时不残留事务。
        let tx = match client.build_transaction().read_only(true).start().await {
            Ok(tx) => tx,
            Err(e) => {
                tracing::error!("Export transaction failed: {}", e);
                let _ = ready_tx.send(Err(()));
                return;
            }
        };
        let rows = match tx
            .query_raw(
                source_sql.as_str(),
                std::iter::empty::<&(dyn tokio_postgres::types::ToSql + Sync)>(),
            )
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("Export query failed: {}", e);
                let _ = ready_tx.send(Err(()));
                return;
            }
        };
        let _ = ready_tx.send(Ok(()));

        let mut rows = std::pin::pin!(rows);
        let mut buf = String::from("-- Yggdrasil 数据导出（仅数据，不含 schema）\n");
        let mut col_clause: Option<String> = None;
        while let Some(row) = rows.next().await {
            let r = match row {
                Ok(r) => r,
                Err(e) => {
                    // 与 CSV 中途出错一样截断下载。
                    tracing::error!("Export row failed: {e}");
                    let _ = tx_chunks.send(Err(std::io::Error::other("查询失败"))).await;
                    return;
                }
            };
            // 列名取自首行；include_columns 关闭或无列时不带列清单。
            let col_clause = col_clause.get_or_insert_with(|| {
                if include_columns && !r.columns().is_empty() {
                    let names: Vec<&str> = r.columns().iter().map(|c| c.name()).collect();
                    format!("({})", names.join(", "))
                } else {
                    String::new()
                }
            });
            let vals: Vec<String> = (0..r.len()).map(|i| sql_quote_cell(&r, i)).collect();
            buf.push_str(&format!(
                "INSERT INTO {} {} VALUES ({});\n",
                table_name,
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
        _ => return Err((StatusCode::INTERNAL_SERVER_ERROR, "查询失败".to_string())),
    }

    let body = Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx_chunks));
    Ok((
        body,
        "application/sql; charset=utf-8",
        "export.sql".to_string(),
    ))
}

/// 把一个单元格值转成 SQL 字面量（字符串单引号转义，数字原样，NULL）。
fn sql_quote_cell(row: &tokio_postgres::Row, idx: usize) -> String {
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
            .map(|v| v.to_string())
            .unwrap_or_else(|| "NULL".into()),
        "int4" => row
            .try_get::<_, Option<i32>>(idx)
            .ok()
            .flatten()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "NULL".into()),
        "int8" => row
            .try_get::<_, Option<i64>>(idx)
            .ok()
            .flatten()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "NULL".into()),
        "float4" | "float8" => row
            .try_get::<_, Option<f64>>(idx)
            .ok()
            .flatten()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "NULL".into()),
        "bool" => row
            .try_get::<_, Option<bool>>(idx)
            .ok()
            .flatten()
            .map(|v| if v { "TRUE" } else { "FALSE" }.to_string())
            .unwrap_or_else(|| "NULL".into()),
        _ => {
            // 字符串/时间戳等：单引号转义
            match row.try_get::<_, Option<String>>(idx) {
                Ok(Some(s)) => format!("'{}'", s.replace('\'', "''")),
                _ => "NULL".into(),
            }
        }
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
        let (sql, name) = parse_source("table:posts").unwrap();
        assert_eq!(sql, "SELECT * FROM \"posts\"");
        assert_eq!(name, "posts");
    }

    #[test]
    fn parse_table_source_trims_whitespace() {
        let (sql, name) = parse_source("table:  posts  ").unwrap();
        assert_eq!(sql, "SELECT * FROM \"posts\"");
        assert_eq!(name, "posts");
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
        assert!(parse_source("query:SELECT * FROM posts").is_ok());
        assert!(parse_source("query:SELECT id, title FROM posts WHERE id > 0").is_ok());
        assert!(parse_source("query:EXPLAIN SELECT * FROM posts").is_ok());
        assert!(parse_source("query:SELECT 1").is_ok());
        // 复杂但只读的查询
        assert!(parse_source("query:WITH t AS (SELECT 1) SELECT * FROM t").is_ok());
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
}
