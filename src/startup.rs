//! 服务端启动流程与 Axum 路由组装。
//!
//! `main.rs` 只保留全局分配器与入口调用；本模块负责服务端启动期的
//! 配置、数据库自举、后台任务生命周期和路由组合。路由装配可能被重复调用
//! （例如测试），因此后台任务启动必须具备进程级幂等性。

use std::future::IntoFuture;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use axum::extract::{Request, State};
use axum::response::{IntoResponse, Response};
use axum::serve::ListenerExt;
use leptos::config::{Env, LeptosOptions};
use leptos_axum::LeptosRoutes;

const IMAGE_UPLOAD_MAX_BYTES: usize = 10 * 1024 * 1024;
const IMAGE_UPLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const BACKUP_IMPORT_TIMEOUT: Duration = Duration::from_secs(600);
const EXPORT_TIMEOUT: Duration = Duration::from_secs(120);
const APP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// 收到退出信号后等待在途请求结束的上限；须小于容器停止超时（podman 默认 10 秒），
/// 否则仍会被 SIGKILL。
const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

static BACKGROUND_TASKS_STARTED: OnceLock<()> = OnceLock::new();

#[derive(Clone, Copy)]
struct ServerOptions {
    ssr_cache_secs: u64,
    expose_version_headers: bool,
}

/// 服务端入口。
pub fn run() {
    // 加载 .env 环境变量；不存在 .env 是正常部署场景。
    dotenvy::dotenv().ok();
    init_tracing();
    crate::build_info::log_build_info();
    validate_required_configuration();
    crate::api::csrf::warn_if_app_base_url_unset();

    run_database_bootstrap();

    // 启动与开发热重载时清除进程内 HTML 快照，避免保留旧版本渲染结果。
    crate::ssr_cache::invalidate_ssr_all_public();
    crate::ssr_cache::invalidate_ssr_route("/admin/preview");

    let options = ServerOptions {
        ssr_cache_secs: crate::utils::server::parse_ssr_cache_secs(),
        expose_version_headers: crate::utils::server::parse_env_bool(
            "EXPOSE_VERSION_HEADERS",
            true,
        ),
    };
    tracing::info!(
        ssr_cache_secs = options.ssr_cache_secs,
        "匿名公开页 HTML 缓存：32 MiB 上限、写入通知失效、TTL 兜底"
    );
    tracing::info!(
        expose_version_headers = options.expose_version_headers,
        "版本响应头开关(Server / X-Yggdrasil-Version / X-Yggdrasil-Git / X-Yggdrasil-Hash)"
    );

    serve_application(options);
}

fn init_tracing() {
    use tracing_subscriber::prelude::*;

    // capture 层在迁移 runtime 之前安装：启动期日志先进 mpsc 缓冲，
    // log_writer 启动后批量补写落库。
    let fmt_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer().with_filter(fmt_filter);
    let capture_layer = crate::api::logs::capture::CaptureLayer
        .with_filter(crate::api::logs::capture::log_viewer_filter());
    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(capture_layer)
        .init();
}

fn validate_required_configuration() {
    if std::env::var("DATABASE_URL").is_err() {
        tracing::error!(
            "DATABASE_URL environment variable not set. Make sure .env exists or the variable is exported."
        );
        eprintln!("ERROR: DATABASE_URL environment variable not set");
        eprintln!(
            "HINT: create a .env file with DATABASE_URL=postgres://user:pass@host:5432/dbname"
        );
        std::process::exit(1);
    }

    // 必须在任何 DB_POOL.get() 调用之前执行，避免配置错误落入 LazyLock
    // 内的不可达 panic 路径。
    if let Err(error) = crate::db::pool::validate_database_url() {
        tracing::error!(%error);
        eprintln!("ERROR: {error}");
        if error.starts_with("DB_POOL_SIZE") {
            eprintln!("HINT: DB_POOL_SIZE must be a positive integer (e.g. 20).");
        } else {
            eprintln!("HINT: expected something like postgres://user:pass@host:5432/dbname");
        }
        std::process::exit(1);
    }
}

