//! 基于 governor 的多级限流。
//!
//! 提供 strict、upload、image、comment 四个限流器，
//! 支持从 `X-Forwarded-For` / `X-Real-IP` 中提取客户端 IP，
//! 并可通过 `TRUSTED_PROXY_COUNT` 配置信任代理层数。
//!
//! 当未配置可信代理时，Axum handler 可回退到 TCP 连接的对端地址；
//! Dioxus server function 无法获取对端地址，会退回到 `"unknown"` key，
//! 此时所有请求共享一个限流桶。生产环境应在反向代理后部署并正确配置
//! `TRUSTED_PROXY_COUNT`。
//!
//! 仅在 `feature = "server"` 时生效。

#[cfg(feature = "server")]
use axum::http::StatusCode;
#[cfg(feature = "server")]
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
#[cfg(feature = "server")]
use std::num::NonZeroU32;
#[cfg(feature = "server")]
use std::sync::LazyLock;
#[cfg(feature = "server")]
use std::time::Duration;

#[cfg(feature = "server")]
fn env_or(key: &str, default: u32) -> NonZeroU32 {
    let val = std::env::var(key)
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(default);
    // val.max(1) 保证 ≥ 1，NonZeroU32::new 必然 Some；expect 说明该不变量。
    NonZeroU32::new(val.max(1)).expect("val.max(1) 保证非零，NonZeroU32::new 不可能失败")
}

#[cfg(feature = "server")]
static STRICT_LIMITER: LazyLock<DefaultKeyedRateLimiter<String>> = LazyLock::new(|| {
    RateLimiter::keyed(
        Quota::per_second(env_or("RATE_LIMIT_STRICT_PER_SEC", 1))
            .allow_burst(env_or("RATE_LIMIT_STRICT_BURST", 5)),
    )
});

#[cfg(feature = "server")]
static UPLOAD_LIMITER: LazyLock<DefaultKeyedRateLimiter<String>> = LazyLock::new(|| {
    RateLimiter::keyed(
        Quota::per_second(env_or("RATE_LIMIT_UPLOAD_PER_SEC", 2))
            .allow_burst(env_or("RATE_LIMIT_UPLOAD_BURST", 15)),
    )
});

#[cfg(feature = "server")]
static IMAGE_LIMITER: LazyLock<DefaultKeyedRateLimiter<String>> = LazyLock::new(|| {
    RateLimiter::keyed(
        Quota::per_second(env_or("RATE_LIMIT_IMAGE_PER_SEC", 10))
            .allow_burst(env_or("RATE_LIMIT_IMAGE_BURST", 50)),
    )
});

#[cfg(feature = "server")]
static COMMENT_LIMITER: LazyLock<DefaultKeyedRateLimiter<String>> = LazyLock::new(|| {
    RateLimiter::keyed(
        Quota::per_second(env_or("RATE_LIMIT_COMMENT_PER_SEC", 1))
            .allow_burst(env_or("RATE_LIMIT_COMMENT_BURST", 5)),
    )
});

#[cfg(feature = "server")]
/// 代码执行单 IP 每秒限流（默认 1 req/s，突发 3）。
/// 防止单个客户端高频提交容器任务，与下方日限额共同构成双层速率限制。
static CODE_EXEC_LIMITER: LazyLock<DefaultKeyedRateLimiter<String>> = LazyLock::new(|| {
    RateLimiter::keyed(
        Quota::per_second(env_or("RATE_LIMIT_CODE_EXEC_PER_SEC", 1))
            .allow_burst(env_or("RATE_LIMIT_CODE_EXEC_BURST", 3)),
    )
});

