use futures::StreamExt;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::time::timeout;

use crate::infra::runner_config::{ResourceLimits, RUNNER_CONFIG};
use bollard::container::LogOutput;
use bollard::models::{ContainerCreateBody, HostConfig, ResourcesUlimits};
use bollard::query_parameters::{
    AttachContainerOptions, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    WaitContainerOptions,
};
use bollard::Docker;

/// 共享的 Docker 客户端。
///
/// `connect_with_unix` 会立即探测 socket 是否存在：缺失（未安装 / 未运行 Docker，
/// 或 `DOCKER_SOCKET_PATH` 指错）时返回 `SocketNotFoundError`。
///
/// 连接失败不 panic：release profile 是 `panic = "abort"`，会让整个服务进程一起挂掉，
/// 而博客本身并不依赖 Docker。改为记 error 日志后返回 `None`，代码运行器把
/// `None` 转成普通 bollard 错误向上冒泡，最终在 execute.rs 的错误脱敏层统一映射为
/// 「系统暂时不可用」，其余功能不受影响。
pub static DOCKER_CLIENT: LazyLock<Option<Docker>> = LazyLock::new(|| {
    match Docker::connect_with_unix(
        &RUNNER_CONFIG.docker_socket_path,
        120,
        bollard::API_DEFAULT_VERSION,
    ) {
        Ok(d) => Some(d),
        Err(e) => {
            tracing::error!(
                error = ?e,
                socket = %RUNNER_CONFIG.docker_socket_path,
                "无法连接 Docker daemon，代码运行功能将不可用。\
                 请确认 Docker 已安装并运行，或设置正确的 DOCKER_SOCKET_PATH",
            );
            None
        }
    }
});

/// 取共享 Docker 客户端；daemon 不可用时返回 IOError（NotFound）。
///
/// `NotFound` 既不命中 `TimedOut` 也不命中超时判断，会走 execute.rs 的通用失败路径
/// （`ExecStatus::Failed` + 「系统暂时不可用」），不会误报成超时。
fn get_docker() -> Result<&'static Docker, bollard::errors::Error> {
    DOCKER_CLIENT
        .as_ref()
        .ok_or_else(|| bollard::errors::Error::IOError {
            err: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Docker daemon 不可用（未安装或未运行）",
            ),
        })
}

/// 跨运行持久化的编译缓存（Docker named volume）。
///
/// 语言镜像把编译缓存目录（如 Go 的 GOCACHE）指到 `mount_path`，docker.rs 把
/// 共享 named volume 挂载到该路径，使每次运行复用上次编译产物。仅编译型语言使用。
#[derive(Clone, Debug, PartialEq)]
pub struct CacheVolume {
    pub name: String,
    pub mount_path: String,
}

pub fn build_host_config(
    limits: &ResourceLimits,
    cache_volume: Option<&CacheVolume>,
) -> HostConfig {
    let mut tmpfs = HashMap::new();
    // /code 用 mode=1777（sticky + all-rwx）让容器内 1000:1000 用户可写。
    // 不用 `uid=1000,gid=1000`：那是 Docker 的 tmpfs 扩展选项，Podman 报
    // `unknown mount option "uid=1000"`。mode=1777 是 POSIX 标准 tmpfs 选项，
    // Docker 与 Podman 都支持，语义等价（任意 UID 可写 /code）。
    tmpfs.insert("/code".to_string(), "size=16m,mode=1777".to_string());
    // /tmp 必须 exec：编译型语言（go/rust）把编译产物落在 /tmp 后再 exec，
    // Docker tmpfs 默认 noexec 会让执行二进制时报 EACCES（permission denied）。
    // 解释型语言（python/node）执行根文件系统的解释器，不受影响。
    tmpfs.insert("/tmp".to_string(), "size=64m,mode=1777,exec".to_string());
    tmpfs.insert("/run".to_string(), "size=16m,mode=1777".to_string());

    let memory = (limits.memory_mb * 1024 * 1024) as i64;

    HostConfig {
        cpu_quota: Some((limits.cpu_cores * 100_000.0) as i64),
        cpu_period: Some(100_000),
        memory: Some(memory),
        memory_swap: Some(memory), // = memory, disable swap
        network_mode: Some(if limits.allow_network {
            "bridge".to_string()
        } else {
            "none".to_string()
        }),
        readonly_rootfs: Some(true),
        tmpfs: Some(tmpfs),
        // 编译缓存 named volume：裸卷名 `name:/path` 即命名卷挂载，Docker/Podman
        // 都会自动创建卷。binds 不受 readonly_rootfs 影响，挂载点仍可写；
        // mount_path（如 /go-cache）是镜像内新路径，与 tmpfs 挂载无叠加。
        binds: cache_volume.map(|cv| vec![format!("{}:{}", cv.name, cv.mount_path)]),
        pids_limit: Some(64),
        // 只保留 nofile（fd 数上限，语义正常）。
        // 不设 nproc：RLIMIT_NPROC 在 setrlimit 时按 UID 计数，配合 non-root 用户会让
        // 容器初始 exec /bin/sh 直接 EAGAIN（"exec: resource temporarily unavailable"），
        // 与容器内实际进程数无关。pids_limit 已在 cgroup 层兜底，nproc 是冗余且有害的双重约束。
        ulimits: Some(vec![ResourcesUlimits {
            name: Some("nofile".to_string()),
            soft: Some(64),
            hard: Some(64),
        }]),
        cap_drop: Some(vec!["ALL".to_string()]),
        security_opt: Some(vec!["no-new-privileges".to_string()]),
        auto_remove: Some(false), // must be false to avoid premature removal before getting logs
        ..Default::default()
    }
}

