//! Axum 中间件与启动期纯函数。
//!
//! 从 `main.rs` 抽出的、可独立测试的服务端 HTTP 中间件（cache-control、admin
//! 守卫）与压缩层构造逻辑。整体 server-only——WASM 构建不会编译本模块。
//!
//! 这些函数此前散落在 `main.rs`，既无法单独测试也使入口职责过载。迁移后
//! `startup.rs` 的路由组装以全路径 `crate::middleware::xxx` 引用，语义不变。

#![cfg(feature = "server")]

/// 压缩算法配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompressionAlgorithms {
    gzip: bool,
    brotli: bool,
    deflate: bool,
    zstd: bool,
}

impl CompressionAlgorithms {
    fn all_enabled() -> Self {
        Self {
            gzip: true,
            brotli: true,
            deflate: true,
            zstd: true,
        }
    }

    fn is_empty(&self) -> bool {
        !self.gzip && !self.brotli && !self.deflate && !self.zstd
    }
}

/// 解析 COMPRESSION_ALGORITHMS 环境变量值。
/// ""、"none"、"off" 返回 None；"all" 或未识别到任何算法时启用全部。
pub(crate) fn parse_compression_algorithms(env: &str) -> Option<CompressionAlgorithms> {
    let env = env.trim();
    if env.is_empty() || env.eq_ignore_ascii_case("none") || env.eq_ignore_ascii_case("off") {
        return None;
    }

    let mut all = false;
    let mut gzip = false;
    let mut brotli = false;
    let mut deflate = false;
    let mut zstd = false;

    for part in env.split(',') {
        match part.trim().to_lowercase().as_str() {
            "all" => all = true,
            "gzip" => gzip = true,
            "brotli" | "br" => brotli = true,
            "deflate" => deflate = true,
            "zstd" => zstd = true,
            other => tracing::warn!(
                "Unknown compression algorithm in COMPRESSION_ALGORITHMS: '{}'",
                other
            ),
        }
    }

    if all {
        return Some(CompressionAlgorithms::all_enabled());
    }

    let algorithms = CompressionAlgorithms {
        gzip,
        brotli,
        deflate,
        zstd,
    };
    if algorithms.is_empty() {
        return None;
    }

    Some(algorithms)
}

/// 根据 COMPRESSION_ALGORITHMS 环境变量构造 CompressionLayer。
/// 默认（未设置时）关闭压缩；显式设为 "all" 启用全部算法，设为 "gzip,brotli,..."
/// 按需选择；设为 ""、"none" 或 "off" 等价于不启用。
///
/// CompressionLayer 使用 tower-http 的 `DefaultPredicate`，开箱即用即：
/// - 跳过 `image/*` content-type（WebP/PNG/JPEG/GIF 等已是压缩格式，再压浪费 CPU，
///   唯一例外是 `image/svg+xml`，作为 XML 文本可被压缩）；
/// - 跳过 gRPC 与 `text/event-stream`（SSE）；
/// - 跳过小于 32 字节的响应。
///
/// 因此无需在此处对图片响应做额外的 content-type 过滤。另：图片实际挂在
/// `static_routes`（无中间件），根本不经此层，详见 startup.rs 路由 merge 处。
///
/// 已带 `Content-Encoding` 的响应（public 下预压缩的 `.br`）原样透传。
/// 生产推荐 "br, gzip"：同等 q 值时 tower-http 按 zstd > br > gzip 选择，
/// Chrome 会拿到 zstd，而 zstd-6 仍比 brotli-6 大 6–9%。
pub(crate) fn compression_layer_from_env() -> Option<tower_http::compression::CompressionLayer> {
    let env = std::env::var("COMPRESSION_ALGORITHMS").unwrap_or_else(|_| "off".to_string());
    parse_compression_algorithms(&env).map(compression_layer)
}

/// 动态压缩统一质量。tower-http 默认 brotli 4，只比 gzip-6 小 2–4%；
/// 取 7 时 brotli 比 6 再小 1.9–2.9%，每次只多几毫秒；q9 起 feed 压缩耗时成倍
/// 增加，q11 只适合构建期预压缩。
const COMPRESSION_QUALITY: i32 = 7;

