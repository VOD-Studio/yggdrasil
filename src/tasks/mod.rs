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
/// 定时清理回收站中超过保留期的已删除文章。
#[cfg(feature = "server")]
pub mod post_purge;
/// 定时删除已过期会话，避免 `sessions` 表无限增长。
#[cfg(feature = "server")]
pub mod session_cleanup;
