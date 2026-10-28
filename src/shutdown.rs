//! 进程退出协调。
//!
//! 收到 SIGTERM / Ctrl-C 后，Axum 的优雅关闭会等所有在途响应结束；SSE 这类
//! 长连接永不自然结束，会把退出拖到容器的停止超时（podman 默认 10 秒）后被 SIGKILL。
//! 长连接用 [`requested`] 在退出开始时主动收尾，startup 另用它限定排空时长。

use std::sync::LazyLock;

use tokio::sync::watch;

static SHUTDOWN: LazyLock<watch::Sender<bool>> = LazyLock::new(|| watch::channel(false).0);

/// 标记进程开始退出，唤醒所有 [`requested`] 等待者。
pub fn trigger() {
    SHUTDOWN.send_replace(true);
}

/// 进程开始退出时完成；已经开始退出则立即完成。
pub async fn requested() {
    let mut rx = SHUTDOWN.subscribe();
    // Sender 是静态值，永不 drop，wait_for 不会返回 Err。
    let _ = rx.wait_for(|stopping| *stopping).await;
}