fn run_database_bootstrap() {
    // 启动期工作主要是单连接 I/O；Argon2 等 CPU 工作由 spawn_blocking 执行，
    // 因此不需要为这个一次性阶段创建 Tokio worker pool。
    let migrate_rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "failed to build migration runtime");
            eprintln!("ERROR: failed to build migration runtime: {error}");
            std::process::exit(1);
        }
    };

    migrate_rt.block_on(async {
        tracing::info!("running database migrations");

        if let Err(error) = crate::db::pool::ensure_database().await {
            tracing::error!("failed to ensure target database exists: {error}");
            eprintln!("ERROR: failed to ensure target database exists: {error}");
            eprintln!("HINT: verify DATABASE_URL; the role needs CREATEDB (or CREATE privilege on the 'postgres' DB) to auto-create the target database.");
            std::process::exit(1);
        }

        let mut conn = match crate::db::pool::get_conn_for_startup().await {
            Ok(conn) => conn,
            Err(error) => {
                let secs = crate::utils::server::parse_migrate_startup_timeout();
                tracing::error!(%error, "could not connect to database within {secs}s startup window");
                eprintln!("ERROR: could not connect to database within {secs}s startup window: {error}");
                eprintln!("HINT: is PostgreSQL running and reachable at the configured DATABASE_URL?");
                eprintln!("HINT: raise MIGRATE_STARTUP_TIMEOUT_SECS if the DB needs longer to start.");
                std::process::exit(1);
            }
        };

        if let Err(error) = crate::db::migrate::run_on_conn(&mut conn).await {
            tracing::error!("database migration failed: {error}");
            eprintln!("ERROR: database migration failed: {error}");
            eprintln!("HINT: check the logs above; verify DATABASE_URL and that PostgreSQL is healthy.");
            std::process::exit(1);
        }

        if let Err(error) = crate::api::settings::bootstrap_startup_settings(&conn).await {
            tracing::error!(error = ?error, "critical startup settings failed to load");
            eprintln!("ERROR: critical startup settings failed to load: {error:?}");
            eprintln!("HINT: verify the settings table and PostgreSQL health; the server will not start with unknown security limits.");
            std::process::exit(1);
        }

        // ADMIN_* 是启动凭据源；同步失败仍不阻止博客本身启动。
        if let Err(error) = crate::api::auth::sync_admin_from_env(&conn).await {
            tracing::warn!(error = ?error, "初始管理员 env 同步失败");
        }

    });

    drop(migrate_rt);
}

fn serve_application(options: ServerOptions) -> ! {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "failed to build server runtime");
            eprintln!("ERROR: failed to build server runtime: {error}");
            std::process::exit(1);
        }
    };

    let result = runtime.block_on(async move {
        let addr = server_addr();
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(error) => fatal_bind_error(addr, error),
        };
        // 关闭 Nagle：hyper 分两次写响应头与包体，开着 Nagle 时第二次写要等对端 ACK，
        // 而反向代理的 keepalive 连接上对端会延迟 ACK 40 ms，复用连接的每个响应都慢 40 ms。
        let listener = listener.tap_io(|tcp| {
            if let Err(error) = tcp.set_nodelay(true) {
                tracing::debug!(%error, "failed to set TCP_NODELAY");
            }
        });
        // 路由装配会启动后台任务（tokio::spawn），必须在 runtime 内执行。
        let router = build_router(options);
        tracing::info!("server listening on {addr}");
        let server = axum::serve(listener, router.into_make_service())
            .with_graceful_shutdown(shutdown_signal())
            .into_future();
        let drain_deadline = async {
            crate::shutdown::requested().await;
            tokio::time::sleep(SHUTDOWN_DRAIN_TIMEOUT).await;
        };
        tokio::select! {
            result = server => result,
            () = drain_deadline => {
                tracing::warn!(
                    "在途请求 {} 秒内未结束，放弃等待并退出",
                    SHUTDOWN_DRAIN_TIMEOUT.as_secs()
                );
                Ok(())
            }
        }
    });

    match result {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            tracing::error!(%error, "server stopped unexpectedly");
            eprintln!("ERROR: server stopped unexpectedly: {error}");
            std::process::exit(1);
        }
    }
}