fn compression_layer(
    algorithms: CompressionAlgorithms,
) -> tower_http::compression::CompressionLayer {
    use tower_http::compression::{CompressionLayer, CompressionLevel};

    CompressionLayer::new()
        .gzip(algorithms.gzip)
        .br(algorithms.brotli)
        .deflate(algorithms.deflate)
        .zstd(algorithms.zstd)
        .quality(CompressionLevel::Precise(COMPRESSION_QUALITY))
}

/// 前端入口文件前缀（`[package.metadata.leptos] output-name = "yggdrasil"`）。
const PKG_ENTRY_PREFIX: &str = "/pkg/yggdrasil.";

/// `/pkg/` 下的文件名是否形如 `<name>.<22 位 [A-Za-z0-9_-]>.<扩展名>`（cargo-leptos 的内容哈希命名）。
fn is_hashed_pkg_file(path: &str) -> bool {
    let Some(file_name) = path.rsplit('/').next() else {
        return false;
    };
    let parts: Vec<&str> = file_name.split('.').collect();
    if parts.len() < 3 || parts[0].is_empty() || parts[parts.len() - 1].is_empty() {
        return false;
    }
    let hash = parts[parts.len() - 2];
    hash.len() == 22
        && hash
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 根据请求路径、查询串和方法决定公开页面的 Cache-Control 头。
/// 返回 None 表示不添加缓存头（保留现有行为或避免覆盖）。
///
/// `entry_content_addressed`：hash.txt 是否带 `entry: content-addressed`（见
/// `scripts/build-release.ts` 的 contentAddressEntries），决定入口文件能否 immutable。
pub(crate) fn cache_control_for_path(
    path: &str,
    query: Option<&str>,
    method: &axum::http::Method,
    entry_content_addressed: bool,
) -> Option<axum::http::HeaderValue> {
    use axum::http::{HeaderValue, Method};

    // 只对 GET/HEAD 请求添加缓存头
    if *method != Method::GET && *method != Method::HEAD {
        return None;
    }

    // 可撤回的笔记与受保护图片不缓存，包含未公开时的 404 响应。
    if path == "/notes" || path.starts_with("/notes/") {
        return Some(HeaderValue::from_static("no-store"));
    }
    if path.starts_with("/admin/notes")
        || path == "/admin/notebooks"
        || path.starts_with("/note-media/")
    {
        return Some(HeaderValue::from_static("private, no-store"));
    }
    if path.starts_with("/api") {
        return None;
    }

    // 管理后台和认证页面：不缓存
    if crate::routes::is_admin_path(path) || path == "/login" || path == "/register" {
        return None;
    }

    // /pkg/ 下 cargo-leptos 产物：文件名带内容哈希（`<name>.<22 位哈希>.<扩展名>`）的永不变化，
    // 可长期缓存；其余（固定文件名）随部署变化。
    //
    // 入口 `yggdrasil.<哈希>.{js,wasm}` 只有发布装配按最终内容重命名后才 immutable：cargo-leptos
    // 先按内容算哈希改名，之后才把分包加载器 `__wasm_split.<哈希>.js` 的文件名写进入口。未经装配
    // 的产物（dev、回归构建）里，只改懒加载代码会换掉加载器文件名而入口同名；入口若 immutable，
    // 回访浏览器会一直用旧入口去请求已删除的旧加载器，整页无法水合，所以仍每次校验。
    if path.starts_with("/pkg/") {
        let immutable = is_hashed_pkg_file(path)
            && (entry_content_addressed || !path.starts_with(PKG_ENTRY_PREFIX));
        return Some(HeaderValue::from_static(if immutable {
            "public, max-age=31536000, immutable"
        } else {
            "public, no-cache"
        }));
    }

    // 固定路径的脚本、样式和 WASM 会随部署变化。`?v=` 等于当前内容版本号的地址
    // （`static_assets::url` 生成）内容不会再变，可长期缓存；其余每次使用前须校验。
    if path.ends_with(".wasm") || path.ends_with(".js") || path.ends_with(".css") {
        return Some(HeaderValue::from_static(
            if crate::static_assets::is_current(path, query) {
                "public, max-age=31536000, immutable"
            } else {
                "public, no-cache"
            },
        ));
    }

    // 公开页面：5 分钟新鲜期，过期后 1 小时内可提供过期内容并后台重新验证
    Some(HeaderValue::from_static(
        "public, max-age=300, stale-while-revalidate=3600",
    ))
}

/// Axum 中间件：为公开页面和静态资源附加 Cache-Control 头。
pub(crate) async fn add_cache_control(
    entry_content_addressed: bool,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::header;

    let path = req.uri().path().to_string();
    let cache_value = cache_control_for_path(
        &path,
        req.uri().query(),
        req.method(),
        entry_content_addressed,
    );

    let mut response = next.run(req).await;

    if let Some(value) = cache_value {
        // 仅当响应尚未设置 Cache-Control 时才添加，避免覆盖已有策略
        response
            .headers_mut()
            .entry(header::CACHE_CONTROL)
            .or_insert(value);
    }

    response
}

/// 请求处理 panic 时返回给客户端的固定文案；panic 内容只写日志。
const PANIC_RESPONSE_BODY: &str = "服务器内部错误";

/// 把 handler 及其内层中间件在响应头发出前的 panic 转成 500。
pub(crate) fn catch_panic_layer() -> tower_http::catch_panic::CatchPanicLayer<
    fn(Box<dyn std::any::Any + Send + 'static>) -> axum::response::Response,
> {
    tower_http::catch_panic::CatchPanicLayer::custom(panic_response)
}

fn panic_response(err: Box<dyn std::any::Any + Send + 'static>) -> axum::response::Response {
    use axum::response::IntoResponse;

    let message = err
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("未知 panic 负载");
    tracing::error!(panic = message, "请求处理 panic，返回 500");
    // 显式 no-store：panic 越过 add_cache_control 展开，响应不会带任何 Cache-Control，
    // CDN 可能按默认规则缓存这个 500。
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        [
            (axum::http::header::CACHE_CONTROL, "no-store"),
            (
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            ),
        ],
        PANIC_RESPONSE_BODY,
    )
        .into_response()
}

/// Axum 中间件：`/admin*` 的 SSR 层认证守卫。
///
/// 未登录访问后台时，服务端**直接 302 跳转 `/login`**，根本不进入 Dioxus
/// SSR 渲染器。此前后台鉴权完全在客户端 WASM 完成（SSR 渲染骨架屏 → WASM
/// 下载/编译 → hydrate → 异步 `get_current_user()` → 客户端 `navigator.push`），
/// 整条链串行，未登录用户首屏要"空白好久"才跳登录。
///
/// - 只匹配 `/admin*`，其它路径（`/login`、公开页、`/api/*`）直接放行。
/// - 复用 `get_user_by_token`：命中内存缓存 + 校验 `session_generation`
///   （封禁/降级后旧 session 立即失效），与客户端鉴权同一套语义。
/// - 无 token、token 无效/过期/世代号不匹配、非管理员：302 跳转 `/login`。
/// - 仅 DB 错误时 **fail-open**（放行进入 SSR）：避免数据库抖动把已登录的
///   管理员也踢到登录页；客户端 `AdminLayout` 仍有兜底校验。
/// - `/admin` 与 `/login` 本就不进 `cache_control_for_path` 缓存，302 不会被缓存。
pub(crate) async fn admin_guard(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use axum::response::Response;

    let path = req.uri().path().to_string();
    if !crate::routes::is_admin_path(&path) {
        return next.run(req).await;
    }

    // 从 Cookie 头读 session token（与 export.rs / upload.rs 同款手法）。
    let cookie = req
        .headers()
        .get("cookie")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let token = crate::auth::session::parse_session_token(cookie);

    let session = match token {
        Some(t) => Some(
            crate::api::auth::get_user_by_token(t)
                .await
                .map(|user| user.map(|u| u.role)),
        ),
        None => None,
    };

    if admin_guard_allows(session) {
        next.run(req).await
    } else {
        Response::builder()
            .status(StatusCode::FOUND)
            .header(header::LOCATION, "/login")
            .body(Body::empty())
            .expect("静态 302 重定向响应（合法 status + 固定 header + 空 body）必然构造成功")
    }
}

/// [`admin_guard`] 的放行判定。
///
/// `session` 为 `None` 表示请求没有 session cookie；`Some(Ok(None))` 表示 token
/// 无效、过期或世代号不匹配。只有查询出错（`Some(Err(_))`）才 fail-open。
fn admin_guard_allows<E>(
    session: Option<Result<Option<crate::models::user::UserRole>, E>>,
) -> bool {
    use crate::models::user::UserRole;
    match session {
        Some(Ok(Some(role))) => role == UserRole::Admin,
        // DB 抖动：fail-open，交给客户端兜底。
        Some(Err(_)) => true,
        // 无 token / token 无效：明确未登录，拦截。
        None | Some(Ok(None)) => false,
    }
}

/// Axum 中间件：把当前 SSR 全局世代号注入请求扩展，并对 GET 请求的响应附加
/// `X-SSR-Generation` 头。世代号同时保护应用 HTML 缓存，阻止失效前的在途响应回填。
pub(crate) async fn ssr_generation_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let generation = crate::ssr_cache::current_global_generation();
    let is_get = req.method() == axum::http::Method::GET;
    let (mut parts, body) = req.into_parts();
    parts
        .extensions
        .insert(crate::ssr_cache::SsrGeneration(generation));
    let mut response = next.run(axum::http::Request::from_parts(parts, body)).await;
    if is_get {
        response.headers_mut().insert(
            axum::http::header::HeaderName::from_static("x-ssr-generation"),
            axum::http::HeaderValue::from_str(&generation.to_string())
                .unwrap_or_else(|_| axum::http::HeaderValue::from_static("0")),
        );
    }
    response
}