#[cfg(feature = "server")]
/// 代码执行单 IP 每日限额（默认 50 次/天）。
/// 容器执行成本高（CPU/内存/启动延迟），需要硬性日上限防止资源耗尽。
///
/// governor 0.8 无 `Quota::per_day`，用 `with_period(24h)` + `allow_burst(daily)`
/// 模拟：每 24h 补充 1 token、突发上限即日额度，等效于「每日最多 daily 次」。
static CODE_EXEC_DAILY_LIMITER: LazyLock<DefaultKeyedRateLimiter<String>> = LazyLock::new(|| {
    RateLimiter::keyed(
        Quota::with_period(Duration::from_secs(86_400))
            .expect("with_period 仅在 Duration 为 0 时返回 None；86_400s 必然 Some")
            .allow_burst(env_or("RATE_LIMIT_CODE_EXEC_DAILY", 50)),
    )
});

#[cfg(feature = "server")]
/// 当无法识别真实客户端 IP（"unknown"）时使用的宽松限流桶。
///
/// TRUSTED_PROXY_COUNT=0（默认）时，Dioxus server function 拿不到 TCP 对端地址，
/// get_client_ip 会返回 "unknown"，导致所有匿名请求共享同一个严格桶
/// （1 req/s, burst 5），正常用户的高频请求被误杀。此桶阈值更高，
/// 通过 env RATE_LIMIT_UNKNOWN_PER_SEC / RATE_LIMIT_UNKNOWN_BURST 可调。
static UNKNOWN_BUCKET_LIMITER: LazyLock<DefaultKeyedRateLimiter<String>> = LazyLock::new(|| {
    RateLimiter::keyed(
        Quota::per_second(env_or("RATE_LIMIT_UNKNOWN_PER_SEC", 30))
            .allow_burst(env_or("RATE_LIMIT_UNKNOWN_BURST", 100)),
    )
});

/// 限流桶 GC 间隔（秒）：周期性调用 governor 的 `retain_recent`，回收已恢复为
/// 「初始态」的 IP 键，防止 IP 轮换攻击下键空间无限膨胀。
///
/// 默认 300 秒。governor 的 `retain_recent` 只丢弃与「新桶」不可区分的键（即限流
/// 窗口早已冷却、保留与否都不影响后续请求），因此即便间隔较长，内存占用也只反映
/// 「最近活跃过且仍在限流窗口内」的 IP 集合，而非历史全集。可用
/// `RATE_LIMIT_GC_INTERVAL_SECS` 覆盖（值越小越激进，回收越勤）。
#[cfg(feature = "server")]
fn limiter_gc_interval() -> Duration {
    let secs = std::env::var("RATE_LIMIT_GC_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    Duration::from_secs(secs.max(1))
}

/// 启动后台限流桶 GC 任务（全进程仅生效一次）。
///
/// 七个 IP 键控限流器均为独立的 `DefaultKeyedRateLimiter`，没有集中状态表，故采用
/// 惰性启动：首个请求到达任一 `check_*` 时，经 `Once` 派生一个常驻 tokio 任务，按
/// [`limiter_gc_interval`] 周期对全部限流器调用 `retain_recent`，回收长时间未命中的
/// 键。这直接缓解 IP 轮换攻击下的内存膨胀——攻击者不断换 IP 制造新键，GC 周期性剔除
/// 已冷却的旧键，使键集合大小收敛到「限流窗口内的活跃 IP」。
#[cfg(feature = "server")]
fn ensure_limiter_gc() {
    static SPAWNED: std::sync::Once = std::sync::Once::new();
    SPAWNED.call_once(|| {
        // 仅在 tokio 运行时上下文中派生 GC 任务。测试（非 #[tokio::test]）无运行时，
        // tokio::spawn 会 panic 并毒化 Once，使后续调用全部 panic。try_current 在无
        // 运行时时返回 Err，静默跳过——测试不依赖 GC，生产必有运行时。
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let interval = limiter_gc_interval();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                STRICT_LIMITER.retain_recent();
                UPLOAD_LIMITER.retain_recent();
                IMAGE_LIMITER.retain_recent();
                COMMENT_LIMITER.retain_recent();
                CODE_EXEC_LIMITER.retain_recent();
                CODE_EXEC_DAILY_LIMITER.retain_recent();
                UNKNOWN_BUCKET_LIMITER.retain_recent();
            }
        });
    });
}

