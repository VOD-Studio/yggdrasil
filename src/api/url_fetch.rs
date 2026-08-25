//! SSRF 防护的 URL 抓取：服务端按图，供 MCP `upload_media(url)` 工具使用。
//!
//! 这是 Option B 的第三通道——LLM 自身（不经 host/shell）绕过 base64 的唯一方式：
//! 工具只收 `url: String`（JSON-RPC 纯文本），服务端去抓二进制，二进制从不进 JSON-RPC。
//!
//! # SSRF 防护（多层纵深）
//!
//! 1. **仅 https**：拒绝 http:// 与其它 scheme（公共图床几乎都走 https；
//!    限 scheme 是最低成本的最大攻击面收敛）。
//! 2. **解析即锁定 IP**：先用 `std::net::ToSocketAddrs` 解析主机名，逐个 IP 做
//!    私网/回环/链路本地/保留段校验，**只把通过校验的 IP** 经 reqwest 的
//!    `.resolve(host, ip)` 钉死——reqwest 不再二次 DNS 查询，杜绝 DNS rebinding
//!    （解析时返回公网 IP 骗过校验，连接瞬间返回内网 IP 的攻击）。
//! 3. **禁重定向**：302 可重定向到内网 IP 绕过白名单——设 `redirect(Policy::none)`，
//!    抓取器对每个跳转点零信任。
//! 4. **体积上限**：流式读 + 累计字节，超过 `MAX_FILE_SIZE` 立即中止，避免无界
//!    响应（Content-Length 可伪造、可缺省）撑爆内存。
//! 5. **超时**：连接 + 整体读取封顶，防慢速 loris 式拖挂。
//! 6. **白名单兜底**：最终交给 `process_image_upload`，magic bytes 二次验真，
//!    Content-Type 不被信任。
//!
//! 抓取在异步 reqwest 中执行（不阻塞 worker；转码/校验仍走 spawn_blocking）。
//! 仅 `feature = "server"` 编译。

#![cfg(feature = "server")]

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

use bytes::Bytes;
use http::Uri;

use crate::api::upload::{process_image_upload, UploadError, UploadOutcome};
use crate::utils::server::MAX_FILE_SIZE;

/// URL 抓取错误（脱敏，映射到 MCP invalid_request / internal_error）。
pub(crate) enum FetchError {
    /// 400 类：scheme 非 https / 主机缺失 / 无法解析 / 指向禁止 IP。
    Invalid(String),
    /// 400 类：远端返回非成功状态、Content-Type 非图片。
    BadStatus(String),
    /// 413 类：响应体超过上限。
    TooLarge,
    /// 500 类：网络/IO/超时等（日志记详情，客户端见静态消息）。
    Fetch(&'static str),
}

impl FetchError {
    fn invalid<E: std::fmt::Display>(e: E, ctx: &str) -> Self {
        tracing::warn!("url_fetch {ctx}: {e}");
        FetchError::Invalid(format!("无效的图片 URL：{ctx}"))
    }
}

/// 抓取 URL → 图片字节 → 走共享入库流水线 → 返回 `/uploads/...` 结果。
///
/// `original_filename` 从 URL 路径末段推导（无则 None）。见模块头部的 SSRF 防护说明。
pub(crate) async fn fetch_and_ingest(url: &str) -> Result<UploadOutcome, FetchError> {
    // 1. 解析 URL：强制 https。
    let uri: Uri = url
        .parse()
        .map_err(|e| FetchError::invalid(e, "URL 格式"))?;
    let scheme = uri.scheme_str().unwrap_or("");
    if scheme != "https" {
        return Err(FetchError::Invalid("仅支持 https:// 图片 URL".into()));
    }
    let host = uri
        .host()
        .ok_or_else(|| FetchError::Invalid("URL 缺少主机名".into()))?;
    if host.is_empty() {
        return Err(FetchError::Invalid("URL 缺少主机名".into()));
    }

    // 2. 解析主机名 → IP 列表，校验每一个都非私网/保留段。
    //    ToSocketAddrs 是阻塞的（getaddrinfo），放进 spawn_blocking。
    let host_owned = host.to_string();
    let port = uri.port_u16().unwrap_or(443);
    let resolve_target = format!("{host_owned}:{port}");
    let addrs = tokio::task::spawn_blocking(move || {
        resolve_target
            .to_socket_addrs()
            .map(|i| i.collect::<Vec<SocketAddr>>())
    })
    .await
    .map_err(|e| FetchError::invalid(e, "解析任务失败"))?
    .map_err(|e| FetchError::invalid(e, "DNS 解析失败"))?;

    if addrs.is_empty() {
        return Err(FetchError::Invalid("DNS 解析无结果".into()));
    }

    // 全部 IP 必须通过校验；任一私网 IP 即拒绝（防多记录里夹带内网）。
    for addr in &addrs {
        let ip = addr.ip();
        if is_forbidden_ip(&ip) {
            tracing::warn!("url_fetch SSRF 拒绝：{} → {}", host_owned, ip);
            return Err(FetchError::Invalid("目标地址位于禁止的网络段".into()));
        }
    }
    // 钉死首个通过校验的 IP，杜绝 reqwest 二次 DNS（rebinding）。
    let locked_ip = addrs[0].ip();

    // 3. 构建 reqwest 客户端：禁重定向 + 锁 IP + 超时 + UA。
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&host_owned, SocketAddr::from((locked_ip, port)))
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("yggdrasil-mcp-media/1.0")
        .build()
        .map_err(|e| {
            tracing::error!("url_fetch client build: {e}");
            FetchError::Fetch("构建抓取客户端失败")
        })?;

