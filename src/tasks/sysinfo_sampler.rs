//! sysinfo 主机指标后台采样。
//!
//! 仅在 `server` feature 启用时编译。sysinfo 的 CPU% **不是即时值**，需要两次采样
//! 间的 delta。因此用后台任务周期刷新，server function 只读
//! [`tokio::sync::RwLock`] 快照（毫秒级返回，零采样成本）。
//!
//! 采样间隔由环境变量 `SYSINFO_SAMPLE_SECS` 配置（默认 0.5 秒，支持小数如 0.1）。
//! 快照 DTO [`crate::models::system::SystemSnapshot`] 定义在 [`crate::models::system`]（两端共享）。

use std::sync::LazyLock;
use std::time::Duration;

use crate::models::system::SystemSnapshot;

static SNAPSHOT: LazyLock<tokio::sync::RwLock<SystemSnapshot>> =
    LazyLock::new(|| tokio::sync::RwLock::new(SystemSnapshot::default()));

/// 采样间隔，由环境变量 `SYSINFO_SAMPLE_SECS` 配置，默认 0.5 秒，下限 0.05 秒。
fn sample_interval() -> Duration {
    let secs = std::env::var("SYSINFO_SAMPLE_SECS")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.5);
    Duration::from_secs_f64(secs.max(0.05))
}

/// 启动后台采样任务。应在 main 启动流程（migrate 之后、serve 之前）调用一次。
///
/// CPU% 需两次采样 delta，故循环里先 refresh 再 sleep 再 refresh 才有有效值。
pub fn spawn_sampler() {
    tokio::spawn(async move {
        use sysinfo::{Disks, System};

        let mut sys = System::new();
        let interval = sample_interval();
        // 首次 refresh 建立基线，CPU% 在下一轮才有意义。
        sys.refresh_cpu_usage();
        let disks = Disks::new_with_refreshed_list();

        loop {
            tokio::time::sleep(interval).await;
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            let load = System::load_average();

            // 主磁盘：取空间最大的盘（通常是数据盘）。
            let (disk_total, disk_available) = disks
                .list()
                .iter()
                .max_by_key(|d| d.total_space())
                .map(|d| (d.total_space(), d.available_space()))
                .unwrap_or((0, 0));

            let snap = SystemSnapshot {
                cpu_usage: sys.global_cpu_usage(),
                load_avg_1: load.one,
                total_memory: sys.total_memory(),
                used_memory: sys.used_memory(),
                disk_total,
                disk_available,
                os_name: System::long_os_version().unwrap_or_default(),
                kernel_version: System::kernel_version().unwrap_or_default(),
                uptime_secs: System::uptime(),
            };
            *SNAPSHOT.write().await = snap;
        }
    });
}

/// 读取最新快照（只读，毫秒级返回，不触发采样）。
pub async fn read_snapshot() -> SystemSnapshot {
    SNAPSHOT.read().await.clone()
}
