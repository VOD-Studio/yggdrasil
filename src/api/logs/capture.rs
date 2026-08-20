//! 进程内日志捕获 Layer（server-only）。
//!
//! 作为 `tracing_subscriber::Layer` 挂进 registry（见 main.rs），把全部 target
//! 的日志事件复制一份送进两条无锁通道：
//! - `mpsc`（容量 4096）：[`crate::tasks::log_writer`] 攒批后批量 INSERT 进 logs 表；
//! - `broadcast`（容量 1024）：[`super::sse`] 的实时流订阅，按连接参数过滤后推送。
//!
//! 关键性质：
//! - **绝不阻塞日志路径**：两条通道都用非阻塞发送。mpsc 满了只累加 dropped
//!   计数；broadcast 满了最旧事件被覆盖，接收端走 `Lagged` → `gap` 事件。
//! - **防递归**：`yggdrasil::api::logs` 与 `yggdrasil::tasks::log_writer`
//!   前缀的 target 直接跳过——写库失败的 error 日志不能再进管道，否则
//!   「写库失败 → 记日志 → 再写库 → 再失败」会自我放大。
//! - **独立过滤**：Layer 在 main.rs 里以 per-layer `EnvFilter` 包裹
//!   （[`log_viewer_filter`]，读 `LOG_VIEWER_LEVEL`，默认 info，不吃 RUST_LOG）。
//!   注意 `tracing` 依赖带 `release_max_level_info`：release 构建中
//!   DEBUG/TRACE 在编译期即被剔除，env 调到 debug 也只有 info+。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, mpsc};
use tracing::field::{Field, Visit};
use tracing::Event;
use tracing::Subscriber;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::EnvFilter;

/// 落库通道容量：writer 未启动（迁移窗口）或 DB 短暂不可达时的缓冲。
/// 满了按条计入 dropped，绝不阻塞打日志的线程。
const MPSC_CAP: usize = 4096;

/// 实时流通道容量：SSE 客户端消费慢时最旧事件被覆盖（接收端收 Lagged）。
const BROADCAST_CAP: usize = 1024;

/// 单条消息上限（字节）：防止异常大日志撑爆内存与表行。
const MAX_MESSAGE_BYTES: usize = 4 * 1024;

/// 防递归 target 前缀：日志管道自身的日志不再进管道。
const EXCLUDED_TARGETS: [&str; 2] = ["yggdrasil::api::logs", "yggdrasil::tasks::log_writer"];

/// 内部捕获记录（writer 落库与 SSE 实时流共用的最小表示，非 serde DTO）。
#[derive(Debug, Clone)]
pub struct LogRecord {
    /// 事件捕获时刻（UTC）。
    pub ts: DateTime<Utc>,
    /// 级别大写静态串（ERROR/WARN/INFO/DEBUG/TRACE）转 owned。
    pub level: String,
    /// tracing target（模块路径）。
    pub target: String,
    /// 消息文本（含追加的结构化字段，已截断至 4KB）。
    pub message: String,
}

/// 进程级日志通道枢纽。
struct LogChannels {
    /// 落库通道发送端（Layer 每条事件 try_send 一份）。
    db_tx: mpsc::Sender<LogRecord>,
    /// 落库通道接收端，启动时被 [`crate::tasks::log_writer`] 取走一次（take 语义）。
    db_rx: Mutex<Option<mpsc::Receiver<LogRecord>>>,
    /// 实时流广播发送端（SSE 每连接 subscribe 一个 receiver）。
    live_tx: broadcast::Sender<LogRecord>,
    /// 丢弃计数：mpsc 满 / 写库失败丢批，逐条累加（get_logs 响应透出）。
    dropped: AtomicU64,
}

/// 全局通道实例（LazyLock：Layer 是 'static 的，无法从外部注入）。
static CHANNELS: LazyLock<LogChannels> = LazyLock::new(|| {
    let (db_tx, db_rx) = mpsc::channel(MPSC_CAP);
    let (live_tx, _) = broadcast::channel(BROADCAST_CAP);
    LogChannels {
        db_tx,
        db_rx: Mutex::new(Some(db_rx)),
        live_tx,
        dropped: AtomicU64::new(0),
    }
});

/// 取走落库通道接收端（一次性；第二次调用返回 None）。
pub fn take_db_receiver() -> Option<mpsc::Receiver<LogRecord>> {
    CHANNELS.db_rx.lock().ok().and_then(|mut g| g.take())
}

/// 订阅实时流广播（每 SSE 连接一个 receiver）。
pub fn subscribe_live() -> broadcast::Receiver<LogRecord> {
    CHANNELS.live_tx.subscribe()
}

