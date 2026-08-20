//! 回收站、自动备份、素材上传与站点配置接口。
//!
//! 按子域拆分为目录模块：`trash`（回收站）、`backup`（自动备份）、`security`
//! （安全）、`image_cache`（图片磁盘缓存）、`asset_purge`（孤儿素材清理）、
//! `rate_limit`（限流）、`upload`（素材上传）、`site`（站点公开配置）、
//! `system`（系统启动信息）、`webp`（WebP 编码）、`image_limit`（图片尺寸限制）、
//! `runner`（代码运行器）。全部公开项经本模块 `pub use` 聚合，
//! `crate::api::settings::xxx` 路径保持不变，外部调用零改动。
//!
//! - 回收站配置：读取与更新自动清理设置，需要 admin 权限。
//! - 自动备份配置：读取与更新定时备份设置（含上次结果/下次执行时间），需要 admin。
//!   `load_backup_settings` / `save_last_backup_run` 同时供备份核心与调度任务复用。
//!   同名 `BACKUP_*` 环境变量仅在对应 settings 键缺失时播种初始值（首次部署），
//!   之后以面板写入的 DB 值为准。
//! - 素材上传配置：读取与更新上传弹窗并发数，需要 admin。`UPLOAD_CONCURRENCY`
//!   环境变量播种语义与 `BACKUP_*` 一致（仅键缺失时生效）。
//! - 站点公开配置：页脚 GitHub 链接等，`get_site_settings` 公开读取（前台页脚 SSR），
//!   `update_site_settings` 仅 admin。配置持久化到 settings 键值表。
//! Dioxus server function，注册在 `/api` 路径下。

mod asset_purge;
mod backup;
mod image_cache;
mod image_limit;
mod rate_limit;
mod runner;
mod security;
mod site;
mod system;
mod trash;
mod upload;
mod webp;

pub use asset_purge::*;
pub use backup::*;
pub use image_cache::*;
pub use image_limit::*;
pub use rate_limit::*;
pub use runner::*;
pub use security::*;
pub use site::*;
// system 的唯一调用方是 system_section.rs 的 #[cfg(target_arch = "wasm32")] 导入，
// 原生构建下该 glob 无人消费；binary crate 里 pub 不豁免 unused_imports，故放行。
#[allow(unused_imports)]
pub use system::*;
pub use trash::*;
pub use upload::*;
pub use webp::*;