#[cfg(feature = "server")]
/// 检查评论请求是否超出限流阈值。
pub fn check_comment_limit(ip: &str) -> Result<(), String> {
    ensure_limiter_gc();
    COMMENT_LIMITER
        .check_key(&ip.to_string())
        .map(|_| ())
        .map_err(|_| "评论过于频繁，请稍后再试".to_string())
}

#[cfg(feature = "server")]
/// 检查图片访问请求是否超出限流阈值，返回 HTTP 状态码。
pub fn check_image_limit(ip: &str) -> Result<(), StatusCode> {
    ensure_limiter_gc();
    IMAGE_LIMITER
        .check_key(&ip.to_string())
        .map(|_| ())
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)
}

#[cfg(feature = "server")]
async fn trusted_proxy_count() -> usize {
    crate::api::settings::runtime_security_settings()
        .await
        .trusted_proxy_count as usize
}

#[cfg(feature = "server")]
fn is_valid_ip(ip: &str) -> bool {
    ip.parse::<std::net::IpAddr>().is_ok()
}

/// 从 `X-Forwarded-For` 头按信任代理层数提取真实客户端 IP。
///
/// # 伪造风险（务必正确配置 `TRUSTED_PROXY_COUNT`）
///
/// XFF 头由客户端**可写**，本函数以 `parts[len-1-trusted_proxy_count]` 选取
/// 「可信代理链最左侧之外」的地址。该取值**完全依赖 `TRUSTED_PROXY_COUNT` 与真实
/// 代理跳数精确相等**，一旦不符即可被滥用：
/// - **配得偏大**：会选中客户端伪造的地址——攻击者在请求里塞入多个伪 XFF 段，
///   使选中的「客户端 IP」落为其伪造值，从而绕过按 IP 的限流（每次伪造一个新 IP），
///   或令多个真实用户被错误归并到同一代理 IP 的桶里。
/// - **配得偏小**：会选中某一跳中间代理的 IP，导致所有用户共享同一个限流桶。
///
/// 因此生产部署**必须**由最外层反向代理覆盖/重写 XFF（而非原样转发客户端 XFF），
/// 并令 `TRUSTED_PROXY_COUNT` 等于「客户端 → 服务端」之间的真实代理跳数。
/// 详见 `AGENTS.md` 中 `TRUSTED_PROXY_COUNT` 的部署说明。
#[cfg(feature = "server")]
fn ip_from_x_forwarded_for(value: &str, trusted_proxy_count: usize) -> Option<String> {
    // X-Forwarded-For 格式：client, proxy1, proxy2, ..., proxyN
    // 越靠右的地址离服务端越近。
    let parts: Vec<&str> = value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    if trusted_proxy_count == 0 || parts.len() <= trusted_proxy_count {
        return None;
    }

    // 真实客户端 IP 位于右侧第 trusted_proxy_count + 1 个。
    let idx = parts.len() - 1 - trusted_proxy_count;
    let ip = parts[idx].to_string();
    if is_valid_ip(&ip) {
        Some(ip)
    } else {
        None
    }
}

#[cfg(feature = "server")]
fn ip_from_x_real_ip(value: &str) -> Option<String> {
    let ip = value.trim().to_string();
    if is_valid_ip(&ip) {
        Some(ip)
    } else {
        None
    }
}

#[cfg(feature = "server")]
fn get_client_ip_internal(
    headers: &http::HeaderMap,
    trusted: usize,
    peer: Option<std::net::SocketAddr>,
) -> String {
    if trusted > 0 {
        if let Some(value) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(ip) = ip_from_x_forwarded_for(value, trusted) {
                return ip;
            }
        }

        if let Some(ip) = headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(ip_from_x_real_ip)
        {
            return ip;
        }
    }

    if let Some(addr) = peer {
        return addr.ip().to_string();
    }

    // Server function 等非 Axum 上下文无法获取对端地址，退回到 unknown。
    // 此时所有请求共享一个限流桶，生产环境应在反向代理后部署。
    tracing::warn!(
        "无法获取客户端真实 IP（未配置 TRUSTED_PROXY_COUNT 且无法读取 TCP 对端地址），\
         限流将按 'unknown' 键聚合"
    );
    "unknown".to_string()
}