/// 进程启动以来累计丢弃的日志条数。
pub fn dropped_count() -> u64 {
    CHANNELS.dropped.load(Ordering::Relaxed)
}

/// 累加丢弃计数（writer 写库失败丢批时按批大小调用）。
pub fn record_dropped(n: u64) {
    CHANNELS.dropped.fetch_add(n, Ordering::Relaxed);
}

/// capture 层的独立 EnvFilter：读 `LOG_VIEWER_LEVEL`，非法/缺失时回退 "info"。
/// 故意不吃 `RUST_LOG`——控制台级别与查看器级别解耦。
pub fn log_viewer_filter() -> EnvFilter {
    std::env::var("LOG_VIEWER_LEVEL")
        .ok()
        .and_then(|v| EnvFilter::try_new(v).ok())
        .unwrap_or_else(|| EnvFilter::new("info"))
}

/// 日志捕获 Layer。在 main.rs 里以 `.with_filter(log_viewer_filter())` 包裹后
/// 挂进 registry；本体的 `enabled` 恒 true，级别过滤全部交给 per-layer filter。
/// 泛型实现（非特化 Registry）：registry().with(fmt).with(capture) 组合时，
/// 外层 S 是 Layered<...> 而非裸 Registry，特化实现会导致 Layered 不再满足
/// Into<Dispatch>。
pub struct CaptureLayer;

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let target = meta.target();

        // 防递归：日志管道自身（查询/导出/SSE/writer）的日志不进管道。
        if EXCLUDED_TARGETS.iter().any(|p| target.starts_with(p)) {
            return;
        }

        // 提取 message 字段；其余结构化字段以 ` key=value` 追加（task_id 等不丢）。
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let mut message = visitor.message.unwrap_or_default();
        for (key, value) in visitor.extra {
            message.push(' ');
            message.push_str(&key);
            message.push('=');
            message.push_str(&value);
        }
        truncate_message(&mut message);

        let record = LogRecord {
            ts: Utc::now(),
            level: meta.level().as_str().to_string(),
            target: target.to_string(),
            message,
        };

        // mpsc 满 / 已关闭：只增 dropped 计数，绝不阻塞日志路径。
        if CHANNELS.db_tx.try_send(record.clone()).is_err() {
            CHANNELS.dropped.fetch_add(1, Ordering::Relaxed);
        }

        // 实时流：无订阅者时 broadcast::send 只会返回 Err，直接跳过省一次分发；
        // 通道满时最旧事件被覆盖，慢客户端走 RecvError::Lagged → gap 事件。
        if CHANNELS.live_tx.receiver_count() > 0 {
            let _ = CHANNELS.live_tx.send(record);
        }
    }
}

/// 事件字段提取：message 单独存，其余字段按声明顺序收集。
#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
    extra: Vec<(String, String)>,
}

impl Visit for MessageVisitor {
    /// `info!("...", k = v)` 的 message（format_args）与非字符串字段走这里。
    /// Arguments 的 Debug 输出即格式化文本，不带引号。
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        } else {
            self.extra
                .push((field.name().to_string(), format!("{value:?}")));
        }
    }

    /// 显式字符串字段（`k = "v"`）走这里，不带引号。
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.extra
                .push((field.name().to_string(), value.to_string()));
        }
    }
}

/// 按字节上限截断，回退到 char 边界防止切坏 UTF-8。
fn truncate_message(s: &mut String) {
    if s.len() > MAX_MESSAGE_BYTES {
        let mut end = MAX_MESSAGE_BYTES;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundary() {
        // 4KB 上限内全是 ASCII 时原样保留
        let mut s = "a".repeat(100);
        truncate_message(&mut s);
        assert_eq!(s.len(), 100);

        // 超限时截断，且多字节字符不被切碎
        let mut s = "日".repeat(MAX_MESSAGE_BYTES); // 每字 3 字节
        truncate_message(&mut s);
        assert!(s.len() <= MAX_MESSAGE_BYTES);
        assert!(s.is_char_boundary(s.len()));
    }

    #[test]
    fn dropped_counter_accumulates() {
        let before = dropped_count();
        record_dropped(7);
        assert_eq!(dropped_count(), before + 7);
    }

    #[test]
    fn db_receiver_take_once() {
        // 注意：本测试消费进程级单例，只能有一个测试做 take 语义断言。
        // take 之后再次 take 必须得到 None（writer 不会重复启动）。
        let first = take_db_receiver();
        let second = take_db_receiver();
        assert!(second.is_none());
        drop(first);
    }
}
