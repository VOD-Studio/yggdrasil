//! 内容变更后清除 EdgeOne 节点上缓存的公开页 HTML。
//!
//! 白名单匿名页带 `s-maxage`（`SsrCacheSettings::edge_ttl_secs`）时，CDN 节点会缓存 HTML；
//! 源站 HTML 缓存每次失效（文章、标签、设置写入，跨实例通知，发版启动）都经
//! `request_purge` 发信号，本任务：
//!
//! 1. 防抖 `DEBOUNCE`：一次写入会连续触发多次失效，批量操作更多，合并成一次；
//! 2. 立刻清一次，`SECOND_PASS_DELAY` 后再清一次：写入前已开始渲染的请求可能在第一次清除
//!    之后才返回旧页面，EdgeOne 会把它按 `s-maxage` 缓存下来（源站缓存有世代号挡住，边缘没有）；
//! 3. 失败按 `RETRY_DELAYS` 重试，最终失败记 error，陈旧上限退回到 `s-maxage`。
//!
//! 清除范围是 `ssr_cache` 的白名单：前缀按目录清除，固定页面按 URL 清除。不用 Hostname 或
//! 全站清除，否则会连带 `/pkg/` 与静态资源，节点重新回源时可能缓存 EdgeOne 自己压缩的
//! 更大的 br（见 `docs/首屏性能实测-2026-10-28.md`）。
//!
//! 启动时路由装配会使源站缓存失效，所以每次发版或重启都会清一次：边缘不会留下引用已删除
//! `/pkg/` 分包的旧 HTML（`YGG_PREVIOUS_PUBLIC_DIR` 只保留上一版）。
//!
//! 凭据只读环境变量（`EDGEONE_ZONE_ID`、`TENCENTCLOUD_SECRET_ID`、`TENCENTCLOUD_SECRET_KEY`），
//! 缺任一项时任务不启动，信号只是空操作。建议用只授权该站点 `teo:CreatePurgeTask` 的 CAM 子账号。

use std::time::Duration;

use futures::FutureExt;
use sha2::{Digest, Sha256};
use tokio::sync::Notify;

const API_HOST: &str = "teo.tencentcloudapi.com";
const API_SERVICE: &str = "teo";
const API_VERSION: &str = "2022-09-01";
const API_ACTION: &str = "CreatePurgeTask";
/// 与腾讯云 SDK 一致：签名头只有 `content-type;host`，内容类型不带 charset。
const CONTENT_TYPE: &str = "application/json";

const DEBOUNCE: Duration = Duration::from_secs(2);
/// 实测 EdgeOne 完成一次目录或 URL 清除要 9–20 秒（2026-11-10）。第二轮要落在第一轮完成之后，
/// 才能清掉第一轮进行期间节点从源站拉回的旧页面。
const SECOND_PASS_DELAY: Duration = Duration::from_secs(30);
const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

static SIGNAL: Notify = Notify::const_new();

/// 请求清除边缘 HTML 缓存。只发信号，立即返回；未配置凭据时没有任务在等，信号被忽略。
pub fn request_purge() {
    SIGNAL.notify_one();
}

struct Credentials {
    zone_id: String,
    secret_id: String,
    secret_key: String,
}

impl Credentials {
    fn from_env() -> Option<Self> {
        let read = |name: &str| {
            std::env::var(name)
                .ok()
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let zone_id = read("EDGEONE_ZONE_ID");
        let secret_id = read("TENCENTCLOUD_SECRET_ID");
        let secret_key = read("TENCENTCLOUD_SECRET_KEY");
        match (zone_id, secret_id, secret_key) {
            (Some(zone_id), Some(secret_id), Some(secret_key)) => Some(Self {
                zone_id,
                secret_id,
                secret_key,
            }),
            (None, None, None) => None,
            _ => {
                tracing::warn!(
                    "EDGEONE_ZONE_ID / TENCENTCLOUD_SECRET_ID / TENCENTCLOUD_SECRET_KEY 未配置齐，不清边缘缓存"
                );
                None
            }
        }
    }
}

/// 是否配置了清边缘缓存的凭据（启动日志用）。
pub fn is_configured() -> bool {
    [
        "EDGEONE_ZONE_ID",
        "TENCENTCLOUD_SECRET_ID",
        "TENCENTCLOUD_SECRET_KEY",
    ]
    .iter()
    .all(|name| std::env::var(name).is_ok_and(|v| !v.trim().is_empty()))
}

/// 常驻任务：等待信号，防抖后清两次。
pub async fn run() {
    let Some(credentials) = Credentials::from_env() else {
        return;
    };
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .user_agent("yggdrasil-edge-purge/1.0")
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(%error, "边缘缓存清理客户端初始化失败，不清边缘缓存");
            return;
        }
    };
    tracing::info!(zone_id = %credentials.zone_id, "EdgeOne HTML 缓存清理已启用");

    loop {
        SIGNAL.notified().await;
        tokio::time::sleep(DEBOUNCE).await;
        // 防抖窗口内的后续信号由这一轮覆盖。
        let _ = SIGNAL.notified().now_or_never();

        let base = crate::api::settings::runtime_security_settings()
            .await
            .app_base_url;
        let Some(targets) = PurgeTargets::for_base(&base) else {
            tracing::warn!("APP_BASE_URL 未设置或不是 http(s) 地址，无法清边缘缓存");
            continue;
        };
        purge_with_retry(&client, &credentials, &targets, "首次").await;
        // 第二轮期间到达的信号留到下一轮，各自也会清两次。
        tokio::time::sleep(SECOND_PASS_DELAY).await;
        purge_with_retry(&client, &credentials, &targets, "延迟").await;
    }
}