struct ContainerGuard {
    container_id: String,
    docker: Docker,
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let docker = self.docker.clone();
        let container_id = self.container_id.clone();
        // 容器清理是 fire-and-forget：调用方已返回，无法把错误回传给业务层。
        // 因此重试几次以抵抗瞬时故障（daemon 繁忙 / socket 抖动），
        // 仍失败则记录 error 级日志并带上 container_id，便于运维手动 `docker rm -f` 清理，
        // 避免容器静默泄漏、长期堆积。
        tokio::spawn(async move {
            let max_attempts = 3u8;
            let mut backoff = Duration::from_millis(200);
            for attempt in 1..=max_attempts {
                let remove_options = Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                });
                match docker.remove_container(&container_id, remove_options).await {
                    Ok(()) => return,
                    Err(e) if attempt < max_attempts => {
                        tracing::warn!(
                            attempt,
                            max_attempts,
                            "remove_container 失败，稍后重试: {:?}",
                            e
                        );
                        tokio::time::sleep(backoff).await;
                        backoff *= 2;
                    }
                    Err(e) => {
                        tracing::error!(
                            container_id = %container_id,
                            "重试 {} 次后仍无法删除容器，可能泄漏；请手动执行 `docker rm -f {}`: {:?}",
                            max_attempts,
                            container_id,
                            e
                        );
                        return;
                    }
                }
            }
        });
    }
}

