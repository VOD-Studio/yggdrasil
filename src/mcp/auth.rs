//! MCP 请求鉴权：bearer token → 已认证主体。
//!
//! 流程（每请求）：
//! 1. axum `from_fn` 中间件 [`mcp_auth_middleware`] 解析 `Authorization: Bearer ygg_...`；
//! 2. SHA-256 哈希后在 `mcp_tokens` 表常量查找未撤销、未过期的行，并 JOIN `users`
//!    要求持有人当前角色仍允许该作用域（见 [`role_can_hold_scope`]）；
//! 3. 命中则把 [`McpPrincipal`] { user_id, scope, token_id } 注入 request.extensions，
//!    并异步刷新 `last_used_at`；
//! 4. 未命中/缺失 → 401（不区分原因，避免探测）。
//!
//! Origin→403、协议版本头、体积上限由 rmcp 的 `StreamableHttpServerConfig` 内置，
//! 本模块只管 bearer 鉴权。`McpPrincipal` 经 rmcp 的 `Extension<http::request::Parts>`
//! 提取器在工具内读取（见 `server.rs`）。

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::db::pool::get_conn;
use crate::models::mcp_token::TokenScope;
use crate::models::user::UserRole;

/// Bearer token 前缀（明文形式以 `ygg_` 起头，便于人眼识别与日志脱敏）。
pub const TOKEN_PREFIX: &str = "ygg_";

/// 已认证主体：由中间件注入 request.extensions，工具经 Extension 提取器读取。
#[derive(Clone, Debug)]
pub struct McpPrincipal {
    pub user_id: i32,
    pub scope: TokenScope,
    /// 令牌 DB id（String，与 model 一致；用于审计/last_used_at 刷新）。
    pub token_id: String,
}

/// 从 Authorization 头解析 bearer 明文（去前缀后返回完整 token 字符串）。
/// 非 bearer、缺前缀、解码失败统一返回 None。
pub fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let scheme = raw.strip_prefix("Bearer ")?.trim();
    if scheme.starts_with(TOKEN_PREFIX) {
        Some(scheme.to_string())
    } else {
        None
    }
}

/// 明文 token → SHA-256 hex（与 mcp_tokens.token_hash 列一致，用于 DB 查找）。
pub fn hash_token(token: &str) -> String {
    crate::utils::server::sha256_hex(token)
}

/// MCP 鉴权中间件：无 bearer 或 token 无效 → 401；否则注入 McpPrincipal。
///
/// 注意：这是 T1 的最小实现——每请求同步查库 + 同步更新 last_used_at。
/// T6 会把 last_used_at 刷新改为节流（批量/惰性）以减负，鉴权查询本身保持同步
/// （这是认证的必要代价，无法乐观）。
/// MCP 限流：按 token 计数的 governor 桶（与 web 的 IP-keyed 限流隔离）。
///
/// key 是 token_id（而非 user_id）：同一用户的多个令牌各自有独立配额，
/// 避免一个泄露的高频令牌耗尽其它令牌的额度。阈值经
/// RATE_LIMIT_MCP_PER_SEC / RATE_LIMIT_MCP_BURST 可调（默认 10/s, burst 30）。
static MCP_LIMITER: std::sync::LazyLock<governor::DefaultKeyedRateLimiter<String>> =
    std::sync::LazyLock::new(|| {
        governor::RateLimiter::keyed(
            governor::Quota::per_second(mcp_rate_per_sec()).allow_burst(mcp_rate_burst()),
        )
    });

fn mcp_rate_per_sec() -> std::num::NonZeroU32 {
    let v = std::env::var("RATE_LIMIT_MCP_PER_SEC")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(10);
    std::num::NonZeroU32::new(v.max(1)).expect("v.max(1) 保证非零")
}

fn mcp_rate_burst() -> std::num::NonZeroU32 {
    let v = std::env::var("RATE_LIMIT_MCP_BURST")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(30);
    std::num::NonZeroU32::new(v.max(1)).expect("v.max(1) 保证非零")
}
/// MCP 上传专用限流（与 MCP_LIMITER 隔离）：上传是重操作（转码 + 落盘），
/// 单独配额避免一个高频令牌的普通请求耗尽上传额度，反之亦然。
/// 阈值经 RATE_LIMIT_MCP_UPLOAD_PER_SEC / _BURST 可调（默认 2/s, burst 5）。
static MCP_UPLOAD_LIMITER: std::sync::LazyLock<governor::DefaultKeyedRateLimiter<String>> =
    std::sync::LazyLock::new(|| {
        governor::RateLimiter::keyed(
            governor::Quota::per_second(mcp_upload_rate_per_sec())
                .allow_burst(mcp_upload_rate_burst()),
        )
    });

fn mcp_upload_rate_per_sec() -> std::num::NonZeroU32 {
    let v = std::env::var("RATE_LIMIT_MCP_UPLOAD_PER_SEC")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(2);
    std::num::NonZeroU32::new(v.max(1)).expect("v.max(1) 保证非零")
}

fn mcp_upload_rate_burst() -> std::num::NonZeroU32 {
    let v = std::env::var("RATE_LIMIT_MCP_UPLOAD_BURST")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(5);
    std::num::NonZeroU32::new(v.max(1)).expect("v.max(1) 保证非零")
}

