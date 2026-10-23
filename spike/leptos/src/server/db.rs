//! PostgreSQL 连接池：沿用主 crate `db::pool::DB_POOL` 的全局 LazyLock 约定。
//!
//! 原型读 `SPIKE_DATABASE_URL`（缺省回落 `DATABASE_URL`），应指向一份可写的
//! 一次性副本，评论写入与审核不会碰开发库。

use std::sync::LazyLock;

use deadpool_postgres::{Manager, ManagerConfig, Object, Pool, RecyclingMethod};
use tokio_postgres::NoTls;

use crate::error::AppError;

pub static DB_POOL: LazyLock<Pool> = LazyLock::new(|| {
    let url = std::env::var("SPIKE_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("SPIKE_DATABASE_URL 或 DATABASE_URL 必须设置");
    let config: tokio_postgres::Config = url.parse().expect("数据库 URL 无法解析");
    let manager = Manager::from_config(
        config,
        NoTls,
        ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        },
    );
    Pool::builder(manager)
        .max_size(8)
        .build()
        .expect("连接池创建失败")
});

pub async fn get_conn() -> Result<Object, AppError> {
    DB_POOL
        .get()
        .await
        .map_err(|e| AppError::internal("获取数据库连接", e))
}

/// `tokio_postgres::Error` → 脱敏内部错误。
pub fn query_err(e: tokio_postgres::Error) -> AppError {
    AppError::internal("数据库查询", e)
}