/// 等待 Ctrl-C 或 SIGTERM，触发优雅退出。
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("收到退出信号，开始优雅关闭");
    crate::shutdown::trigger();
}

fn fatal_bind_error(addr: std::net::SocketAddr, error: impl std::fmt::Display) -> ! {
    tracing::error!(%error, "无法绑定监听地址 {addr}");
    eprintln!("ERROR: 无法绑定监听地址 {addr}: {error}");
    eprintln!(
        "HINT: 端口 {} 可能已被占用。用 `lsof -i :{}` 查看占用进程，或设置 PORT 环境变量换一个端口。",
        addr.port(),
        addr.port()
    );
    std::process::exit(1);
}

fn spawn_background_tasks_once() {
    if BACKGROUND_TASKS_STARTED.set(()).is_err() {
        return;
    }

    use crate::tasks::{self, spawn_supervised};
    spawn_supervised("ip_purge", tasks::ip_purge::run_purge());
    spawn_supervised("session_cleanup", tasks::session_cleanup::run_cleanup());
    spawn_supervised("post_purge", tasks::post_purge::run_purge());
    spawn_supervised("backup", tasks::backup::run_scheduler());
    spawn_supervised("cache_sync", tasks::cache_sync::run());
    spawn_supervised(
        "image_cache_cleanup",
        tasks::image_cache_cleanup::run_cleanup(),
    );
    spawn_supervised("orphan_asset_purge", tasks::orphan_asset_purge::run_purge());
    spawn_supervised("log_writer", tasks::log_writer::run_writer());
    spawn_supervised("log_purge", tasks::log_purge::run_purge());
    tasks::sysinfo_sampler::spawn_sampler();
    tokio::spawn(crate::api::code_runner::readiness::log_runner_readiness());
}

/// 发布命名空间（plan §6.7）：一个 SSR 缓存实例只服务一个发布，
/// 发布变化即整表失效，杜绝缓存命中跨发布「串味」。release 取
/// 发布清单（`<site_root>/release-manifest.json`）的 release_id；
/// dev 无清单时回退构建信息（git describe 含 dirty 标记）。
fn init_cache_namespace(mode: &str) {
    let release = std::fs::read_to_string(site_root().join("release-manifest.json"))
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|value| {
            value
                .get("release_id")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| crate::build_info::BUILD_INFO.git_describe.to_owned());
    crate::ssr_cache::set_release_namespace(&format!("{mode}/{release}"));
}

/// 上一发布资源目录（plan §6.7 滚动发布的旧资源保留窗口）。配置了但不是
/// 目录时仅告警停用——保留窗口是兜底增强，不能反过来阻止服务启动。
fn previous_public_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("YGG_PREVIOUS_PUBLIC_DIR").filter(|v| !v.is_empty())?;
    let dir = PathBuf::from(dir);
    if !dir.is_dir() {
        tracing::warn!(
            dir = %dir.display(),
            "YGG_PREVIOUS_PUBLIC_DIR 不是目录，旧发布资源保留窗口停用"
        );
        return None;
    }
    Some(dir)
}

/// 站点根目录（`public/` 资源与 `pkg/` 前端产物所在）。
///
/// `LEPTOS_SITE_ROOT` 非空则用它（cargo-leptos 在 watch/serve 时自动设置，
/// Docker 镜像显式设置）；否则是可执行文件旁的 `public/`。
pub(crate) fn site_root() -> PathBuf {
    match std::env::var_os("LEPTOS_SITE_ROOT").filter(|v| !v.is_empty()) {
        Some(root) => PathBuf::from(root),
        None => exe_dir().join("public"),
    }
}

/// 可执行文件所在目录；`hash.txt` 与它同目录。
fn exe_dir() -> PathBuf {
    let mut dir = std::env::current_exe().unwrap_or_else(|e| {
        panic!("无法定位可执行文件以推导站点目录：{e}");
    });
    dir.pop();
    dir
}

