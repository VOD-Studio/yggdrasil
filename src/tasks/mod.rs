//! 后台任务调度入口。
//!
//! 所有任务仅在 `server` feature 启用时编译，运行在服务端独立的 tokio 任务中。

/// 每天定时自动备份数据库与 uploads 素材，含自动轮转。
#[cfg(feature = "server")]
pub mod backup;
/// 定时清理图片磁盘缓存，避免缓存目录无限增长。
#[cfg(feature = "server")]
pub mod image_cache_cleanup;
/// 定时清理评论过期的 IP 与用户代理信息，满足隐私保护要求。
#[cfg(feature = "server")]
pub mod ip_purge;
/// 运行日志保留策略裁剪（龄期 + 行数上限），每小时执行。
#[cfg(feature = "server")]
pub mod log_purge;
/// tracing 捕获日志的攒批落库 writer（capture mpsc → logs 表）。
#[cfg(feature = "server")]
pub mod log_writer;
/// 定时物理删除无引用的孤儿素材（评论区匿名传图的主要回收手段）。
#[cfg(feature = "server")]
pub mod orphan_asset_purge;
/// 定时清理回收站中超过保留期的已删除文章。
#[cfg(feature = "server")]
pub mod post_purge;
/// 定时删除已过期会话，避免 `sessions` 表无限增长。
#[cfg(feature = "server")]
pub mod session_cleanup;
/// sysinfo 主机指标（CPU/内存/磁盘）后台采样，server function 只读快照。
#[cfg(feature = "server")]
pub mod sysinfo_sampler;