/// 一轮清除的目标：目录（`purge_prefix`）与固定页面（`purge_url`）。
#[derive(Debug, PartialEq)]
struct PurgeTargets {
    prefixes: Vec<String>,
    pages: Vec<String>,
}

impl PurgeTargets {
    fn for_base(base: &str) -> Option<Self> {
        let base = base.trim().trim_end_matches('/');
        let host = base
            .strip_prefix("https://")
            .or_else(|| base.strip_prefix("http://"))?;
        if host.is_empty() || host.contains('/') {
            return None;
        }
        let join = |path: &str| format!("{base}{path}");
        Some(Self {
            prefixes: crate::ssr_cache::CACHEABLE_PREFIXES
                .iter()
                .map(|p| join(p))
                .collect(),
            pages: crate::ssr_cache::CACHEABLE_PAGES
                .iter()
                .map(|p| join(p))
                .collect(),
        })
    }
}

async fn purge_with_retry(
    client: &reqwest::Client,
    credentials: &Credentials,
    targets: &PurgeTargets,
    pass: &'static str,
) {
    let tasks = [
        (
            "purge_prefix",
            serde_json::json!({
                "ZoneId": credentials.zone_id,
                "Type": "purge_prefix",
                // delete：不论源站内容是否变化都删除节点缓存，行为确定。
                "Method": "delete",
                "Targets": targets.prefixes,
            }),
        ),
        (
            "purge_url",
            serde_json::json!({
                "ZoneId": credentials.zone_id,
                "Type": "purge_url",
                "Targets": targets.pages,
            }),
        ),
    ];
    for (kind, body) in tasks {
        let payload = body.to_string();
        let mut attempt = 0;
        loop {
            match create_purge_task(client, credentials, &payload).await {
                Ok(job_id) => {
                    tracing::info!(pass, kind, %job_id, "已提交 EdgeOne HTML 缓存清除");
                    break;
                }
                Err(error) if attempt < RETRY_DELAYS.len() => {
                    tracing::warn!(pass, kind, attempt, %error, "EdgeOne 缓存清除失败，稍后重试");
                    tokio::time::sleep(RETRY_DELAYS[attempt]).await;
                    attempt += 1;
                }
                Err(error) => {
                    tracing::error!(
                        pass,
                        kind,
                        %error,
                        "EdgeOne 缓存清除最终失败，边缘 HTML 要等 s-maxage 到期才更新"
                    );
                    break;
                }
            }
        }
    }
}

async fn create_purge_task(
    client: &reqwest::Client,
    credentials: &Credentials,
    payload: &str,
) -> Result<String, String> {
    let timestamp = chrono::Utc::now().timestamp();
    let authorization = authorization(
        &credentials.secret_id,
        &credentials.secret_key,
        timestamp,
        payload,
    );
    let response = client
        .post(format!("https://{API_HOST}/"))
        .header("Content-Type", CONTENT_TYPE)
        .header("X-TC-Action", API_ACTION)
        .header("X-TC-Version", API_VERSION)
        .header("X-TC-Timestamp", timestamp.to_string())
        .header("Authorization", authorization)
        .body(payload.to_owned())
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("读取响应失败：{e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    parse_response(&text)
}

/// 解析云 API 响应：`Response.Error` 或非空 `FailedList` 视为失败，成功返回 `JobId`。
fn parse_response(text: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| "响应不是 JSON".to_owned())?;
    let response = &value["Response"];
    if let Some(error) = response.get("Error") {
        return Err(format!(
            "{}: {}",
            error["Code"].as_str().unwrap_or("Unknown"),
            error["Message"].as_str().unwrap_or("")
        ));
    }
    if let Some(failed) = response["FailedList"].as_array().filter(|l| !l.is_empty()) {
        let failed: Vec<String> = failed
            .iter()
            .map(|item| {
                format!(
                    "{} ({})",
                    item["Target"].as_str().unwrap_or("?"),
                    item["Reason"].as_str().unwrap_or("?")
                )
            })
            .collect();
        return Err(format!("部分目标失败：{}", failed.join(", ")));
    }
    Ok(response["JobId"].as_str().unwrap_or("").to_owned())
}

