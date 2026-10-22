//! 数据库管理 server functions 与 Axum 处理器。
//!
//! 按功能拆分子模块：
//! - [`crate::api::database::status`]：数据库运行状态聚合（表/连接/死元组/迁移版本）。

/// 备份/恢复（双模式 + 任务进度）。
pub mod backup;
/// 数据导出 Axum 流式处理器（仅 server：纯 Axum 路由，无 WASM 消费者）。
#[cfg(feature = "server")]
pub mod export;
/// SQL 只读判定（导出与 SQL 控制台共用，仅 server：依赖 sqlparser）。
#[cfg(feature = "server")]
pub(crate) mod read_only;
/// SQL 补全用 schema 拉取。
pub mod schema;
/// SQL 控制台执行（全读写 + 4 护栏）。
pub mod sql_console;
/// 数据库运行状态聚合查询。
pub mod status;
/// 服务器状态聚合查询（应用内 + 主机层）。
pub mod system_status;
/// 备份/恢复异步任务进度表。
pub mod tasks;
