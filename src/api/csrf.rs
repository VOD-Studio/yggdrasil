//! CSRF 防护：对写请求校验 Origin（回退 Referer）必须等于本站。
//!
//! SameSite=Lax 只在顶级 GET 导航时自动带 cookie，对跨站 POST 不带 cookie，
//! 挡住了大部分经典 CSRF。但存在两个 Lax 覆盖不到的盲区：
//! 1. 登录 CSRF（攻击者诱导受害者登录攻击者账号，Lax 不阻止「设置」cookie）；
//! 2. 未来若出现 GET 化写接口，Lax 会在顶级 GET 导航时带 cookie。
//! 因此对所有写请求叠加 Origin 校验作为纵深防御。
//! 仅在 `feature = "server"` 时编译。

#[cfg(feature = "server")]
use axum::http::{HeaderMap, Method, Request};

/// 判断请求是否需要 CSRF 校验：非简单方法（POST/PUT/PATCH/DELETE）需要。
#[cfg(feature = "server")]
fn is_write_method(method: &Method) -> bool {
    matches!(
        method,
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    )
}

/// 启动时检查 `APP_BASE_URL` 是否已设置，未设置则打一条 WARN。
///
/// [`trusted_origin`] 在拿不到该变量时会回退到请求 `Host` 头推导本站 origin，
/// 反向代理后若 `Host` 头可被客户端影响，该回退路径可被 CSRF 绕过。
/// 生产环境应显式设置该变量为站点完整 origin（如 `https://your-domain.example`）。
///
/// 本地开发同样会触发（默认不设 `APP_BASE_URL`），代价仅是启动时一条 WARN，
/// 远小于误判 localhost 的复杂度。与 `image.rs` 的启动告警同范式：一次性 WARN，
/// 不污染每请求路径。
#[cfg(feature = "server")]
pub fn warn_if_app_base_url_unset() {
    if app_base_url_is_set() {
        return;
    }
    tracing::warn!(
        "APP_BASE_URL 未设置。CSRF 校验将回退到请求 Host 头推导本站 origin，\
         反向代理后若 Host 头可被客户端影响存在绕过风险。\
         生产环境应显式设置为站点完整 origin，如 https://your-domain.example。"
    );
}

