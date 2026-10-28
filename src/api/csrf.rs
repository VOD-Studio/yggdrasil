//! CSRF 防护：对写请求校验 Origin（回退 Referer）必须等于本站。
//!
//! SameSite=Lax 只在顶级 GET 导航时自动带 cookie，对跨站 POST 不带 cookie，
//! 挡住了大部分经典 CSRF。但存在两个 Lax 覆盖不到的盲区：
//! 1. 登录 CSRF（攻击者诱导受害者登录攻击者账号，Lax 不阻止「设置」cookie）；
//! 2. 顶级 GET 导航时 Lax 会带 cookie，GET 下载类接口（数据导出）可被跨站触发。
//! 因此对所有写请求叠加 Origin 校验作为纵深防御；GET 下载类接口另挂
//! [`crate::api::csrf::same_origin_read_middleware`]。
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
/// 未配置时 [`trusted_source`] 回退为“来源主机等于请求 `Host` 头”，
/// 反向代理后若 `Host` 头可被客户端影响，该回退路径可被 CSRF 绕过。
/// 生产环境应显式设置该变量为站点完整 origin（如 `https://your-domain.example`）。
///
/// 本地开发同样会触发（默认不设 `APP_BASE_URL`），代价仅是启动时一条 WARN。
/// 与 `image.rs` 的启动告警同范式：一次性 WARN，不污染每请求路径。
#[cfg(feature = "server")]
pub fn warn_if_app_base_url_unset() {
    if app_base_url_is_set() {
        return;
    }
    tracing::warn!(
        "APP_BASE_URL 未设置。CSRF 校验将回退为比对请求来源与 Host 头的主机，\
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
    format!("{}://{}", scheme, normalize_authority(rest))
}

/// 从 `host[:port][/path]` 取出 `host[:port]`，省略默认端口（80/443）。
#[cfg(feature = "server")]
fn normalize_authority(input: &str) -> &str {
    // input 形如 host[:port]/path?query，去掉首个 '/' 及之后内容。
    let authority = match input.split_once('/') {
        Some((auth, _)) => auth,
        None => input,
    };
    match authority.rsplit_once(':') {
        Some((host, port)) if port == "80" || port == "443" => host,
        _ => authority,
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

/// 本站来源的判定依据。
#[cfg(feature = "server")]
enum TrustedSource {
    /// 配置了 APP_BASE_URL：来源必须与它的 origin 完全相同（含协议）。
    Origin(String),
    /// 未配置：来源的 `host[:port]` 必须等于请求 `Host` 头，协议不限。
    /// 本地 http 开发无需配置即可使用，localhost 与 127.0.0.1 各自成立；
    /// 跨站请求的来源主机不同，照样被拒。
    Host(String),
}

/// 计算本站来源：优先「站点配置 → 安全」面板的 APP_BASE_URL（即时生效），
/// 否则取请求 Host 头。
///
/// 返回 None 表示无法确定本站来源；写请求在这种情况下会被拒绝，
/// 避免在来源头缺失时把 cookie-auth 请求当作可信请求。
#[cfg(feature = "server")]
async fn trusted_source(headers: &HeaderMap) -> Option<TrustedSource> {
    let base = crate::api::settings::runtime_security_settings()
        .await
        .app_base_url;
    if !base.is_empty() {
        return Some(TrustedSource::Origin(normalize_origin(&base)));
    }
    let host = headers.get(axum::http::header::HOST)?.to_str().ok()?;
    Some(TrustedSource::Host(normalize_authority(host).to_string()))
}

/// 判断写请求的来源是否可信。
///
/// cookie-auth 写请求必须同时拥有可确定的本站来源与来源头；
/// 任一缺失都拒绝，避免 `Origin`/`Referer` 被剥离时形成 CSRF fail-open。
#[cfg(feature = "server")]
fn origin_matches(trusted: Option<&TrustedSource>, incoming: Option<&str>) -> bool {
    match (trusted, incoming) {
        (Some(TrustedSource::Origin(origin)), Some(incoming)) => origin == incoming,
        (Some(TrustedSource::Host(host)), Some(incoming)) => incoming
            .strip_prefix("http://")
            .or_else(|| incoming.strip_prefix("https://"))
            .is_some_and(|authority| !host.is_empty() && authority.eq_ignore_ascii_case(host)),
        _ => false,
    }
}

/// 从 Fetch Metadata 判断请求是否由本站页面发起。
///
/// 浏览器的顶级 GET 导航（`window.open`、`<a href>`）不带 `Origin`，只能靠
/// `Sec-Fetch-Site`：仅 `same-origin` 可信；`same-site` 包含兄弟子域，`cross-site`
/// 是跨站，`none` 是地址栏/书签直开，本站 UI 不会产生，一律拒绝。
/// 返回 None 表示请求不带该头（旧浏览器或非浏览器客户端）。
#[cfg(feature = "server")]
fn fetch_site_is_same_origin(headers: &HeaderMap) -> Option<bool> {
    headers
        .get("sec-fetch-site")
        .map(|v| v.as_bytes() == b"same-origin")
}

#[cfg(feature = "server")]
fn forbidden() -> axum::response::Response {
    axum::response::Response::builder()
        .status(axum::http::StatusCode::FORBIDDEN)
        .body(axum::body::Body::empty())
        .expect("static forbidden response is always valid")
}

/// CSRF 校验中间件。
///
/// 对写方法校验请求来源等于本站；不匹配返回 403。GET/OPTIONS 等放行。
/// 拿不到本站 origin 或请求来源时拒绝写请求。
#[cfg(feature = "server")]
pub async fn csrf_middleware(
    req: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if is_write_method(req.method()) {
        let headers = req.headers().clone();
        let trusted = trusted_source(&headers).await;
        let incoming = extract_origin(&headers);
        let ok = origin_matches(trusted.as_ref(), incoming.as_deref());
        if !ok {
            return forbidden();
        }
    }
    next.run(req).await
}

/// GET 下载类接口的同源校验中间件：拒绝跨站触发的顶级导航。
///
/// 优先看 `Sec-Fetch-Site`；不带该头时回退到与写请求相同的 Origin/Referer 校验，
/// 来源缺失同样拒绝。只挂在需要的路由上，不影响普通页面 GET。
#[cfg(feature = "server")]
pub async fn same_origin_read_middleware(
    req: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let ok = match fetch_site_is_same_origin(req.headers()) {
        Some(same_origin) => same_origin,
        None => {
            let headers = req.headers().clone();
            let trusted = trusted_source(&headers).await;
            origin_matches(trusted.as_ref(), extract_origin(&headers).as_deref())
        }
    };
    if !ok {
        return forbidden();
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

    #[test]
    fn origin_matches_requires_both_origins() {
        let site = TrustedSource::Origin("https://example.com".to_string());
        assert!(origin_matches(Some(&site), Some("https://example.com")));
        assert!(!origin_matches(Some(&site), None));
        assert!(!origin_matches(None, Some("https://example.com")));
        assert!(!origin_matches(Some(&site), Some("https://evil.example")));
        // 配置了站点地址时协议也要一致。
        assert!(!origin_matches(Some(&site), Some("http://example.com")));
    }

    #[test]
    fn origin_matches_host_fallback_ignores_scheme_only() {
        let host = TrustedSource::Host(normalize_authority("localhost:8080").to_string());
        assert!(origin_matches(Some(&host), Some("http://localhost:8080")));
        assert!(origin_matches(Some(&host), Some("https://localhost:8080")));
        assert!(!origin_matches(Some(&host), Some("http://localhost:3000")));
        assert!(!origin_matches(Some(&host), Some("http://127.0.0.1:8080")));
        assert!(!origin_matches(
            Some(&host),
            Some("http://evil.localhost:8080")
        ));
        // 沙箱 iframe、file:// 等来源为 "null"。
        assert!(!origin_matches(Some(&host), Some("null")));
        assert!(!origin_matches(Some(&host), None));

        // 默认端口两侧都省略：Host 头 `example.com:443` 与 Origin `https://example.com` 一致。
        let host = TrustedSource::Host(normalize_authority("example.com:443").to_string());
        let incoming = normalize_origin("https://example.com:443");
        assert!(origin_matches(Some(&host), Some(&incoming)));
        assert!(origin_matches(Some(&host), Some("https://example.com")));
    }

    #[test]
    fn fetch_site_trusts_only_same_origin() {
        let with = |value: &'static str| {
            let mut headers = HeaderMap::new();
            headers.insert("sec-fetch-site", HeaderValue::from_static(value));
            fetch_site_is_same_origin(&headers)
        };
        assert_eq!(with("same-origin"), Some(true));
        // same-site 含兄弟子域；none 为地址栏直开，本站 UI 不会产生。
        assert_eq!(with("same-site"), Some(false));
        assert_eq!(with("cross-site"), Some(false));
        assert_eq!(with("none"), Some(false));
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