pub async fn run_in_container(
    image_name: &str,
    run_cmd: &str,
    source: &str,
    ext: &str,
    limits: ResourceLimits,
    cache_volume: Option<&CacheVolume>,
) -> Result<(Option<i64>, String, String, bool), bollard::errors::Error> {
    let docker = get_docker()?;
    let host_config = build_host_config(&limits, cache_volume);

    // Source injection script: use sh -c to first receive stdin and write to file, then exec the actual command
    let setup_cmd = format!("cat > /code/main.{} && exec {}", ext, run_cmd);
    let cmd = vec!["sh".to_string(), "-c".to_string(), setup_cmd];

    let config = ContainerCreateBody {
        image: Some(image_name.to_string()),
        cmd: Some(cmd),
        host_config: Some(host_config),
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        open_stdin: Some(true),
        stdin_once: Some(true),
        user: Some("1000:1000".to_string()), // non-root user
        working_dir: Some("/code".to_string()),
        ..Default::default()
    };

    let container = docker
        .create_container(None::<CreateContainerOptions>, config)
        .await?;

    let container_id = container.id;
    let _guard = ContainerGuard {
        container_id: container_id.clone(),
        docker: docker.clone(),
    };

    // Attach to container to stream stdin, stdout, and stderr
    let attach_res = docker
        .attach_container(
            &container_id,
            Some(AttachContainerOptions {
                stdin: true,
                stdout: true,
                stderr: true,
                stream: true,
                logs: false,
                ..Default::default()
            }),
        )
        .await;

    let (mut writer, mut stream) = match attach_res {
        Ok(res) => (res.input, res.output),
        Err(e) => return Err(e),
    };

    // Start container
    docker
        .start_container(&container_id, None::<StartContainerOptions>)
        .await?;

    // Write source code to stdin and drop/close the writer
    use tokio::io::AsyncWriteExt;
    let write_fut = async {
        let _ = writer.write_all(source.as_bytes()).await;
        let _ = writer.flush().await;
        let _ = writer.shutdown().await;
    };

    if timeout(Duration::from_secs(5), write_fut).await.is_err() {
        return Err(bollard::errors::Error::IOError {
            err: std::io::Error::new(std::io::ErrorKind::TimedOut, "Writing to stdin timed out"),
        });
    }
    drop(writer);

    // Wait for execution with timeout control
    let wait_future = async {
        let mut wait_stream = docker.wait_container(&container_id, None::<WaitContainerOptions>);
        wait_stream.next().await
    };

    let wait_res = timeout(Duration::from_secs(limits.timeout_secs), wait_future).await;

    let mut timed_out = false;
    let mut exit_code = None;

    match wait_res {
        Ok(Some(Ok(exit_status))) => {
            exit_code = Some(exit_status.status_code);
        }
        Ok(_) => {} // wait error
        Err(_) => {
            // timeout, kill container
            timed_out = true;
            let _ = docker.kill_container(&container_id, None).await;
        }
    }

    // Collect logs
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();

    while let Some(item) = stream.next().await {
        match item {
            Ok(chunk) => {
                match chunk {
                    LogOutput::StdOut { message } => {
                        let remaining = (limits.output_bytes as usize)
                            .saturating_sub(stdout_buf.len() + stderr_buf.len());
                        if remaining > 0 {
                            let to_add = message.len().min(remaining);
                            stdout_buf.extend_from_slice(&message[..to_add]);
                        }
                    }
                    LogOutput::StdErr { message } => {
                        let remaining = (limits.output_bytes as usize)
                            .saturating_sub(stdout_buf.len() + stderr_buf.len());
                        if remaining > 0 {
                            let to_add = message.len().min(remaining);
                            stderr_buf.extend_from_slice(&message[..to_add]);
                        }
                    }
                    _ => {}
                }
                if stdout_buf.len() + stderr_buf.len() >= limits.output_bytes as usize {
                    break;
                }
            }
            Err(e) => {
                tracing::error!("Error reading container log stream: {:?}", e);
                break;
            }
        }
    }

    // Check OOM status
    let inspect = docker.inspect_container(&container_id, None).await;
    let oom_killed = inspect
        .ok()
        .and_then(|info| info.state.and_then(|s| s.oom_killed))
        .unwrap_or(false);

    // Truncate output to limits.output_bytes
    let limit_bytes = limits.output_bytes as usize;
    let stdout_len = stdout_buf.len().min(limit_bytes);
    let stderr_len = stderr_buf.len().min(limit_bytes);

    let stdout = String::from_utf8_lossy(&stdout_buf[..stdout_len]).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_buf[..stderr_len]).into_owned();

    if timed_out {
        return Err(bollard::errors::Error::IOError {
            err: std::io::Error::new(std::io::ErrorKind::TimedOut, "Execution timed out"),
        });
    }

    Ok((exit_code, stdout, stderr, oom_killed))
}

/// 流式输出 chunk：run_in_container_stream 边读日志边推送给 SSE handler。
///
/// 序列化后作为 SSE event data；`Done` 同时携带终态信息（退出码 / OOM / 超时 / 耗时）。
#[derive(Clone, Debug)]
pub enum OutputChunk {
    /// stdout 块（容器逐块产出）。
    Stdout(String),
    /// stderr 块（容器逐块产出）。
    Stderr(String),
    /// 终态：容器执行结束。exit_code=None 表示拿不到退出码（wait 出错）。
    /// duration_ms = start_container 到 wait 完成的耗时。
    Done {
        exit_code: Option<i64>,
        oom_killed: bool,
        timed_out: bool,
        duration_ms: u64,
        /// 系统级错误消息（镜像缺失 / daemon 不可达等）。正常执行时为 None；
        /// 由 spawn_exec_task 的 Err 分支在容器拉起失败时推送，让前端 SSE
        /// 能拿到可操作消息，而非笼统的「连接异常」。
        error: Option<String>,
    },
}

