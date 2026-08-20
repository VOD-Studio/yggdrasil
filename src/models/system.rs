//! 主机指标快照模型。
//!
//! `SystemSnapshot` 两端都编译（被 `api::database::system_status` 的
//! `ServerStatus::host` 字段引用，经 serde 在服务端与客户端之间共享序列化）；
//! 真正的后台采样任务 / 全局 `RwLock` / `read_snapshot` 在
//! [`crate::tasks::sysinfo_sampler`]（server-only）。

use serde::{Deserialize, Serialize};

/// 主机指标快照（由后台采样任务周期更新）。
#[derive(Clone, Default, Serialize, Deserialize, Debug)]
pub struct SystemSnapshot {
    /// 总体 CPU 使用率（百分比）。
    pub cpu_usage: f32,
    /// 系统 1 分钟平均负载。
    pub load_avg_1: f64,
    /// 总物理内存（字节）。
    pub total_memory: u64,
    /// 已用物理内存（字节）。
    pub used_memory: u64,
    /// 主磁盘总空间（字节，取根分区或最大盘）。
    pub disk_total: u64,
    /// 主磁盘可用空间（字节）。
    pub disk_available: u64,
    /// 操作系统版本（如 "macOS 15.5"）。
    pub os_name: String,
    /// 内核版本。
    pub kernel_version: String,
    /// 系统启动后秒数。
    pub uptime_secs: u64,
}
