//! 数据库连接模块。
//!
//! 连接池、重试和迁移仅在 `server` feature 下编译；WASM 前端无需数据库占位实现。

/// 错误格式化工具：把 `std::error::Error` 的 source 链完整展开为字符串。
///
/// 存在的原因：`tokio_postgres::Error` 的 `Display` 对 DB 侧错误只会打印
/// 无信息量的占位串 `db error`，真正的消息文本（如
/// `column "x" of relation "y" already exists`、SQLSTATE、约束名）藏在
/// `source()` 链里的 `postgres::error::DbError`。不主动遍历链，日志和错误
/// 字符串就会全部折叠成 `db error`，无法定位失败原因。
///
/// 用法：`format!("...: {}", format_with_sources(&e))` 或直接
/// `format_with_sources(&e)` 得到完整的 `e: cause: deeper cause`。
#[cfg(feature = "server")]
pub fn format_with_sources(e: &dyn std::error::Error) -> String {
    use std::fmt::Write;
    let mut s = e.to_string();
    let mut cur: &dyn std::error::Error = e;
    while let Some(next) = cur.source() {
        // 跳过与外层 Display 完全相同的占位层（如 tokio_postgres 的 `db error`），
        // 避免输出 `db error: db error` 这种重复。只在能带来新信息时追加。
        let next_disp = next.to_string();
        if !next_disp.is_empty() && next_disp != s {
            let _ = write!(s, ": {next_disp}");
        }
        cur = next;
    }
    s
}

/// 真实的 PostgreSQL 连接池实现，仅在启用 server feature 时编译。
#[cfg(feature = "server")]
pub mod pool;

/// 连接获取的指数退避重试策略，仅在启用 server feature 时编译。
#[cfg(feature = "server")]
pub mod retry;

/// 数据库迁移运行器，仅在启用 server feature 时编译。
#[cfg(feature = "server")]
pub mod migrate;