/// bearer 端点专用：按 token_id 检查上传配额。超限返回 Err(脱敏消息)。
pub(crate) fn check_mcp_upload_limit(token_id: &str) -> Result<(), &'static str> {
    // governor 的 keyed 限流器要求 &String 键；token_id 是 &str，转一次。
    let key = token_id.to_string();
    if MCP_UPLOAD_LIMITER.check_key(&key).is_err() {
        return Err("上传过于频繁，请稍后再试");
    }
    Ok(())
}

/// `last_used_at` 刷新节流：同一令牌的 UPDATE 至少间隔 60s，避免高频请求
/// 每次都写库。窗口外才刷新，窗口内跳过（best-effort，失败静默）。
const LAST_USED_REFRESH_SECS: i64 = 60;

pub async fn mcp_auth_middleware(
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = match extract_bearer(req.headers()) {
        Some(t) => t,
        None => return Err(StatusCode::UNAUTHORIZED),
    };
    let principal = resolve_principal(&token)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // 限流：按 token_id 计数。超限返回 429（Too Many Requests）。
    if MCP_LIMITER.check_key(&principal.token_id).is_err() {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    // 审计：记录每次已认证的 MCP 请求（token_id + scope + user_id）。
    // MCP 令牌是自主 AI 客户端，记录其活动有安全价值（事后可追溯滥用）。
    tracing::info!(
        user_id = principal.user_id,
        scope = principal.scope.as_str(),
        token_id = %principal.token_id,
        "mcp request authenticated"
    );

    req.extensions_mut().insert(principal);
    Ok(next.run(req).await)
}

/// 令牌持有人的当前角色是否仍允许持有该作用域的令牌。
///
/// 与签发规则一致：只有管理员能签发令牌（任意作用域），被封禁的用户不能
/// 持有任何令牌。账号被封禁或降级后，名下令牌在下一次请求时即失效，
/// 不需要逐个手工吊销。
pub(crate) fn role_can_hold_scope(role: &UserRole, scope: TokenScope) -> bool {
    match (role, scope) {
        (UserRole::Admin, TokenScope::Read | TokenScope::Write | TokenScope::Admin) => true,
        (UserRole::Blocked, _) => false,
    }
}

/// 查库解析 token → 主体；未撤销、未过期且持有人角色仍允许该作用域才返回 Some。
pub(crate) async fn resolve_principal(token: &str) -> Option<McpPrincipal> {
    let hash = hash_token(token);
    let client = get_conn().await.ok()?;
    // 一次查询取出 + 校验所有条件（含持有人当前角色），并带出 last_used_at 供节流判断。
    let row = client
        .query_opt(
            "SELECT t.id, t.user_id, t.scope, t.last_used_at, u.role
             FROM mcp_tokens t
             JOIN users u ON u.id = t.user_id
             WHERE t.token_hash = $1
               AND t.revoked_at IS NULL
               AND (t.expires_at IS NULL OR t.expires_at > NOW())",
            &[&hash],
        )
        .await
        .ok()??;

    let token_id: uuid::Uuid = row.get(0);
    let user_id: i32 = row.get(1);
    let scope_str: &str = row.get(2);
    let scope = TokenScope::from_db(scope_str)?;
    let last_used: Option<chrono::DateTime<chrono::Utc>> = row.get(3);
    let role = UserRole::from_str(row.get(4))?;
    if !role_can_hold_scope(&role, scope) {
        return None;
    }

    // 节流刷新 last_used_at：仅在 NULL 或距上次 ≥ 60s 时写库，避免高频请求每次 UPDATE。
    let needs_refresh = last_used
        .map(|t| (chrono::Utc::now() - t).num_seconds() >= LAST_USED_REFRESH_SECS)
        .unwrap_or(true);
    if needs_refresh {
        let _ = client
            .execute(
                "UPDATE mcp_tokens SET last_used_at = NOW() WHERE id = $1",
                &[&token_id],
            )
            .await;
    }

    Some(McpPrincipal {
        user_id,
        scope,
        token_id: token_id.to_string(),
    })
}
/// bearer 端点鉴权：从 Authorization 头解析 bearer → 查库解析主体。
///
/// 供 `/api/mcp/upload` 等不在 `/mcp` 中间件链上的端点复用——这些端点
/// 自行做 bearer 鉴权（不经 rmcp，不走 McpPrincipal→extensions 注入路径）。
/// 失败统一返回 401（不区分缺失/无效/过期，避免 token 探测）。
pub(crate) async fn resolve_bearer_principal(
    headers: &HeaderMap,
) -> Result<McpPrincipal, StatusCode> {
    let token = extract_bearer(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    resolve_principal(&token)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_role_keeps_every_token_scope() {
        for scope in [TokenScope::Read, TokenScope::Write, TokenScope::Admin] {
            assert!(role_can_hold_scope(&UserRole::Admin, scope), "{scope:?}");
        }
    }

    #[test]
    fn blocked_role_loses_every_token_scope() {
        for scope in [TokenScope::Read, TokenScope::Write, TokenScope::Admin] {
            assert!(!role_can_hold_scope(&UserRole::Blocked, scope), "{scope:?}");
        }
    }
}