#[cfg(feature = "server")]
/// 根据信任代理层数从请求头中提取客户端 IP，并校验 IP 合法性。
///
/// 当未配置可信代理时，不会信任任何 `X-Forwarded-For` / `X-Real-IP` 头，
/// 而是直接返回 `peer` 中的 TCP 对端地址（如果提供）。
pub async fn get_client_ip_with_peer(
    headers: &http::HeaderMap,
    peer: Option<std::net::SocketAddr>,
) -> String {
    get_client_ip_internal(headers, trusted_proxy_count().await, peer)
}
#[cfg(feature = "server")]
/// 使用「站点配置 → 安全」面板的代理层数提取客户端 IP。
///
/// 适用于 Dioxus server function 等无法获取 `ConnectInfo` 的场景。
/// 生产环境建议配合反向代理与设置面板的 TRUSTED_PROXY_COUNT 使用。
pub async fn get_client_ip(headers: &http::HeaderMap) -> String {
    get_client_ip_internal(headers, trusted_proxy_count().await, None)
}

#[cfg(feature = "server")]
/// 检查严格限流（注册、登录等敏感接口）。
///
/// 当 IP 为 "unknown"（无法识别真实客户端，通常是 TRUSTED_PROXY_COUNT=0
/// 且调用方为 Dioxus server function 时）改用宽松桶，避免所有匿名请求共享
/// 严格桶导致正常用户被误杀。生产环境配好 TRUSTED_PROXY_COUNT 后走真实 IP，
/// 始终命中严格桶。
pub fn check_strict_limit(ip: &str) -> Result<(), String> {
    ensure_limiter_gc();
    if ip == "unknown" {
        UNKNOWN_BUCKET_LIMITER
            .check_key(&ip.to_string())
            .map(|_| ())
            .map_err(|_| "服务繁忙，请稍后再试".to_string())
    } else {
        STRICT_LIMITER
            .check_key(&ip.to_string())
            .map(|_| ())
            .map_err(|_| "请求过于频繁，请稍后再试".to_string())
    }
}

#[cfg(feature = "server")]
/// 检查上传请求是否超出限流阈值。
pub fn check_upload_limit(ip: &str) -> Result<(), String> {
    ensure_limiter_gc();
    UPLOAD_LIMITER
        .check_key(&ip.to_string())
        .map(|_| ())
        .map_err(|_| "上传过于频繁，请稍后再试".to_string())
}