/// 流式执行：与 [`run_in_container`] 相同的容器生命周期与清理（`ContainerGuard`），
/// 但边读日志流边推 chunk 到 `tx`，同时保留完整 buffer 供调用方回填 EXEC_TASKS。
///
/// 与 `run_in_container` 的差异：
/// 1. 日志循环里每块 chunk 既 `tx.send` 推流，也 append 到本地 buffer。
/// 2. 用 `tokio::select!` 在日志读取中并发等待 `tx` 关闭——客户端断开（SSE 关闭）
///    → `tx` 所有 Sender drop → `rx` 返回 None → 中止读取。
/// 3. 终态推 `OutputChunk::Done` 后 return。
///
/// 返回完整 buffer（exit_code / stdout / stderr / oom / timed_out），供调用方写 EXEC_TASKS，
/// 让轮询兜底路径（get_exec_result）也能拿到完整结果。
pub async fn run_in_container_stream(
    image_name: &str,
    run_cmd: &str,
    source: &str,
    ext: &str,
    limits: ResourceLimits,
    cache_volume: Option<&CacheVolume>,
    tx: tokio::sync::mpsc::Sender<OutputChunk>,
) -> Result<(Option<i64>, String, String, bool, bool), bollard::errors::Error> {
    let docker = get_docker()?;
    let host_config = build_host_config(&limits, cache_volume);

    // 与 run_in_container 相同的 stdin 注入脚本。
    let setup_cmd = format!("cat > /code/main.{} && exec {}", ext, run_cmd);
    let cmd = vec!["sh".to_string(), "-c".to_string(), setup_cmd];

    let config = ContainerCreateBody {
        image: Some(image_name.to_string()),
        cmd: Some(cmd),
        host_config: Some(host_config),
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        open_stdin: Some(true),
        stdin_once: Some(true),
        user: Some("1000:1000".to_string()),
        working_dir: Some("/code".to_string()),
        ..Default::default()
    };

    let container = docker
        .create_container(None::<CreateContainerOptions>, config)
        .await?;

    let container_id = container.id;
    let _guard = ContainerGuard {
        container_id: container_id.clone(),
        docker: docker.clone(),
    };

    // Attach 到容器的 stdin/stdout/stderr 流。
    let attach_res = docker
        .attach_container(
            &container_id,
            Some(AttachContainerOptions {
                stdin: true,
                stdout: true,
                stderr: true,
                stream: true,
                logs: false,
                ..Default::default()
            }),
        )
        .await;

    let (mut writer, mut stream) = match attach_res {
        Ok(res) => (res.input, res.output),
        Err(e) => return Err(e),
    };

    docker
        .start_container(&container_id, None::<StartContainerOptions>)
        .await?;

    // 容器开始执行的时刻，用于计算 duration_ms（start_container 返回即视为起点）。
    let start_time = std::time::Instant::now();

    // 写入源码到 stdin 后关闭 writer。
    use tokio::io::AsyncWriteExt;
    let write_fut = async {
        let _ = writer.write_all(source.as_bytes()).await;
        let _ = writer.flush().await;
        let _ = writer.shutdown().await;
    };

    if timeout(Duration::from_secs(5), write_fut).await.is_err() {
        return Err(bollard::errors::Error::IOError {
            err: std::io::Error::new(std::io::ErrorKind::TimedOut, "Writing to stdin timed out"),
        });
    }
    drop(writer);

    // —— 关键：wait_container 与日志读取必须并发，否则流式失效 ——
    //
    // 若先 await wait_container（等容器退出）再读日志流，wait 会阻塞到程序结束，
    // 届时 attach stream 里已缓冲全部输出，stream.next() 一次性快速读完——
    // 表现为"等完再一次性输出"，流式名存实亡。
    //
    // 用 tokio::select! 让两条分支并发：
    // - log_reader：持续读 attach stream，每块 chunk 立即 tx.send 推流 + 累积 buffer
    // - wait_with_timeout：等容器退出（带超时），退出后日志流自然结束（stream 返回 None）
    // 先完成的一方触发 select 返回；若 wait 超时则 kill 容器。
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let limit_bytes = limits.output_bytes as usize;
    let mut client_disconnected = false;
    let mut timed_out = false;
    let mut exit_code = None;

    // 日志读取循环：逐块推流 + 累积 buffer。
    // 循环正常退出条件：stream 返回 None（容器退出后 Docker 关闭 attach 流），
    // 或输出超限 break，或 select 被另一分支抢先完成（log_reader 被 drop）。
    let log_reader = async {
        while let Some(item) = stream.next().await {
            match item {
                Ok(chunk) => match chunk {
                    LogOutput::StdOut { message } => {
                        let remaining =
                            limit_bytes.saturating_sub(stdout_buf.len() + stderr_buf.len());
                        if remaining > 0 {
                            let to_add = message.len().min(remaining);
                            let slice = &message[..to_add];
                            stdout_buf.extend_from_slice(slice);
                            if !client_disconnected {
                                let text = String::from_utf8_lossy(slice).into_owned();
                                if tx.send(OutputChunk::Stdout(text)).await.is_err() {
                                    client_disconnected = true;
                                }
                            }
                        }
                    }
                    LogOutput::StdErr { message } => {
                        let remaining =
                            limit_bytes.saturating_sub(stdout_buf.len() + stderr_buf.len());
                        if remaining > 0 {
                            let to_add = message.len().min(remaining);
                            let slice = &message[..to_add];
                            stderr_buf.extend_from_slice(slice);
                            if !client_disconnected {
                                let text = String::from_utf8_lossy(slice).into_owned();
                                if tx.send(OutputChunk::Stderr(text)).await.is_err() {
                                    client_disconnected = true;
                                }
                            }
                        }
                    }
                    _ => {}
                },
                Err(e) => {
                    tracing::error!("Error reading container log stream: {:?}", e);
                    break;
                }
            }
            if stdout_buf.len() + stderr_buf.len() >= limit_bytes {
                break;
            }
        }
    };

    // 带超时地等待容器退出。
    let wait_future = async {
        let mut wait_stream = docker.wait_container(&container_id, None::<WaitContainerOptions>);
        wait_stream.next().await
    };
    let wait_with_timeout = async {
        match timeout(Duration::from_secs(limits.timeout_secs), wait_future).await {
            Ok(Some(Ok(exit_status))) => Some(exit_status.status_code),
            Ok(Some(Err(_))) | Ok(None) => None, // wait error or stream ended
            Err(_) => {
                // 超时，杀容器。kill 后 attach stream 会被 Docker 关闭，log_reader 自然结束。
                timed_out = true;
                let _ = docker.kill_container(&container_id, None).await;
                None
            }
        }
    };

    // 并发：哪边先完成就先用其结果。
    // 通常 wait 先完成（容器退出 → Docker 关闭 attach stream → log_reader 也很快结束），
    // 但若日志流先因输出超限 break，wait 会被 select drop 掉（容器仍在跑，后续 _guard 清理）。
    tokio::select! {
        status = wait_with_timeout => {
            exit_code = status;
            // 容器已退出，但 attach stream 可能还有缓冲的尾部日志。
            // 继续读完日志流（非阻塞：stream 即将返回 None）。
            while let Some(item) = stream.next().await {
                if let Ok(chunk) = item {
                    match chunk {
                        LogOutput::StdOut { message } => {
                            let remaining = limit_bytes.saturating_sub(stdout_buf.len() + stderr_buf.len());
                            let to_add = message.len().min(remaining);
                            if to_add > 0 {
                                let slice = &message[..to_add];
                                stdout_buf.extend_from_slice(slice);
                                if !client_disconnected {
                                    let text = String::from_utf8_lossy(slice).into_owned();
                                    let _ = tx.send(OutputChunk::Stdout(text)).await;
                                }
                            }
                        }
                        LogOutput::StdErr { message } => {
                            let remaining = limit_bytes.saturating_sub(stdout_buf.len() + stderr_buf.len());
                            let to_add = message.len().min(remaining);
                            if to_add > 0 {
                                let slice = &message[..to_add];
                                stderr_buf.extend_from_slice(slice);
                                if !client_disconnected {
                                    let text = String::from_utf8_lossy(slice).into_owned();
                                    let _ = tx.send(OutputChunk::Stderr(text)).await;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        _ = log_reader => {
            // 日志流先结束（输出超限 break，或 attach 断开），容器可能仍在运行。
            // C3 修复：旧代码 wait_container().next().await 无超时无 kill，注释还谎称
            // "短超时"。`while True: print()` 式程序会永久挂起 → RUNNER_SEMAPHORE 许可
            // 不释放（默认 4 并发 → 4 个即 DoS 全部代码执行器）+ ContainerGuard 不 drop
            // → 容器永久运行烧 CPU。与 wait_with_timeout 分支对称：带超时地等退出，超时则 kill。
            let mut wait_stream = docker.wait_container(&container_id, None::<WaitContainerOptions>);
            match timeout(Duration::from_secs(limits.timeout_secs), wait_stream.next()).await {
                Ok(Some(Ok(status))) => exit_code = Some(status.status_code),
                Ok(Some(Err(_))) | Ok(None) => {}
                Err(_) => {
                    // 超时：kill 后再等一次回收 exit_code（kill 后 wait 立即返回）。
                    timed_out = true;
                    let _ = docker.kill_container(&container_id, None).await;
                    if let Some(Ok(status)) = wait_stream.next().await {
                        exit_code = Some(status.status_code);
                    }
                }
            }
        }
    }

    // 检查 OOM 状态。
    let inspect = docker.inspect_container(&container_id, None).await;
    let oom_killed = inspect
        .ok()
        .and_then(|info| info.state.and_then(|s| s.oom_killed))
        .unwrap_or(false);
    // 推送终态 chunk（客户端已断开则跳过，send 必然失败）。
    let duration_ms = start_time.elapsed().as_millis() as u64;
    if !client_disconnected {
        let _ = tx
            .send(OutputChunk::Done {
                exit_code,
                oom_killed,
                timed_out,
                duration_ms,
                error: None,
            })
            .await;
    }

    let stdout_len = stdout_buf.len().min(limit_bytes);
    let stderr_len = stderr_buf.len().min(limit_bytes);
    let stdout = String::from_utf8_lossy(&stdout_buf[..stdout_len]).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_buf[..stderr_len]).into_owned();

    if timed_out {
        return Err(bollard::errors::Error::IOError {
            err: std::io::Error::new(std::io::ErrorKind::TimedOut, "Execution timed out"),
        });
    }

    Ok((exit_code, stdout, stderr, oom_killed, timed_out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::runner_config::ResourceLimits;

    #[test]
    fn test_host_config_generation() {
        let limits = ResourceLimits {
            cpu_cores: 1.5,
            memory_mb: 256,
            timeout_secs: 5,
            output_bytes: 1024,
            allow_network: false,
        };
        let host_config = build_host_config(&limits, None);
        assert_eq!(host_config.cpu_quota, Some(150_000));
        assert_eq!(host_config.memory, Some(256 * 1024 * 1024));
        assert_eq!(host_config.readonly_rootfs, Some(true));
        assert_eq!(host_config.network_mode.as_deref(), Some("none"));

        // 隔离不变量：这些配置若被误删，容器隔离会悄悄失效。
        // 单独断言而非隐含在端到端测试里，确保回归可见。
        assert_eq!(
            host_config.cap_drop.as_deref(),
            Some(&["ALL".to_string()][..])
        );
        assert_eq!(
            host_config.security_opt.as_deref(),
            Some(&["no-new-privileges".to_string()][..])
        );
        assert_eq!(host_config.pids_limit, Some(64));
        // memory_swap == memory 才是真正禁用 swap；二者不等价于可换出到磁盘
        assert_eq!(host_config.memory_swap, host_config.memory);
        assert_eq!(host_config.memory_swap, Some(256 * 1024 * 1024));
    }

    #[test]
    fn host_config_cache_volume_binds() {
        // 编译缓存 named volume：None 时不挂载任何 binds；Some 时以裸卷名
        // `name:/path` 挂载（Docker/Podman 自动创建卷，不受 readonly_rootfs 影响）。
        let limits = ResourceLimits {
            cpu_cores: 1.0,
            memory_mb: 128,
            timeout_secs: 5,
            output_bytes: 1024,
            allow_network: false,
        };
        assert_eq!(build_host_config(&limits, None).binds, None);

        let cache = CacheVolume {
            name: "yggdrasil-gocache".to_string(),
            mount_path: "/go-cache".to_string(),
        };
        assert_eq!(
            build_host_config(&limits, Some(&cache)).binds,
            Some(vec!["yggdrasil-gocache:/go-cache".to_string()])
        );
    }

    #[test]
    fn host_config_ulimits_drops_only_nofile() {
        // nproc 在容器内对 non-root 用户有害（初始 exec /bin/sh 直接 EAGAIN），
        // 只保留 nofile=64。若有人加回 nproc，这里会失败。
        let limits = ResourceLimits {
            cpu_cores: 1.0,
            memory_mb: 128,
            timeout_secs: 5,
            output_bytes: 1024,
            allow_network: false,
        };
        let host_config = build_host_config(&limits, None);
        let ulimits = host_config.ulimits.expect("ulimits must be set");
        assert_eq!(ulimits.len(), 1, "only nofile, no nproc");
        let nf = &ulimits[0];
        assert_eq!(nf.name.as_deref(), Some("nofile"));
        assert_eq!(nf.soft, Some(64));
        assert_eq!(nf.hard, Some(64));
    }

    #[test]
    fn host_config_tmpfs_has_exec_on_tmp_only() {
        // /tmp 必须 exec（编译型语言把产物落在 /tmp 后再 exec）；
        // /code、/run 不带 exec（默认 noexec）。若误给 /code 也加 exec，测试失败。
        let limits = ResourceLimits {
            cpu_cores: 1.0,
            memory_mb: 128,
            timeout_secs: 5,
            output_bytes: 1024,
            allow_network: false,
        };
        let host_config = build_host_config(&limits, None);
        let tmpfs = host_config.tmpfs.expect("tmpfs must be set");
        assert_eq!(tmpfs.len(), 3);
        assert!(tmpfs["/tmp"].contains("exec"));
        assert!(!tmpfs["/code"].contains("exec"));
        assert!(!tmpfs["/run"].contains("exec"));
    }

    #[test]
    fn host_config_network_mode_follows_allow_network() {
        let base = ResourceLimits {
            cpu_cores: 1.0,
            memory_mb: 128,
            timeout_secs: 5,
            output_bytes: 1024,
            allow_network: false,
        };
        assert_eq!(
            build_host_config(&base, None).network_mode.as_deref(),
            Some("none")
        );
        let mut net = base;
        net.allow_network = true;
        assert_eq!(
            build_host_config(&net, None).network_mode.as_deref(),
            Some("bridge")
        );
    }

    #[test]
    fn host_config_cpu_and_memory_scale_with_limits() {
        // 资源钳制是安全边界，断言换算公式不漂移（cpu_cores * 100_000，memory_mb * 1MiB）。
        let limits = ResourceLimits {
            cpu_cores: 2.0,
            memory_mb: 512,
            timeout_secs: 5,
            output_bytes: 1024,
            allow_network: false,
        };
        let host_config = build_host_config(&limits, None);
        assert_eq!(host_config.cpu_quota, Some(200_000));
        assert_eq!(host_config.cpu_period, Some(100_000));
        assert_eq!(host_config.memory, Some(512 * 1024 * 1024));
        assert_eq!(host_config.memory_swap, Some(512 * 1024 * 1024));
    }

    /// 探测 Docker daemon 是否可用：socket 缺失（`DOCKER_CLIENT == None`）、
    /// daemon 无响应、或所需镜像不在本地时返回 `None`。集成测试用它做动态
    /// 守卫——daemon 与镜像都在则跑，否则显式跳过（eprintln + 提前 return），
    /// 而非 panic 失败。
    ///
    /// 这比 `#[ignore]` 更合适：`#[ignore]` 会在所有环境（含有 Docker 的开发机）
    /// 一律跳过、且需 `cargo test --ignored` 显式触发；探测守卫让这些测试在
    /// 有 Docker 时自动运行、在无 Docker 或缺镜像的 CI 上静默放行。
    ///
    /// 镜像探测：GitHub runner 有 daemon 但默认不带 alpine:latest，创建容器
    /// 时会 404。这里用 inspect_image 只读检查本地是否已有镜像，缺失即跳过，
    /// 不在此 pull（CI 不应依赖外网拉镜像——慢且脆弱）。
    async fn require_docker_with_image(image: &str) -> Option<&'static Docker> {
        let docker = DOCKER_CLIENT.as_ref()?;
        // socket 在但 daemon 挂了的情况靠这一步轻量只读调用兜住。
        docker.version().await.ok()?;
        // 镜像不在本地 → 视为环境不可用，跳过而非 panic。
        docker.inspect_image(image).await.ok()?;
        Some(docker)
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_run_in_container_success() {
        if require_docker_with_image("alpine:latest").await.is_none() {
            eprintln!("skip: Docker daemon 不可用或缺少 alpine:latest");
            return;
        }
        let limits = ResourceLimits {
            cpu_cores: 1.0,
            memory_mb: 128,
            timeout_secs: 5,
            output_bytes: 1024,
            allow_network: false,
        };
        let (exit_code, stdout, stderr, oom_killed) = run_in_container(
            "alpine:latest",
            "cat /code/main.txt",
            "hello world",
            "txt",
            limits,
            None,
        )
        .await
        .unwrap();

        assert_eq!(exit_code, Some(0));
        assert_eq!(stdout, "hello world");
        assert!(stderr.is_empty());
        assert!(!oom_killed);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_run_in_container_output_truncation() {
        if require_docker_with_image("alpine:latest").await.is_none() {
            eprintln!("skip: Docker daemon 不可用或缺少 alpine:latest");
            return;
        }
        let limits = ResourceLimits {
            cpu_cores: 1.0,
            memory_mb: 128,
            timeout_secs: 5,
            output_bytes: 5,
            allow_network: false,
        };
        let (exit_code, stdout, stderr, oom_killed) = run_in_container(
            "alpine:latest",
            "cat /code/main.txt",
            "hello world",
            "txt",
            limits,
            None,
        )
        .await
        .unwrap();

        assert_eq!(exit_code, Some(0));
        assert_eq!(stdout, "hello");
        assert!(stderr.is_empty());
        assert!(!oom_killed);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_run_in_container_timeout() {
        if require_docker_with_image("alpine:latest").await.is_none() {
            eprintln!("skip: Docker daemon 不可用或缺少 alpine:latest");
            return;
        }
        let limits = ResourceLimits {
            cpu_cores: 1.0,
            memory_mb: 128,
            timeout_secs: 1,
            output_bytes: 1024,
            allow_network: false,
        };
        let res = run_in_container("alpine:latest", "sleep 10", "", "txt", limits, None).await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        match err {
            bollard::errors::Error::IOError { err } => {
                assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
            }
            _ => panic!("Expected IOError(TimedOut), got {:?}", err),
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_run_in_container_cancellation() {
        use bollard::query_parameters::ListContainersOptions;
        let docker = match require_docker_with_image("alpine:latest").await {
            Some(d) => d,
            None => {
                eprintln!("skip: Docker daemon 不可用或缺少 alpine:latest");
                return;
            }
        };

        let before = docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                ..Default::default()
            }))
            .await
            .unwrap();
        let before_ids: std::collections::HashSet<String> =
            before.into_iter().map(|c| c.id.unwrap()).collect();

        let limits = ResourceLimits {
            cpu_cores: 1.0,
            memory_mb: 128,
            timeout_secs: 10,
            output_bytes: 1024,
            allow_network: false,
        };

        let run_fut = run_in_container("alpine:latest", "sleep 100", "", "txt", limits, None);

        tokio::select! {
            _ = run_fut => {
                panic!("Should have been cancelled");
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                // Cancelled!
            }
        }

        // ContainerGuard::drop 的清理是 fire-and-forget（tokio::spawn +
        // remove_container 带 3 次重试退避）。固定 sleep + 单次检查会在 CI
        // runner 高负载时误报泄漏——Docker daemon 的 kill+remove 可能需要
        // 数秒。改为轮询：容器一消失就立即通过，仅当超过宽限期限仍存在才判定泄漏。
        let deadline = Duration::from_secs(15);
        let poll_interval = Duration::from_millis(250);
        let start = std::time::Instant::now();

        let leaked = loop {
            tokio::time::sleep(poll_interval).await;

            let after = docker
                .list_containers(Some(ListContainersOptions {
                    all: true,
                    ..Default::default()
                }))
                .await
                .unwrap();

            let leaked: Vec<String> = after
                .into_iter()
                .filter(|c| {
                    c.id.as_ref().is_some_and(|id| !before_ids.contains(id))
                        && c.image.as_deref() == Some("alpine:latest")
                })
                .map(|c| c.id.unwrap())
                .collect();

            if leaked.is_empty() || start.elapsed() >= deadline {
                break leaked;
            }
        };

        // 超时后仍有残留 → 清理掉避免影响后续 serial 测试，再断言失败。
        for id in &leaked {
            let _ = docker
                .remove_container(
                    id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await;
        }

        assert_eq!(
            leaked.len(),
            0,
            "Found {} leaked containers after polling",
            leaked.len()
        );
    }
}