/// 监听地址：`IP`（默认 127.0.0.1）+ `PORT`（默认 8080）。解析失败直接退出。
fn server_addr() -> std::net::SocketAddr {
    fn env_or(key: &str, default: &str) -> String {
        std::env::var(key)
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| default.to_string())
    }

    let ip = env_or("IP", "127.0.0.1");
    let port = env_or("PORT", "8080");
    let ip = ip.parse::<std::net::IpAddr>().unwrap_or_else(|error| {
        eprintln!("ERROR: IP 环境变量 {ip:?} 不是有效的 IP 地址：{error}");
        std::process::exit(1);
    });
    let port = port.parse::<u16>().unwrap_or_else(|error| {
        eprintln!("ERROR: PORT 环境变量 {port:?} 不是有效的端口号：{error}");
        std::process::exit(1);
    });
    std::net::SocketAddr::new(ip, port)
}

/// Leptos 运行配置，与 `Cargo.toml` 的 `[package.metadata.leptos]` 保持一致。
fn leptos_options() -> LeptosOptions {
    LeptosOptions::builder()
        .output_name("yggdrasil")
        .site_root(site_root().to_string_lossy().into_owned())
        .site_pkg_dir("pkg")
        .site_addr(server_addr())
        .reload_port(3001)
        .hash_files(true)
        .env(if cfg!(debug_assertions) {
            Env::DEV
        } else {
            Env::PROD
        })
        .build()
}

/// 校验前端产物与服务端二进制配套：`hash.txt`（与可执行文件同目录）指向的
/// 脚本、WASM 与分包清单必须在 `<site_root>/pkg/` 下真实存在，缺失即拒绝启动，
/// 不静默拼出混合版本。debug 与 release 都执行。
///
/// 返回入口是否已按内容重命名（hash.txt 的 `entry: content-addressed`，由发布装配写入）。
fn validate_site_assets(options: &LeptosOptions) -> bool {
    let hash_path = exe_dir().join("hash.txt");
    let hashes = std::fs::read_to_string(&hash_path).unwrap_or_else(|_| {
        panic!(
            "hash.txt 缺失（{}）：前端产物与服务端二进制不配套，拒绝启动",
            hash_path.display()
        )
    });

    let (mut js, mut wasm, mut manifest, mut entry_content_addressed) = (None, None, None, false);
    for line in hashes.lines() {
        if let Some((key, value)) = line.trim().split_once(':') {
            let value = value.trim().to_string();
            match key.trim() {
                "js" => js = Some(value),
                "wasm" => wasm = Some(value),
                "manifest" => manifest = Some(value),
                "entry" => entry_content_addressed = value == "content-addressed",
                _ => {}
            }
        }
    }

    let pkg = Path::new(options.site_root.as_ref()).join(options.site_pkg_dir.as_ref());
    let name = options.output_name.as_ref();
    let mut missing: Vec<String> = Vec::new();
    match js {
        Some(hash) => {
            let file = pkg.join(format!("{name}.{hash}.js"));
            if !file.is_file() {
                missing.push(file.display().to_string());
            }
        }
        None => missing.push(format!("{}（缺少 js 行）", hash_path.display())),
    }
    match wasm {
        Some(hash) => {
            let file = pkg.join(format!("{name}.{hash}.wasm"));
            if !file.is_file() {
                missing.push(file.display().to_string());
            }
        }
        None => missing.push(format!("{}（缺少 wasm 行）", hash_path.display())),
    }
    if let Some(hash) = manifest {
        let file = pkg.join(format!("__wasm_split_manifest.{hash}.json"));
        if !file.is_file() {
            missing.push(file.display().to_string());
        }
    }
    if !missing.is_empty() {
        panic!("前端产物缺失：{missing:?}——前端产物与服务端二进制不配套，拒绝启动");
    }
    entry_content_addressed
}