#[cfg(feature = "server")]
/// 检查代码执行请求的双层速率限制（每秒突发 + 每日总额）。
///
/// 两层任一被限即拒绝。返回中文错误消息，供 server function 直接透传给前端。
pub fn check_code_exec_limit(ip: &str) -> Result<(), String> {
    ensure_limiter_gc();
    CODE_EXEC_LIMITER
        .check_key(&ip.to_string())
        .map_err(|_| "请求过于频繁，请稍后再试".to_string())?;
    CODE_EXEC_DAILY_LIMITER
        .check_key(&ip.to_string())
        .map_err(|_| "今日运行次数已达上限，请明天再试".to_string())?;
    Ok(())
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use http::HeaderMap;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn get_client_ip_from_x_forwarded_for_with_one_trusted_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 1, None),
            "1.2.3.4"
        );
    }

    #[test]
    fn get_client_ip_from_x_forwarded_for_with_two_trusted_proxies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "1.2.3.4, 5.6.7.8, 9.10.11.12".parse().unwrap(),
        );
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 2, None),
            "1.2.3.4"
        );
    }

    #[test]
    fn get_client_ip_ignores_x_forwarded_for_when_no_trusted_proxies() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 0, None),
            "unknown"
        );
    }

    #[test]
    fn get_client_ip_falls_back_to_peer_when_no_trusted_proxies() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345);
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 0, Some(peer)),
            "127.0.0.1"
        );
    }

    #[test]
    fn get_client_ip_from_x_real_ip_when_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 1, None),
            "9.8.7.6"
        );
    }

    #[test]
    fn get_client_ip_x_real_ip_ignored_when_not_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "9.8.7.6".parse().unwrap());
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 12345);
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 0, Some(peer)),
            "192.168.1.1"
        );
    }

    #[test]
    fn get_client_ip_x_forwarded_for_takes_priority_over_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.1.1.1, 2.2.2.2".parse().unwrap());
        headers.insert("x-real-ip", "3.3.3.3".parse().unwrap());
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 1, None),
            "1.1.1.1"
        );
    }

    #[test]
    fn get_client_ip_no_headers_returns_unknown() {
        let headers = HeaderMap::new();
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 1, None),
            "unknown"
        );
    }

    #[test]
    fn get_client_ip_ignores_short_x_forwarded_for_list() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 2, None),
            "unknown"
        );
    }

    #[test]
    fn get_client_ip_ignores_x_forwarded_for_equal_to_proxy_count() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 2, None),
            "unknown"
        );
    }

    #[test]
    fn get_client_ip_ignores_empty_x_forwarded_for_entries() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            " , 1.2.3.4 , 5.6.7.8 , ".parse().unwrap(),
        );
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 1, None),
            "1.2.3.4"
        );
    }

    #[test]
    fn get_client_ip_rejects_invalid_x_forwarded_for_value() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "not-an-ip, 5.6.7.8".parse().unwrap());
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 1, None),
            "unknown"
        );
    }

    #[test]
    fn get_client_ip_rejects_invalid_x_real_ip_value() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "not-an-ip".parse().unwrap());
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 1, None),
            "unknown"
        );
    }

    #[test]
    fn get_client_ip_prefers_xff_over_peer() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 12345);
        assert_eq!(
            get_client_ip_with_trusted_and_peer(&headers, 1, Some(peer)),
            "1.2.3.4"
        );
    }

    #[tokio::test]
    async fn get_client_ip_defaults_to_unknown_without_db() {
        // 无 DB 连接的单元测试环境，trusted_proxy_count 回退默认 0，
        // 不信任任何 XFF 头，返回 "unknown"。
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(get_client_ip(&headers).await, "unknown");
    }

    #[test]
    #[serial_test::serial]
    fn check_strict_unknown_ip_uses_lenient_bucket() {
        // "unknown" 桶 burst 为 100，少量请求应全部放行，不被严格桶误杀。
        // 用 serial 隔离，因为 UNKNOWN_BUCKET_LIMITER 是全局状态。
        for _ in 0..20 {
            assert!(
                super::check_strict_limit("unknown").is_ok(),
                "unknown bucket should allow small bursts, not hit strict 1 req/s limit"
            );
        }
    }

    #[test]
    #[serial_test::serial]
    fn check_strict_real_ip_uses_strict_bucket() {
        // 真实 IP 命中严格桶（1 req/s, burst 5）。连发超过 burst 应被限流。
        // 用一个唯一的 IP 避免与其他测试状态冲突。
        let unique_ip = "198.51.100.42";
        let mut allowed = 0;
        let mut blocked = false;
        for _ in 0..50 {
            match super::check_strict_limit(unique_ip) {
                Ok(()) => allowed += 1,
                Err(_) => blocked = true,
            }
            if blocked {
                break;
            }
        }
        assert!(
            blocked,
            "strict bucket should eventually block real IP burst"
        );
        assert!(
            allowed <= 6,
            "strict burst is 5, allowed should be <= 6, got {allowed}"
        );
    }

    // 测试辅助函数：绕过环境变量读取，直接指定 trusted_proxy_count。
    fn get_client_ip_with_trusted_and_peer(
        headers: &HeaderMap,
        trusted: usize,
        peer: Option<SocketAddr>,
    ) -> String {
        get_client_ip_internal(headers, trusted, peer)
    }
}