/// Axum 中间件：把分享元信息所需的站点信息（地址、简介、站点图地址）放进 GET 请求
/// 扩展，页面服务端渲染时由 `components::share_meta` 同步读取（类型见 [`crate::models::share::ShareContext`]）。位于 HTML 缓存内层，
/// 缓存命中时不执行。
pub(crate) async fn share_context_middleware(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if req.method() == axum::http::Method::GET {
        let ctx = crate::api::og::share_context().await;
        req.extensions_mut().insert(ctx);
    }
    next.run(req).await
}

/// Axum 中间件：为所有响应附加 Server / X-Yggdrasil-Version / X-Yggdrasil-Git / X-Yggdrasil-Hash 头。
/// 数据源与启动日志 `log_build_info()` 同源（`crate::build_info::BUILD_INFO`）。
/// 受 `EXPOSE_VERSION_HEADERS` 控制——该开关在 `startup.rs` 决定是否挂载本层。
///
/// 暴露 hash 头的原因：tag 精确 checkout 时 `git describe` 只返回纯 tag 名（如 `v0.10.1`），
/// 不含 commit hash；单独暴露 `git_hash`（完整 40 位）以精确定位线上二进制对应的提交。
pub(crate) async fn version_headers_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(req).await;
    let h = response.headers_mut();
    h.insert(
        axum::http::header::SERVER,
        axum::http::HeaderValue::from_str(&format!(
            "yggdrasil/{}",
            crate::build_info::BUILD_INFO.version
        ))
        .unwrap_or_else(|_| axum::http::HeaderValue::from_static("yggdrasil")),
    );
    h.insert(
        axum::http::header::HeaderName::from_static("x-yggdrasil-version"),
        axum::http::HeaderValue::from_static(crate::build_info::BUILD_INFO.version),
    );
    h.insert(
        axum::http::header::HeaderName::from_static("x-yggdrasil-git"),
        axum::http::HeaderValue::from_str(crate::build_info::BUILD_INFO.git_describe)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("unknown")),
    );
    h.insert(
        axum::http::header::HeaderName::from_static("x-yggdrasil-hash"),
        axum::http::HeaderValue::from_str(crate::build_info::BUILD_INFO.git_hash)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("unknown")),
    );
    response
}