/// `APP_BASE_URL` 是否已设置为非空值。纯函数，便于测试。
#[cfg(feature = "server")]
fn app_base_url_is_set() -> bool {
    std::env::var("APP_BASE_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

/// 从 `scheme://host[:port][/path][?query]` 提取标准化的 `scheme://host[:port]`，
/// 端口为默认值（http=80, https=443）时省略。
///
/// 不引入 url crate：Origin 头本身就是 `scheme://host[:port]`（无路径），
/// Referer 需要剥离 path/query，用简单的 split 即可。
#[cfg(feature = "server")]
fn normalize_origin(input: &str) -> String {
    // 取 authority 之前的部分作为 scheme，以及第一个 '/' 之前的部分作为 authority。
    let (scheme, rest) = match input.split_once("://") {
        Some(pair) => pair,
        None => return input.to_string(),
    };
    // rest 形如 host[:port]/path?query，去掉首个 '/' 及之后内容。
    let authority = match rest.split_once('/') {
        Some((auth, _)) => auth,
        None => rest,
    };
    // 省略默认端口。
    match authority.rsplit_once(':') {
        Some((host, port)) if port == "80" || port == "443" => {
            format!("{}://{}", scheme, host)
        }
        _ => format!("{}://{}", scheme, authority),
    }
}

/// 从请求头解析来源站点（Origin 优先，回退 Referer）。
///
/// 返回标准化的 `scheme://host[:port]`。两者都缺失时返回 None（视为不可信）。
#[cfg(feature = "server")]
fn extract_origin(headers: &HeaderMap) -> Option<String> {
    if let Some(origin) = headers.get(axum::http::header::ORIGIN) {
        return origin.to_str().ok().map(normalize_origin);
    }
    headers
        .get(axum::http::header::REFERER)
        .and_then(|v| v.to_str().ok())
        .map(normalize_origin)
}

/// 计算本站可信 origin：优先「站点配置 → 安全」面板的 APP_BASE_URL（即时生效），
/// 否则用请求 Host 头 + `X-Forwarded-Proto`（反代后）或 https 推导。
///
/// 返回 None 表示无法确定本站 origin（此时放行，避免误杀——CSRF 漏判
/// 是请求被拒，但拿不到本站 origin 时误杀合法请求代价更高，故保守放行）。
#[cfg(feature = "server")]
async fn trusted_origin(headers: &HeaderMap) -> Option<String> {
    let base = crate::api::settings::runtime_security_settings()
        .await
        .app_base_url;
    if !base.is_empty() {
        return Some(normalize_origin(&base));
    }
    let host = headers.get(axum::http::header::HOST)?.to_str().ok()?;
    let proto = headers
        .get("X-Forwarded-Proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("https");
    Some(normalize_origin(&format!("{}://{}", proto, host)))
}

/// CSRF 校验中间件。
///
/// 对写方法校验请求来源等于本站；不匹配返回 403。GET/OPTIONS 等放行。
/// 拿不到本站 origin 或请求来源时放行（见 trusted_origin 注释）。
#[cfg(feature = "server")]
pub async fn csrf_middleware(
    req: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if is_write_method(req.method()) {
        let headers = req.headers().clone();
        let trusted = trusted_origin(&headers).await;
        let incoming = extract_origin(&headers);
        let ok = match (&trusted, &incoming) {
            (Some(t), Some(o)) => t == o,
            // 拿不到本站 origin 或请求来源时放行。
            _ => true,
        };
        if !ok {
            return axum::response::Response::builder()
                .status(axum::http::StatusCode::FORBIDDEN)
                .body(axum::body::Body::empty())
                .expect("static forbidden response is always valid");
        }
    }
    next.run(req).await
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue, Method};

    #[test]
    fn is_write_method_recognizes_writes() {
        assert!(is_write_method(&Method::POST));
        assert!(is_write_method(&Method::PUT));
        assert!(is_write_method(&Method::PATCH));
        assert!(is_write_method(&Method::DELETE));
        assert!(!is_write_method(&Method::GET));
        assert!(!is_write_method(&Method::OPTIONS));
        assert!(!is_write_method(&Method::HEAD));
    }

    #[test]
    fn normalize_strips_path_and_query() {
        assert_eq!(
            normalize_origin("https://example.com/a/b?c=1"),
            "https://example.com"
        );
    }

    #[test]
    fn normalize_preserves_nondefault_port() {
        assert_eq!(
            normalize_origin("http://localhost:3000/x"),
            "http://localhost:3000"
        );
    }

    #[test]
    fn normalize_drops_default_ports() {
        assert_eq!(
            normalize_origin("https://example.com:443/path"),
            "https://example.com"
        );
        assert_eq!(
            normalize_origin("http://example.com:80/path"),
            "http://example.com"
        );
    }

    #[test]
    fn normalize_keeps_explicit_nondefault_https_port() {
        assert_eq!(
            normalize_origin("https://example.com:8443"),
            "https://example.com:8443"
        );
    }

    #[test]
    fn normalize_plain_origin_no_path() {
        assert_eq!(
            normalize_origin("https://example.com"),
            "https://example.com"
        );
    }

    #[test]
    fn extract_origin_prefers_origin_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::ORIGIN,
            HeaderValue::from_static("https://example.com"),
        );
        assert_eq!(
            extract_origin(&headers),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn extract_origin_falls_back_to_referer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::REFERER,
            HeaderValue::from_static("https://example.com/posts/1"),
        );
        // Referer 的路径被剥离，只保留 scheme://host。
        assert_eq!(
            extract_origin(&headers),
            Some("https://example.com".to_string())
        );
    }

    #[test]
    fn extract_origin_returns_none_when_both_absent() {
        let headers = HeaderMap::new();
        assert_eq!(extract_origin(&headers), None);
    }

    // ── APP_BASE_URL 启动告警 ──────────────────────────────────────
    // 这些测试读/写 APP_BASE_URL 全局环境变量，用 serial 串行隔离，
    // 与 rate_limit.rs 的 env 测试同模式（保存 → 设值 → 恢复）。

    #[test]
    #[serial_test::serial]
    fn app_base_url_is_set_false_when_unset() {
        let original = std::env::var("APP_BASE_URL").ok();
        std::env::remove_var("APP_BASE_URL");
        assert!(!app_base_url_is_set());
        restore_env("APP_BASE_URL", original);
    }

    #[test]
    #[serial_test::serial]
    fn app_base_url_is_set_false_when_empty() {
        let original = std::env::var("APP_BASE_URL").ok();
        std::env::set_var("APP_BASE_URL", "");
        assert!(!app_base_url_is_set());
        restore_env("APP_BASE_URL", original);
    }

    #[test]
    #[serial_test::serial]
    fn app_base_url_is_set_false_when_whitespace_only() {
        let original = std::env::var("APP_BASE_URL").ok();
        std::env::set_var("APP_BASE_URL", "   \t  ");
        assert!(!app_base_url_is_set());
        restore_env("APP_BASE_URL", original);
    }

    #[test]
    #[serial_test::serial]
    fn app_base_url_is_set_true_when_set() {
        let original = std::env::var("APP_BASE_URL").ok();
        std::env::set_var("APP_BASE_URL", "https://example.com");
        assert!(app_base_url_is_set());
        restore_env("APP_BASE_URL", original);
    }

    #[test]
    #[serial_test::serial]
    fn app_base_url_is_set_trims_surrounding_whitespace() {
        let original = std::env::var("APP_BASE_URL").ok();
        std::env::set_var("APP_BASE_URL", "  https://example.com  ");
        assert!(app_base_url_is_set());
        restore_env("APP_BASE_URL", original);
    }

    /// 恢复环境变量到测试前的状态，避免污染其他测试。
    fn restore_env(key: &str, original: Option<String>) {
        match original {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}
