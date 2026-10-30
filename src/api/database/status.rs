#![allow(clippy::unused_unit, deprecated)]

//! 数据库运行状态聚合查询（只读）。
//!
//! 全部查询走 `pg_catalog` / `pg_stat_*` / `schema_migrations`，零写、零风险。
//! [`get_db_status`][crate::api::database::status::get_db_status] 在一次 server function 调用里聚合多组数据返回。

use crate::api::error::ApiError;
use chrono::{DateTime, Utc};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

// 仅 server 构建用到：admin 鉴权 + DB 查询。WASM 侧的 server-function 客户端桩
// 不解析这些符号，必须 gate 以避免在非 server 构建里找不到 server-only 符号。
#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;

/// 数据库状态聚合数据。
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct DbStatus {
    /// 当前数据库总大小（字节）。
    pub db_size_bytes: i64,
    /// 当前数据库的活跃连接数。
    pub total_connections: i32,
    /// PG 配置的最大连接数（`max_connections`）。
    pub max_connections: i32,
    /// 已应用的最新迁移版本（`schema_migrations.version`）。
    pub migration_version: Option<String>,
    /// 最新迁移的应用时间。
    #[cfg_attr(
        target_arch = "wasm32",
        serde(
            default,
            deserialize_with = "crate::models::rfc3339::deserialize_option"
        )
    )]
    pub migration_applied_at: Option<DateTime<Utc>>,
    /// 用户表清单（按总大小降序）。
    pub tables: Vec<TableInfo>,
    /// 索引占用 Top N。
    pub top_indexes: Vec<IndexInfo>,
    /// 活跃连接列表（已过滤掉自身这条查询）。
    pub active_connections: Vec<ConnInfo>,
}

/// 单张表的统计信息。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    /// 行数：total_size 小于阈值时为真实 COUNT(*)，否则回退 reltuples 估算（见 row_count_estimated）。
    pub row_count: i64,
    /// true 表示 row_count 是 reltuples 估算值（大表回退，未 ANALYZE 时 reltuples=-1 会显示为估算）。
    pub row_count_estimated: bool,
    pub table_size_bytes: i64,
    pub index_size_bytes: i64,
    pub total_size_bytes: i64,
    #[cfg_attr(
        target_arch = "wasm32",
        serde(
            default,
            deserialize_with = "crate::models::rfc3339::deserialize_option"
        )
    )]
    pub last_vacuum: Option<DateTime<Utc>>,
    #[cfg_attr(
        target_arch = "wasm32",
        serde(
            default,
            deserialize_with = "crate::models::rfc3339::deserialize_option"
        )
    )]
    pub last_analyze: Option<DateTime<Utc>>,
    pub dead_tuples: i64,
    pub live_tuples: i64,
}

/// 索引占用信息。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IndexInfo {
    pub name: String,
    pub table_name: String,
    pub size_bytes: i64,
}

/// 单条活跃连接信息。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConnInfo {
    pub pid: i32,
    pub user: String,
    pub state: Option<String>,
    pub query: Option<String>,
    /// 当前查询已运行秒数（无查询时为 None）。
    pub query_duration_secs: Option<f64>,
}

