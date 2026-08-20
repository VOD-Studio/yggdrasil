#![allow(clippy::unused_unit, deprecated)]

//! 服务器状态聚合查询（只读）：应用内指标 + 主机层指标。
//!
//! 应用内：DB 连接池、moka 缓存命中率、活跃会话数、进程运行时间。
//! 主机层：sysinfo 后台采样快照（CPU/内存/磁盘/load），由 [`crate::tasks::sysinfo_sampler`] 维护。

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

// admin 鉴权 + AppError 仅在 server 构建里被 server function 体引用。
#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;

/// 单个缓存的统计快照（前端展示用）。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CacheStat {
    pub name: String,
    pub entry_count: u64,
    pub hits: u64,
    pub misses: u64,
    /// 命中率（0.0–1.0）。
    pub hit_rate: f64,
}

/// 服务器状态聚合数据。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerStatus {
    /// 进程运行时间（秒）。
    pub uptime_secs: u64,
    /// DB 连接池总大小（已创建连接数）。
    pub pool_size: usize,
    /// 连接池最大容量。
    pub pool_max_size: usize,
    /// 当前空闲可用连接数。
    pub pool_available: usize,
    /// 正在等待获取连接的请求数。
    pub pool_waiting: usize,
    /// 有效会话数（sessions 表 expires_at > now()）。
    pub active_sessions: i64,
    /// 主机层指标快照（CPU/内存/磁盘等）。
    pub host: crate::models::system::SystemSnapshot,
    /// 各缓存命中率与条目数。
    pub caches: Vec<CacheStat>,
}

/// 进程启动时刻（LazyLock），用于计算运行时间。
#[cfg(feature = "server")]
static STARTED_AT: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);

/// 获取服务器状态（只读，管理员）。
#[server(GetServerStatus, "/api")]
pub async fn get_server_status() -> Result<ServerStatus, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        use crate::cache::cache_stats;
        use crate::db::pool::{get_conn, DB_POOL};

        // 连接池状态
        let pool_status = DB_POOL.status();
        let pool_size = pool_status.size;
        let pool_max_size = pool_status.max_size;
        let pool_available = pool_status.available;
        let pool_waiting = pool_status.waiting;

        // 活跃会话数
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let active_sessions: i64 = client
            .query_one(
                "SELECT count(*) FROM sessions WHERE expires_at > now()",
                &[],
            )
            .await
            .map_err(AppError::query)?
            .get(0);

        // 进程运行时间
        let uptime_secs = STARTED_AT.elapsed().as_secs();

        // 主机层快照
        let host = crate::tasks::sysinfo_sampler::read_snapshot().await;

        // 缓存统计
        let caches = cache_stats()
            .into_iter()
            .map(|s| CacheStat {
                name: s.name.to_string(),
                entry_count: s.entry_count,
                hits: s.hits,
                misses: s.misses,
                hit_rate: s.hit_rate,
            })
            .collect();

        Ok(ServerStatus {
            uptime_secs,
            pool_size,
            pool_max_size,
            pool_available,
            pool_waiting,
            active_sessions,
            host,
            caches,
        })
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(ServerStatus {
            uptime_secs: 0,
            pool_size: 0,
            pool_max_size: 0,
            pool_available: 0,
            pool_waiting: 0,
            active_sessions: 0,
            host: crate::models::system::SystemSnapshot::default(),
            caches: vec![],
        })
    }
}
