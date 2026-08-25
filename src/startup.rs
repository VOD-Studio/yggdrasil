//! 服务端启动流程与 Axum 路由组装。
//!
//! `main.rs` 只保留目标选择与全局分配器；本模块负责服务端启动期的
//! 配置、数据库自举、后台任务生命周期和路由组合。Dioxus 开发态会在
//! 热重载时多次调用 router callback，因此后台任务启动必须具备进程级幂等性。

use std::sync::OnceLock;
use std::time::Duration;

use dioxus::server::axum;

const IMAGE_UPLOAD_MAX_BYTES: usize = 10 * 1024 * 1024;
const IMAGE_UPLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const BACKUP_IMPORT_TIMEOUT: Duration = Duration::from_secs(600);
const EXPORT_TIMEOUT: Duration = Duration::from_secs(120);
const APP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

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

    let options = ServerOptions {
        ssr_cache_secs: crate::utils::server::parse_ssr_cache_secs(),
        expose_version_headers: crate::utils::server::parse_env_bool(
            "EXPOSE_VERSION_HEADERS",
            true,
        ),
    };
    tracing::info!(
        ssr_cache_secs = options.ssr_cache_secs,
        "增量渲染缓存生效（写入后内容可见滞后的上界）；调小可缩短滞后，代价是 SSR 重渲染更频繁"
    );
    tracing::info!(
        expose_version_headers = options.expose_version_headers,
        "版本响应头开关(Server / X-Yggdrasil-Version / X-Yggdrasil-Git / X-Yggdrasil-Hash)"
    );

    // Dioxus 负责自己的 server runtime 与 listener。callback 在开发态热重载时
    // 会再次执行，因此 build_router() 内的后台任务启动必须是幂等的。
    dioxus::server::serve(move || async move { Ok(build_router(options)) });
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

        // Dioxus 0.7.10 的 serve() 会再次 bind listener。这个探测只能改善
        // 常见端口占用时的错误提示，仍存在二次 bind 的 TOCTOU 竞态，不能视为
        // listener 预留或绝对保证。
        let addr = dioxus::cli_config::fullstack_address_or_localhost();
        if let Err(error) = tokio::net::TcpListener::bind(addr).await {
            tracing::error!(%error, "无法绑定监听地址 {addr}");
            eprintln!("ERROR: 无法绑定监听地址 {addr}: {error}");
            eprintln!(
                "HINT: 端口 {} 可能已被占用。用 `lsof -i :{}` 查看占用进程，或设置 PORT 环境变量换一个端口。",
                addr.port(),
                addr.port()
            );
            std::process::exit(1);
        }
    });

    drop(migrate_rt);
}

fn spawn_background_tasks_once() {
    if BACKGROUND_TASKS_STARTED.set(()).is_err() {
        return;
    }

    tokio::spawn(crate::tasks::ip_purge::run_purge());
    tokio::spawn(crate::tasks::session_cleanup::run_cleanup());
    tokio::spawn(crate::tasks::post_purge::run_purge());
    tokio::spawn(crate::tasks::backup::run_scheduler());
    tokio::spawn(crate::tasks::image_cache_cleanup::run_cleanup());
    tokio::spawn(crate::tasks::orphan_asset_purge::run_purge());
    tokio::spawn(crate::tasks::log_writer::run_writer());
    tokio::spawn(crate::tasks::log_purge::run_purge());
    crate::tasks::sysinfo_sampler::spawn_sampler();
    tokio::spawn(crate::api::code_runner::readiness::log_runner_readiness());
}

fn build_router(options: ServerOptions) -> axum::Router {
    use axum::http::StatusCode;
    use dioxus::server::{DioxusRouterExt, ServeConfig};
    use tower_http::timeout::TimeoutLayer;

    spawn_background_tasks_once();

    let config = ServeConfig::builder().incremental(
        dioxus::server::IncrementalRendererConfig::default()
            .invalidate_after(Duration::from_secs(options.ssr_cache_secs)),
    );

    let upload_route = axum::Router::new()
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
            axum::routing::get(crate::api::database::export::export_data),
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

    let dioxus_app = axum::Router::new().serve_dioxus_application(config, crate::router::AppRouter);

    let mut app_routes = dioxus_app
        .layer(axum::middleware::from_fn(
            crate::middleware::ssr_generation_middleware,
        ))
        .layer(axum::middleware::from_fn(
            crate::middleware::add_cache_control,
        ))
        .layer(axum::middleware::from_fn(crate::api::csrf::csrf_middleware));
    if let Some(layer) = crate::middleware::compression_layer_from_env() {
        app_routes = app_routes.layer(layer);
    }
    let app_routes = app_routes.layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        APP_REQUEST_TIMEOUT,
    ));
    let app_routes = app_routes.layer(axum::middleware::from_fn(crate::middleware::admin_guard));

    let static_routes = axum::Router::new()
        .route("/healthz", axum::routing::get(crate::api::health::healthz))
        .route("/readyz", axum::routing::get(crate::api::health::readyz))
        .route(
            "/uploads/{*path}",
            axum::routing::get(crate::api::image::serve_image),
        )
        .route(
            "/uploads",
            axum::routing::get(|| async { StatusCode::NOT_FOUND }),
        )
        .route("/feed.xml", axum::routing::get(crate::api::feed::rss_feed))
        .route(
            "/feed.json",
            axum::routing::get(crate::api::feed::json_feed),
        );

    let router = upload_route
        .merge(mcp_upload_route)
        .merge(backup_import_route)
        .merge(export_route)
        .merge(sse_route)
        .merge(logs_sse_route)
        .merge(app_routes)
        .merge(static_routes)
        .merge(crate::mcp::router::mcp_route());

    if options.expose_version_headers {
        router.layer(axum::middleware::from_fn(
            crate::middleware::version_headers_middleware,
        ))
    } else {
        router
    }
}