/// 未匹配任何路由的请求：先查站点目录里的静态文件（带 `.br` 预压缩协商），
/// 其余交给 Leptos 渲染（未知页面由 `NotFound` 设置 404）。`/pkg/` 下缺失的文件
/// 直接返回空 404，不渲染页面。
async fn site_fallback(State(options): State<LeptosOptions>, req: Request) -> Response {
    use axum::http::{Method, StatusCode};
    use tower::ServiceExt;

    if req.method() != Method::GET && req.method() != Method::HEAD {
        return StatusCode::NOT_FOUND.into_response();
    }

    let mut static_request = axum::http::Request::builder()
        .method(req.method())
        .uri(req.uri().clone());
    for (name, value) in req.headers() {
        static_request = static_request.header(name, value);
    }
    if let Ok(static_request) = static_request.body(axum::body::Body::empty()) {
        let served = tower_http::services::ServeDir::new(site_root())
            .append_index_html_on_directories(false)
            .precompressed_br()
            .oneshot(static_request)
            .await;
        // ServeDir 的错误类型是 Infallible，解构不会失败。
        let Ok(asset) = served;
        if asset.status() != StatusCode::NOT_FOUND {
            return asset.map(axum::body::Body::new);
        }
    }

    if req.uri().path().starts_with("/pkg/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    leptos_axum::render_app_async(move || crate::app::shell(options.clone()))(req).await
}

fn build_router(options: ServerOptions) -> axum::Router {
    use axum::http::StatusCode;
    use tower_http::timeout::TimeoutLayer;

    let leptos_opts = leptos_options();
    let entry_content_addressed = validate_site_assets(&leptos_opts);
    init_cache_namespace("leptos");

    spawn_background_tasks_once();

    // 前台与后台页面由 Leptos 渲染；server fn 随 `leptos_routes` 自动注册到 /api/<名称>。
    // 未匹配的路径（静态资源、未知页面）交给 `site_fallback`。
    let routes = leptos_axum::generate_route_list(crate::app::App);
    let leptos_app = axum::Router::new()
        .leptos_routes(&leptos_opts, routes, {
            let leptos_opts = leptos_opts.clone();
            move || crate::app::shell(leptos_opts.clone())
        })
        .fallback(site_fallback)
        .with_state(leptos_opts.clone());

    // 旧发布资源保留窗口（plan §6.7）：滚动更新后，已打开标签页与缓存 HTML
    // 仍引用上一发布的带 hash 资源（含按路由懒加载的 WASM 分包）；当前发布未收录时
    // 按 404 回退到 YGG_PREVIOUS_PUBLIC_DIR，保留窗口覆盖部署、HTML 缓存、回滚与长开标签页。
    let leptos_app = match previous_public_dir() {
        Some(previous_root) => {
            tracing::info!(
                dir = %previous_root.display(),
                "旧发布资源保留窗口已启用（/pkg 缺失时回退）"
            );
            leptos_app.layer(axum::middleware::from_fn(
                move |req: axum::extract::Request, next: axum::middleware::Next| {
                    crate::middleware::previous_release_assets(previous_root.clone(), req, next)
                },
            ))
        }
        None => leptos_app,
    };

    let upload_route = axum::Router::new()
        .route(
            "/api/notes/upload",
            axum::routing::post(crate::api::notes::attachments::upload),
        )
        .route(
            "/api/upload",
            axum::routing::post(crate::api::upload::upload_image),
        )
        .route(
            "/api/comments/upload",
            axum::routing::post(crate::api::upload::comment_upload_image),
        )
        .layer(axum::extract::DefaultBodyLimit::max(IMAGE_UPLOAD_MAX_BYTES))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            IMAGE_UPLOAD_TIMEOUT,
        ))
        .layer(axum::middleware::from_fn(crate::api::csrf::csrf_middleware));

    let mcp_upload_route = axum::Router::new()
        .route(
            "/api/mcp/upload",
            axum::routing::post(crate::api::upload::mcp_upload_image),
        )
        .layer(axum::extract::DefaultBodyLimit::max(IMAGE_UPLOAD_MAX_BYTES))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            IMAGE_UPLOAD_TIMEOUT,
        ));

    let backup_import_max = crate::api::database::backup::import_max_bytes();
    tracing::info!(
        max_mb = backup_import_max / 1024 / 1024,
        "备份导入单文件上限生效（BACKUP_IMPORT_MAX_MB）"
    );
    let backup_import_limit = backup_import_max
        .saturating_add(crate::api::database::backup::MULTIPART_FRAME_SLACK)
        .min(usize::MAX as u64) as usize;
    let backup_import_route = axum::Router::new()
        .route(
            "/api/database/backups/import",
            axum::routing::post(crate::api::database::backup::import_backup),
        )
        .layer(axum::extract::DefaultBodyLimit::max(backup_import_limit))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            BACKUP_IMPORT_TIMEOUT,
        ))
        .layer(axum::middleware::from_fn(crate::api::csrf::csrf_middleware));

    let export_route = axum::Router::new()
        .route(
            "/api/database/export",
            axum::routing::get(crate::api::database::export::export_data).layer(
                axum::middleware::from_fn(crate::api::csrf::same_origin_read_middleware),
            ),
        )
        .route(
            "/api/database/backups/{filename}",
            axum::routing::get(crate::api::database::backup::download_backup),
        )
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            EXPORT_TIMEOUT,
        ))
        .layer(axum::middleware::from_fn(crate::api::csrf::csrf_middleware));

    let sse_route = axum::Router::new()
        .route(
            "/api/exec/stream",
            axum::routing::get(crate::api::code_runner::sse::exec_stream),
        )
        .layer(axum::middleware::from_fn(crate::api::csrf::csrf_middleware));

    let logs_sse_route = axum::Router::new()
        .route(
            "/api/logs/stream",
            axum::routing::get(crate::api::logs::sse::log_stream),
        )
        .layer(axum::middleware::from_fn(crate::api::csrf::csrf_middleware));

    let mut app_routes = leptos_app
        .layer(axum::middleware::from_fn(
            crate::middleware::share_context_middleware,
        ))
        .layer(axum::middleware::from_fn(
            crate::ssr_cache::cache_public_html,
        ))
        .layer(axum::middleware::from_fn(
            crate::middleware::ssr_generation_middleware,
        ))
        .layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                crate::middleware::add_cache_control(entry_content_addressed, req, next)
            },
        ))
        .layer(axum::middleware::from_fn(crate::api::csrf::csrf_middleware));
    let compression = crate::middleware::compression_layer_from_env();
    if let Some(layer) = compression.clone() {
        app_routes = app_routes.layer(layer);
    }
    let app_routes = app_routes.layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        APP_REQUEST_TIMEOUT,
    ));
    let app_routes = app_routes.layer(axum::middleware::from_fn(crate::middleware::admin_guard));

    let static_routes = axum::Router::new()
        .route(
            "/note-media/{id}",
            axum::routing::get(crate::api::notes::attachments::read),
        )
        // 旧收藏链接直接返回永久重定向；Leptos 路由表不注册 /tags（同一路径注册两次会让 merge panic）。
        .route(
            "/tags",
            axum::routing::get(|| async { axum::response::Redirect::permanent("/archives") }),
        )
        .route(
            "/og/site.png",
            axum::routing::get(crate::api::og::site_image),
        )
        .route(
            "/og/post/{file}",
            axum::routing::get(crate::api::og::post_image),
        )
        .route("/healthz", axum::routing::get(crate::api::health::healthz))
        .route("/readyz", axum::routing::get(crate::api::health::readyz))
        .route(
            "/uploads/{*path}",
            axum::routing::get(crate::api::image::serve_image),
        )
        .route(
            "/uploads",
            axum::routing::get(|| async { StatusCode::NOT_FOUND }),
        );

    // Feed 含 20 篇全文（MB 级），反代 gzip_types 也不含其 MIME，必须由压缩层处理。
    let mut feed_routes = axum::Router::new()
        .route("/feed.xml", axum::routing::get(crate::api::feed::rss_feed))
        .route(
            "/feed.json",
            axum::routing::get(crate::api::feed::json_feed),
        );
    if let Some(layer) = compression {
        feed_routes = feed_routes.layer(layer);
    }

    let router = upload_route
        .merge(mcp_upload_route)
        .merge(backup_import_route)
        .merge(export_route)
        .merge(sse_route)
        .merge(logs_sse_route)
        .merge(app_routes)
        .merge(static_routes)
        .merge(feed_routes)
        .merge(crate::mcp::router::mcp_route());

    // 最外层兜底：任何路由或中间件在响应头发出前 panic 都返回 500，而不是断开连接。
    let router = router.layer(crate::middleware::catch_panic_layer());

    if options.expose_version_headers {
        router.layer(axum::middleware::from_fn(
            crate::middleware::version_headers_middleware,
        ))
    } else {
        router
    }
}