/// 获取数据库运行状态（只读，管理员）。
#[server(GetDbStatus, endpoint = "get_db_status", input = leptos::server_fn::codec::Json)]
pub async fn get_db_status() -> Result<DbStatus, ApiError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;

        // 数据库总大小
        let db_size: i64 = client
            .query_one("SELECT pg_database_size(current_database())", &[])
            .await
            .map_err(AppError::query)?
            .get(0);

        // 当前库连接数 + 全局最大连接数。
        // count(*) 原生返回 bigint(int8)，与下方 setting::int 一并显式转 int4，
        // 以匹配 total_connections/max_connections 的 i32 类型（否则 FromSql 反序列化失败）。
        let conn_row = client
            .query_one(
                "SELECT count(*)::int, \
                 (SELECT setting::int FROM pg_settings WHERE name = 'max_connections') \
                 FROM pg_stat_activity WHERE datname = current_database()",
                &[],
            )
            .await
            .map_err(AppError::query)?;
        let total_conn: i32 = conn_row.get(0);
        let max_conn: i32 = conn_row.get(1);

        // 最新迁移版本（schema_migrations 由 migrate.rs 创建）
        let migration = client
            .query_opt(
                "SELECT version, applied_at FROM schema_migrations \
                 ORDER BY applied_at DESC LIMIT 1",
                &[],
            )
            .await
            .map_err(AppError::query)?;
        let (migration_version, migration_applied_at) = match migration {
            Some(row) => (Some(row.get(0)), Some(row.get(1))),
            None => (None, None),
        };

        // 表清单：大小/统计走 catalog；行数对小表用真实 COUNT(*)，大表回退 reltuples 估算。
        // 阈值 100MB：超过则 COUNT(*) 成本过高（全表扫描），降级为估算值并标注。
        const COUNT_SIZE_THRESHOLD: i64 = 100 * 1024 * 1024;
        let table_rows = client
            .query(
                "SELECT c.relname, c.reltuples::bigint, pg_relation_size(c.oid), \
                 pg_total_relation_size(c.oid) - pg_relation_size(c.oid), \
                 pg_total_relation_size(c.oid), s.last_vacuum, s.last_analyze, \
                 s.n_dead_tup, s.n_live_tup \
                 FROM pg_class c \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid \
                 WHERE c.relkind = 'r' AND n.nspname = 'public' \
                 ORDER BY pg_total_relation_size(c.oid) DESC",
                &[],
            )
            .await
            .map_err(AppError::query)?;
        let tables = {
            let mut out = Vec::with_capacity(table_rows.len());
            for r in table_rows {
                let name: String = r.get(0);
                let reltuples: i64 = r.get(1);
                let total_size: i64 = r.get(4);
                // 小表：真实 COUNT(*)；大表：回退 reltuples 估算。
                // relname 来自 pg_class 可信，但表名可能含大写/特殊字符，需 PG 标识符转义（双引号包裹，内部双引号双写）。
                let (row_count, estimated) = if total_size < COUNT_SIZE_THRESHOLD {
                    let ident = format!("\"{}\"", name.replace('"', "\"\""));
                    let cnt: i64 = client
                        .query_one(&format!("SELECT count(*)::bigint FROM {}", ident), &[])
                        .await
                        .map_err(AppError::query)?
                        .get(0);
                    (cnt, false)
                } else {
                    (reltuples, true)
                };
                out.push(TableInfo {
                    name,
                    row_count,
                    row_count_estimated: estimated,
                    table_size_bytes: r.get(2),
                    index_size_bytes: r.get(3),
                    total_size_bytes: total_size,
                    last_vacuum: r.get(5),
                    last_analyze: r.get(6),
                    dead_tuples: r.get(7),
                    live_tuples: r.get(8),
                });
            }
            out
        };

        // 索引占用 Top 10
        let index_rows = client
            .query(
                "SELECT c.relname AS index_name, cl.relname AS table_name, \
                 pg_relation_size(c.oid) \
                 FROM pg_class c \
                 JOIN pg_index i ON i.indexrelid = c.oid \
                 JOIN pg_class cl ON cl.oid = i.indrelid \
                 JOIN pg_namespace n ON n.oid = cl.relnamespace \
                 WHERE n.nspname = 'public' \
                 ORDER BY pg_relation_size(c.oid) DESC LIMIT 10",
                &[],
            )
            .await
            .map_err(AppError::query)?;
        let top_indexes = index_rows
            .into_iter()
            .map(|r| IndexInfo {
                name: r.get(0),
                table_name: r.get(1),
                size_bytes: r.get(2),
            })
            .collect();

        // 活跃连接（过滤自身 pid，避免循环显示）。
        // extract(epoch FROM ...) 原生返回 numeric(decimal)，tokio-postgres 无 FromSql<f64>
        // 实现该类型；显式 ::double precision 转 float8 以匹配 query_duration_secs 的 f64。
        let conn_rows = client
            .query(
                "SELECT pid, usename, state, query, \
                 extract(epoch FROM now() - query_start)::double precision \
                 FROM pg_stat_activity \
                 WHERE datname = current_database() AND pid <> pg_backend_pid() \
                 ORDER BY query_start DESC NULLS LAST LIMIT 50",
                &[],
            )
            .await
            .map_err(AppError::query)?;
        let active_connections = conn_rows
            .into_iter()
            .map(|r| ConnInfo {
                pid: r.get(0),
                user: r.get::<_, Option<String>>(1).unwrap_or_default(),
                state: r.get(2),
                query: r.get(3),
                query_duration_secs: r.get(4),
            })
            .collect();

        Ok(DbStatus {
            db_size_bytes: db_size,
            total_connections: total_conn,
            max_connections: max_conn,
            migration_version,
            migration_applied_at,
            tables,
            top_indexes,
            active_connections,
        })
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(DbStatus::default())
    }
}