/// 解析旧发布目录中可能命中的带 hash 资源。只放行 /pkg/（cargo-leptos 内容
/// 寻址产物目录）；URI path 保持百分号编码原样映射，任何非常规组件（`..`、
/// 根/父目录引用）都不是 Normal 分量，直接拒绝；文件不存在返回 None。
fn previous_asset_file(previous_root: &std::path::Path, path: &str) -> Option<std::path::PathBuf> {
    use std::path::Component;
    use std::path::Path;

    let rel = path.strip_prefix('/')?;
    let top = rel.split('/').next()?;
    if top != "pkg" {
        return None;
    }
    let rel_path = Path::new(rel);
    if rel_path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    let file = previous_root.join(rel_path);
    file.is_file().then_some(file)
}

/// Axum 中间件（由 startup.rs 按需挂载）：旧发布资源保留窗口（plan §6.7）。
///
/// 滚动更新后，已打开标签页与反代/浏览器缓存的旧 HTML 仍引用上一发布的带
/// hash 资源（含按路由懒加载的 WASM 分包）；新发布未收录这些文件时（404），
/// 回退到 `YGG_PREVIOUS_PUBLIC_DIR` 同路径文件。语义：
/// - 只拦截 /pkg/ 的 GET 404，未命中保留原 404，不遮蔽当前发布
///   的真实缺失；
/// - 复用 ServeFile 的内容类型推断与 `.br` 旁车协商（release public 自带旁车，
///   与静态资源服务的预压缩行为一致）；
/// - 命中即 immutable（内容寻址）并带 `x-previous-release` 标记，外层
///   add_cache_control 的 or_insert 不会覆盖。
pub(crate) async fn previous_release_assets(
    previous_root: std::path::PathBuf,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use tower::ServiceExt;

    let is_get = req.method() == axum::http::Method::GET;
    let fallback = if is_get {
        previous_asset_file(&previous_root, req.uri().path())
    } else {
        None
    };
    let (parts, body) = req.into_parts();
    let response = next
        .run(axum::http::Request::from_parts(parts.clone(), body))
        .await;
    let Some(file) = fallback else {
        return response;
    };
    if response.status() != axum::http::StatusCode::NOT_FOUND {
        return response;
    }
    let request = {
        let mut builder = axum::http::Request::builder()
            .method(axum::http::Method::GET)
            .uri(parts.uri.clone());
        for (name, value) in parts.headers.iter() {
            builder = builder.header(name, value);
        }
        match builder.body(axum::body::Body::empty()) {
            Ok(request) => request,
            Err(_) => return response,
        }
    };
    let served = tower_http::services::ServeFile::new(&file)
        .precompressed_br()
        .oneshot(request)
        .await;
    match served {
        Ok(asset) if asset.status() == axum::http::StatusCode::OK => {
            let mut asset = asset.map(axum::body::Body::new);
            let headers = asset.headers_mut();
            headers.insert(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
            headers.insert(
                axum::http::HeaderName::from_static("x-previous-release"),
                axum::http::HeaderValue::from_static("1"),
            );
            tracing::debug!(file = %file.display(), "旧发布资源经保留窗口返回");
            asset
        }
        // 上一发布同样缺失或读取失败：保留当前发布的 404。
        _ => response,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cache_control_for_path, compression_layer, parse_compression_algorithms,
        previous_asset_file, CompressionAlgorithms,
    };
    use axum::http::{header, Method, Request};
    use tower::ServiceExt;

    #[test]
    fn admin_guard_redirects_invalid_session_and_fails_open_only_on_db_error() {
        use super::admin_guard_allows;
        use crate::models::user::UserRole;

        // 无 cookie、token 无效/过期/世代号不匹配、非管理员：拦截。
        assert!(!admin_guard_allows::<()>(None));
        assert!(!admin_guard_allows::<()>(Some(Ok(None))));
        assert!(!admin_guard_allows::<()>(Some(Ok(Some(UserRole::Blocked)))));
        // 有效管理员会话：放行。
        assert!(admin_guard_allows::<()>(Some(Ok(Some(UserRole::Admin)))));
        // 仅 DB 错误 fail-open。
        let db_error: Result<Option<UserRole>, &str> = Err("db down");
        assert!(admin_guard_allows(Some(db_error)));
    }

    #[test]
    fn previous_asset_window_only_serves_hashed_dirs_inside_root() {
        let root = std::env::temp_dir().join("ygg-prev-asset-test");
        let pkg = root.join("pkg");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(
            pkg.join("legacy_chunk.AbCdEfGhIjKlMnOpQrStUv.wasm"),
            b"wasm",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("uploads")).unwrap();
        std::fs::write(root.join("uploads").join("secret.txt"), b"secret").unwrap();

        let get = |path: &str| previous_asset_file(&root, path);
        assert_eq!(
            get("/pkg/legacy_chunk.AbCdEfGhIjKlMnOpQrStUv.wasm"),
            Some(pkg.join("legacy_chunk.AbCdEfGhIjKlMnOpQrStUv.wasm"))
        );
        // 缺失文件与目录外前缀一律 None；非 Normal 分量（含 %2e%2e 编码形态的
        // 字面量不匹配文件名，../ 形态则被分量检查拒绝）。
        assert_eq!(get("/pkg/missing.js"), None);
        assert_eq!(get("/pkg/"), None);
        assert_eq!(
            get("/assets/legacy_chunk.AbCdEfGhIjKlMnOpQrStUv.wasm"),
            None
        );
        assert_eq!(get("/uploads/secret.txt"), None);
        assert_eq!(get("/api/posts"), None);
        assert_eq!(get("/pkg/../uploads/secret.txt"), None);
        assert_eq!(get("/pkg/%2e%2e/uploads/secret.txt"), None);
        std::fs::remove_dir_all(&root).ok();
    }

    fn cache_value(path: &str, method: Method) -> Option<String> {
        cache_control_for_path(path, None, &method, false).map(|v| v.to_str().unwrap().to_string())
    }

    #[test]
    fn public_page_is_cached() {
        assert_eq!(
            cache_value("/", Method::GET),
            Some("public, max-age=300, stale-while-revalidate=3600".to_string())
        );
        assert_eq!(
            cache_value("/post/hello-world", Method::GET),
            Some("public, max-age=300, stale-while-revalidate=3600".to_string())
        );
        assert_eq!(
            cache_value("/tags/rust", Method::GET),
            Some("public, max-age=300, stale-while-revalidate=3600".to_string())
        );
    }

    #[test]
    fn revocable_notes_never_use_browser_or_cdn_cache() {
        for path in ["/notes", "/notes/example", "/notes/book/1"] {
            for method in [Method::GET, Method::HEAD] {
                assert_eq!(cache_value(path, method).as_deref(), Some("no-store"));
            }
        }
        assert_eq!(
            cache_value("/admin/notes/edit/1", Method::GET).as_deref(),
            Some("private, no-store")
        );
        assert_eq!(
            cache_value("/note-media/example", Method::GET).as_deref(),
            Some("private, no-store")
        );
    }

    #[test]
    fn unversioned_assets_require_revalidation() {
        for path in [
            "/style.css",
            "/highlight.css",
            "/tiptap/editor.css",
            "/xterm/terminal.css",
            "/tiptap/editor.js",
            "/codemirror/editor.js",
            "/yggdrasil-core/yggdrasil-core.js",
            "/yggdrasil-core/yggdrasil-enhance.js",
            "/mermaid/mermaid.js",
            "/pkg/yggdrasil.wasm",
            "/pkg/yggdrasil.js",
        ] {
            for method in [Method::GET, Method::HEAD] {
                assert_eq!(
                    cache_value(path, method).as_deref(),
                    Some("public, no-cache"),
                    "{path}"
                );
            }
        }
    }

    #[test]
    #[serial_test::serial]
    fn only_current_asset_version_is_immutable() {
        let root = std::env::temp_dir().join(format!("ygg-cache-control-{}", std::process::id()));
        std::fs::create_dir_all(root.join("yggdrasil-core")).unwrap();
        std::fs::write(root.join("yggdrasil-core/yggdrasil-core.js"), "1").unwrap();
        let previous = std::env::var_os("LEPTOS_SITE_ROOT");
        std::env::set_var("LEPTOS_SITE_ROOT", &root);

        let path = crate::static_assets::CORE_JS;
        let url = crate::static_assets::url(path);
        let (_, current) = url.split_once('?').unwrap();
        let value = |query: Option<&str>| {
            cache_control_for_path(path, query, &Method::GET, false)
                .map(|v| v.to_str().unwrap().to_owned())
        };
        assert_eq!(
            value(Some(current)).as_deref(),
            Some("public, max-age=31536000, immutable")
        );
        // 滚动发布期间旧页面带着旧版本号来请求，拿到的是新内容，不能以旧地址长期缓存。
        assert_eq!(
            value(Some("v=0123456789abcdef")).as_deref(),
            Some("public, no-cache")
        );
        assert_eq!(value(None).as_deref(), Some("public, no-cache"));

        match previous {
            Some(value) => std::env::set_var("LEPTOS_SITE_ROOT", value),
            None => std::env::remove_var("LEPTOS_SITE_ROOT"),
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn hashed_pkg_assets_are_immutable() {
        for path in [
            "/pkg/__wasm_split.AbCdEfGhIjKlMnOpQrStUv.js",
            "/pkg/chunk_12.AbCdEfGhIjKlMnOpQrStUv.wasm",
            "/pkg/split___home_route_view_42.A-_dEfGhIjKlMnOpQrStU0.wasm",
        ] {
            for method in [Method::GET, Method::HEAD] {
                assert_eq!(
                    cache_value(path, method).as_deref(),
                    Some("public, max-age=31536000, immutable"),
                    "{path}"
                );
            }
        }
        // 未经发布装配的入口文件名不随内嵌的加载器文件名变化，必须校验；哈希长度不对或缺少
        // 哈希段的 /pkg/ 文件同样需要校验。
        for path in [
            "/pkg/yggdrasil.AbCdEfGhIjKlMnOpQrStUv.wasm",
            "/pkg/yggdrasil.AbCdEfGhIjKlMnOpQrStUv.js",
            "/pkg/chunk_12.short.wasm",
            "/pkg/chunk_12.AbCdEfGhIjKlMnOpQrStUvX.wasm",
            "/pkg/chunk_12.AbCdEfGhIjKlMnOpQrSt!v.wasm",
            "/pkg/__wasm_split_manifest.json",
        ] {
            assert_eq!(
                cache_value(path, Method::GET).as_deref(),
                Some("public, no-cache"),
                "{path}"
            );
        }
    }

    #[test]
    fn content_addressed_entries_are_immutable() {
        let value = |path: &str| {
            cache_control_for_path(path, None, &Method::GET, true)
                .map(|v| v.to_str().unwrap().to_owned())
        };
        for path in [
            "/pkg/yggdrasil.AbCdEfGhIjKlMnOpQrStUv.wasm",
            "/pkg/yggdrasil.AbCdEfGhIjKlMnOpQrStUv.js",
            "/pkg/chunk_12.AbCdEfGhIjKlMnOpQrStUv.wasm",
        ] {
            assert_eq!(
                value(path).as_deref(),
                Some("public, max-age=31536000, immutable"),
                "{path}"
            );
        }
        // 内容寻址只改变入口的判定，未带哈希的文件仍需校验。
        for path in ["/pkg/yggdrasil.wasm", "/pkg/__wasm_split_manifest.json"] {
            assert_eq!(value(path).as_deref(), Some("public, no-cache"), "{path}");
        }
    }

    #[test]
    fn api_and_admin_and_auth_are_not_cached() {
        assert_eq!(cache_value("/api/posts", Method::GET), None);
        assert_eq!(cache_value("/admin", Method::GET), None);
        assert_eq!(cache_value("/admin/posts", Method::GET), None);
        assert_eq!(cache_value("/login", Method::GET), None);
        assert_eq!(cache_value("/register", Method::GET), None);
        // 后台样式是公开静态文件，与后台页面同前缀但走版本化缓存。
        assert_eq!(
            cache_value("/admin.css", Method::GET).as_deref(),
            Some("public, no-cache")
        );
    }

    #[test]
    fn non_get_requests_are_not_cached() {
        assert_eq!(cache_value("/", Method::POST), None);
        assert_eq!(cache_value("/post/hello-world", Method::POST), None);
        assert_eq!(cache_value("/style.css", Method::POST), None);
    }

    #[test]
    fn head_requests_are_cached_like_get() {
        assert_eq!(
            cache_value("/", Method::HEAD),
            Some("public, max-age=300, stale-while-revalidate=3600".to_string())
        );
    }

    #[test]
    fn compression_all_enables_everything() {
        assert_eq!(
            parse_compression_algorithms("all"),
            Some(CompressionAlgorithms::all_enabled())
        );
    }

    #[test]
    fn compression_default_env_is_off() {
        // 模拟未设置环境变量时的默认值
        assert_eq!(parse_compression_algorithms("off"), None);
    }

    #[test]
    fn compression_empty_none_off_disable() {
        assert_eq!(parse_compression_algorithms(""), None);
        assert_eq!(parse_compression_algorithms("none"), None);
        assert_eq!(parse_compression_algorithms("NONE"), None);
        assert_eq!(parse_compression_algorithms("off"), None);
        assert_eq!(parse_compression_algorithms("OFF"), None);
    }

    #[test]
    fn compression_single_algorithm() {
        assert_eq!(
            parse_compression_algorithms("gzip"),
            Some(CompressionAlgorithms {
                gzip: true,
                brotli: false,
                deflate: false,
                zstd: false,
            })
        );
        assert_eq!(
            parse_compression_algorithms("br"),
            Some(CompressionAlgorithms {
                gzip: false,
                brotli: true,
                deflate: false,
                zstd: false,
            })
        );
    }

    #[test]
    fn compression_multiple_algorithms() {
        assert_eq!(
            parse_compression_algorithms("gzip, zstd"),
            Some(CompressionAlgorithms {
                gzip: true,
                brotli: false,
                deflate: false,
                zstd: true,
            })
        );
    }

    #[test]
    fn compression_case_insensitive_and_whitespace_tolerant() {
        assert_eq!(
            parse_compression_algorithms("GZIP, Brotli, Deflate, Zstd"),
            Some(CompressionAlgorithms::all_enabled())
        );
        assert_eq!(
            parse_compression_algorithms(" gzip , br , deflate , zstd "),
            Some(CompressionAlgorithms::all_enabled())
        );
    }

    #[test]
    fn compression_unknown_algorithms_are_ignored() {
        assert_eq!(
            parse_compression_algorithms("gzip, unknown, lz4"),
            Some(CompressionAlgorithms {
                gzip: true,
                brotli: false,
                deflate: false,
                zstd: false,
            })
        );
    }

    /// 用给定 Accept-Encoding 请求挂了压缩层的 HTML 路由，返回响应编码。
    async fn response_encoding(env: &str, accept_encoding: &str) -> Option<String> {
        let html = "<article><p>压缩回归测试段落 compression regression.</p></article>".repeat(400);
        let app = axum::Router::new()
            .route(
                "/",
                axum::routing::get(move || async move { axum::response::Html(html) }),
            )
            .layer(compression_layer(
                parse_compression_algorithms(env).unwrap(),
            ));
        let response = app
            .oneshot(
                Request::get("/")
                    .header(header::ACCEPT_ENCODING, accept_encoding)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        response
            .headers()
            .get(header::CONTENT_ENCODING)
            .map(|v| v.to_str().unwrap().to_string())
    }

    #[tokio::test]
    async fn recommended_compression_serves_brotli_to_chrome() {
        let encoding = response_encoding("br, gzip", "gzip, deflate, br, zstd").await;
        assert_eq!(encoding.as_deref(), Some("br"));
    }

    #[tokio::test]
    async fn recommended_compression_falls_back_to_gzip() {
        let encoding = response_encoding("br, gzip", "gzip").await;
        assert_eq!(encoding.as_deref(), Some("gzip"));
    }

    #[tokio::test]
    async fn all_compression_prefers_zstd_for_chrome() {
        let encoding = response_encoding("all", "gzip, deflate, br, zstd").await;
        assert_eq!(encoding.as_deref(), Some("zstd"));
    }

    #[tokio::test]
    async fn panic_in_public_page_returns_uncached_500() {
        async fn boom() -> &'static str {
            panic!("内部细节 secret")
        }

        let app = axum::Router::new()
            .route("/post/x", axum::routing::get(boom))
            .layer(axum::middleware::from_fn(|req, next| {
                super::add_cache_control(false, req, next)
            }))
            .layer(super::catch_panic_layer());

        let response = app
            .oneshot(
                Request::get("/post/x")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert_eq!(body, "服务器内部错误");
        assert!(!body.contains("secret"));
    }
}