/// TC3-HMAC-SHA256 签名，构造方式与腾讯云 Python SDK（`_get_tc3_signature`）一致。
fn authorization(secret_id: &str, secret_key: &str, timestamp: i64, payload: &str) -> String {
    let date = chrono::DateTime::from_timestamp(timestamp, 0)
        .unwrap_or_default()
        .format("%Y-%m-%d")
        .to_string();
    let canonical_request = format!(
        "POST\n/\n\ncontent-type:{CONTENT_TYPE}\nhost:{API_HOST}\n\ncontent-type;host\n{}",
        hex::encode(Sha256::digest(payload.as_bytes()))
    );
    let scope = format!("{date}/{API_SERVICE}/tc3_request");
    let string_to_sign = format!(
        "TC3-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );
    let key = hmac_sha256(format!("TC3{secret_key}").as_bytes(), date.as_bytes());
    let key = hmac_sha256(&key, API_SERVICE.as_bytes());
    let key = hmac_sha256(&key, b"tc3_request");
    let signature = hex::encode(hmac_sha256(&key, string_to_sign.as_bytes()));
    format!(
        "TC3-HMAC-SHA256 Credential={secret_id}/{scope}, SignedHeaders=content-type;host, Signature={signature}"
    )
}

/// HMAC-SHA256（RFC 2104）。只此一处用到，不为它引入 `hmac` crate。
fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    const BLOCK: usize = 64;
    let mut block_key = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digest = Sha256::digest(key);
        block_key[..digest.len()].copy_from_slice(&digest);
    } else {
        block_key[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block_key.map(|b| b ^ 0x36));
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(block_key.map(|b| b ^ 0x5c));
    outer.update(inner);
    outer.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc_4231() {
        // RFC 4231 测试用例 2 与 6（密钥长于块大小）。
        assert_eq!(
            hex::encode(hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        assert_eq!(
            hex::encode(hmac_sha256(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn signature_matches_tencentcloud_sdk() {
        // 期望值由腾讯云 Python SDK 的 `Sign.sign_tc3` 按同样的规范请求算出。
        let payload =
            r#"{"ZoneId":"zone-test","Type":"purge_url","Targets":["https://example.com/"]}"#;
        assert_eq!(
            authorization("AKIDtest", "test-secret-key", 1_762_761_600, payload),
            "TC3-HMAC-SHA256 Credential=AKIDtest/2025-11-10/teo/tc3_request, \
             SignedHeaders=content-type;host, \
             Signature=c5074f1f2ae43492077ae5622538a500c6a5af8fc435886f2602c36218bf845b"
        );
    }

    #[test]
    fn targets_cover_the_ssr_cache_whitelist() {
        let targets = PurgeTargets::for_base(" https://rua.plus/ ").unwrap();
        assert_eq!(
            targets.prefixes,
            [
                "https://rua.plus/post/",
                "https://rua.plus/page/",
                "https://rua.plus/tags/"
            ]
        );
        assert_eq!(
            targets.pages,
            [
                "https://rua.plus/",
                "https://rua.plus/archives",
                "https://rua.plus/friends",
                "https://rua.plus/about",
                "https://rua.plus/changelog"
            ]
        );
        for base in [
            "",
            "rua.plus",
            "ftp://rua.plus",
            "https://",
            "https://rua.plus/blog",
        ] {
            assert_eq!(PurgeTargets::for_base(base), None, "{base}");
        }
    }

    #[test]
    fn response_errors_and_failed_targets_are_failures() {
        assert_eq!(
            parse_response(r#"{"Response":{"JobId":"job-1","FailedList":[],"RequestId":"r"}}"#),
            Ok("job-1".to_owned())
        );
        assert_eq!(
            parse_response(
                r#"{"Response":{"Error":{"Code":"AuthFailure.SignatureFailure","Message":"bad"},"RequestId":"r"}}"#
            ),
            Err("AuthFailure.SignatureFailure: bad".to_owned())
        );
        assert_eq!(
            parse_response(
                r#"{"Response":{"JobId":"job-2","FailedList":[{"Target":"https://rua.plus/","Reason":"quota"}]}}"#
            ),
            Err("部分目标失败：https://rua.plus/ (quota)".to_owned())
        );
        assert!(parse_response("<html>").is_err());
    }
}
