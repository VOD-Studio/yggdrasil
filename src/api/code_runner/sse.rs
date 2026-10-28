//! SSE 流式输出端点。`GET /api/exec/stream?task_id=X`。
//!
//! 鉴权 + 限流已在 [`super::execute::start_exec_stream`] 完成（校验链），此处只
//! 校验 task 存在。task_id 是 UUID，不可枚举；只有过了校验的调用者才知道 task_id。
//!
//! 前端用原生 EventSource 连接本端点（`web_sys::EventSource`），按 event 类型
//! 分发：`stdout` → 终端 writeStdout，`stderr` → writeStderr，`done` → 终态收尾。
//! keep-alive comment 每 15s 一次，防反向代理超时关闭空闲连接。

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::api::code_runner::progress::EXEC_STREAMS;
use crate::infra::docker::OutputChunk;

/// SSE 查询参数：`?task_id=X`。
#[derive(serde::Deserialize)]
pub struct StreamQuery {
    pub task_id: String,
}

#[derive(serde::Serialize)]
struct DonePayload {
    exit_code: Option<i64>,
    oom_killed: bool,
    timed_out: bool,
    duration_ms: u64,
    /// 系统级错误消息（镜像缺失 / daemon 不可达等）。正常执行时为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// SSE handler：从 EXEC_STREAMS 取出 receiver（取出即移除，防重复连接），
/// 包成 ReceiverStream，映射成 SSE Event 流。
pub async fn exec_stream(
    Query(q): Query<StreamQuery>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    // 取出 rx（取出即移除）：同一 task_id 只能连一次 SSE，
    // 防止多客户端或重连导致 receiver 被多次消费。
    let (_, entry) = EXEC_STREAMS
        .remove(&q.task_id)
        .ok_or((StatusCode::NOT_FOUND, "任务不存在或已结束".to_string()))?;

    let stream = ReceiverStream::new(entry.rx).map(|chunk| {
        Ok::<_, Infallible>(match chunk {
            OutputChunk::Stage(s) => Event::default().event("stage").data(s),
            OutputChunk::Stdout(s) => Event::default().event("stdout").data(s),
            OutputChunk::Stderr(s) => Event::default().event("stderr").data(s),
            OutputChunk::Done {
                exit_code,
                oom_killed,
                timed_out,
                duration_ms,
                error,
            } => Event::default()
                .event("done")
                .json_data(DonePayload {
                    exit_code,
                    oom_killed,
                    timed_out,
                    duration_ms,
                    error,
                })
                .unwrap_or_else(|_| Event::default().event("done").data("{}")),
        })
    });

    // 进程退出时不等容器跑完，主动结束响应，避免拖住优雅关闭。
    let stream = stream.take_until(crate::shutdown::requested());
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