    // 4. 发起 GET。
    let resp = client.get(url).send().await.map_err(|e| {
        tracing::warn!("url_fetch GET {url} failed: {e}");
        FetchError::Fetch("抓取图片失败")
    })?;

    if !resp.status().is_success() {
        return Err(FetchError::BadStatus(format!("远端返回 {}", resp.status())));
    }

    // 5. 流式读取，累计字节超 MAX_FILE_SIZE 立即中止。
    //    Content-Length 可伪造/缺省，不能信任；以累计字节为准。
    let mut total: usize = 0;
    let mut buf: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            tracing::warn!("url_fetch stream read failed: {e}");
            FetchError::Fetch("读取图片数据失败")
        })?;
        total += chunk.len();
        if total > MAX_FILE_SIZE {
            return Err(FetchError::TooLarge);
        }
        buf.extend_from_slice(&chunk);
    }

    let data = Bytes::from(buf);

    // 6. 从 URL 路径末段推导展示文件名（仅 assets 表展示字段，不影响落盘）。
    let original_filename = uri
        .path()
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // 7. 走共享入库流水线（magic bytes 二次验真 + 尺寸 + 去重 + 转码 + 落盘）。
    process_image_upload(data, original_filename)
        .await
        .map_err(|e| match e {
            UploadError::TooLarge => FetchError::TooLarge,
            other => {
                let ctx = match other {
                    UploadError::Empty => "空响应",
                    UploadError::BadType => "非图片或格式不支持",
                    UploadError::Oversized => "图片尺寸超限",
                    UploadError::Corrupt => "图片损坏",
                    UploadError::TooLarge => "文件过大",
                    UploadError::Internal(_) => "入库失败",
                };
                FetchError::BadStatus(ctx.into())
            }
        })
}

/// SSRF 拒绝：私网 / 回环 / 链路本地 / 保留 / CGNAT 等非公网单播段。
fn is_forbidden_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()        // 10/8, 172.16/12, 192.168/16
            || v4.is_loopback()    // 127/8
            || v4.is_link_local()  // 169.254/16
            || v4.is_unspecified() // 0.0.0.0
            || v4.is_broadcast()   // 255.255.255.255
            || v4.is_documentation() // 192.0.2/24, 198.51.100/24, 203.0.113/24
            // CGNAT 100.64.0.0/10（std 未覆盖，手动判）
            || {
                let o = v4.octets();
                o[0] == 100 && o[1] >= 64 && o[1] <= 127
            }
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() {
                return true;
            }
            // IPv4-mapped / compatible IPv6 地址仍可能连接到 IPv4 内网服务；
            // 先归一化后复用完整 IPv4 禁止网段检查，避免 ::ffff:127.0.0.1 绕过。
            if let Some(v4) = v6.to_ipv4() {
                return is_forbidden_ip(&IpAddr::V4(v4));
            }
            v6.is_multicast()   // ff00::/8
            || {
                let s = v6.segments();
                (s[0] & 0xfe00) == 0xfc00 // ULA fc00::/7
                || (s[0] & 0xffc0) == 0xfe80 // 链路本地 fe80::/10
            }
        }
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::is_forbidden_ip;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn forbidden_ipv4_ranges() {
        assert!(is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
        assert!(is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))));
        assert!(is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1)))); // CGNAT
        assert!(is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(
            100, 127, 255, 255
        )))); // CGNAT 末
        assert!(is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)))); // 文档段
    }

    #[test]
    fn allowed_ipv4_ranges() {
        assert!(!is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(99, 63, 0, 1)))); // CGNAT 前
        assert!(!is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(
            100, 63, 255, 255
        )))); // CGNAT 前
        assert!(!is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1)))); // CGNAT 后
        assert!(!is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(
            172, 15, 255, 255
        )))); // 172.16 前
        assert!(!is_forbidden_ip(&IpAddr::V4(Ipv4Addr::new(172, 32, 0, 1)))); // 172.16/12 后
    }

    #[test]
    fn forbidden_ipv6_ranges() {
        assert!(is_forbidden_ip(&IpAddr::V6(Ipv6Addr::LOCALHOST))); // ::1
        assert!(is_forbidden_ip(&IpAddr::V6(Ipv6Addr::UNSPECIFIED))); // ::
        assert!(is_forbidden_ip(&IpAddr::V6("ff02::1".parse().unwrap()))); // 多播
        assert!(is_forbidden_ip(&IpAddr::V6("fc00::1".parse().unwrap()))); // ULA
        assert!(is_forbidden_ip(&IpAddr::V6("fd12::1".parse().unwrap()))); // ULA
        assert!(is_forbidden_ip(&IpAddr::V6("fe80::1".parse().unwrap()))); // 链路本地
    }

    #[test]
    fn forbidden_ipv4_mapped_ipv6_ranges() {
        assert!(is_forbidden_ip(&IpAddr::V6(
            "::ffff:127.0.0.1".parse().expect("valid mapped loopback")
        )));
        assert!(is_forbidden_ip(&IpAddr::V6(
            "::ffff:169.254.169.254"
                .parse()
                .expect("valid mapped link-local")
        )));
        assert!(is_forbidden_ip(&IpAddr::V6(
            "::ffff:10.0.0.1".parse().expect("valid mapped private")
        )));
    }

    #[test]
    fn allowed_ipv6_ranges() {
        assert!(!is_forbidden_ip(&IpAddr::V6(
            "2606:4700::1".parse().unwrap()
        ))); // Cloudflare 公网
        assert!(!is_forbidden_ip(&IpAddr::V6(
            "2001:4860:4860::8888".parse().unwrap()
        ))); // Google DNS
    }
}
