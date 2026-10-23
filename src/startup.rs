//! 服务端启动流程与 Axum 路由组装。
//!
//! `main.rs` 只保留目标选择与全局分配器；本模块负责服务端启动期的
//! 配置、数据库自举、后台任务生命周期和路由组合。Dioxus 开发态会在
//! 热重载时多次调用 router callback，因此后台任务启动必须具备进程级幂等性。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use dioxus::server::axum;
use dioxus::server::{DioxusRouterExt, FullstackState, ServeConfig};

const IMAGE_UPLOAD_MAX_BYTES: usize = 10 * 1024 * 1024;
const IMAGE_UPLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const BACKUP_IMPORT_TIMEOUT: Duration = Duration::from_secs(600);
const EXPORT_TIMEOUT: Duration = Duration::from_secs(120);
const APP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// reader 客户端模板目录（相对 public 根）。目录里的 index.html 由发布构建
/// 从 reader 产物复制而来；full 模板始终是 public/index.html（dev 与发布一致）。
const READER_TEMPLATE_DIR: &str = "flavors/reader";

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
    #[cfg(debug_assertions)]
    {
        // 开发态继续使用 Dioxus serve 以保留 devtools/hot reload。这里只做
        // best-effort 预检；真实 listener 仍由 Dioxus 绑定。
        let addr = dioxus::cli_config::fullstack_address_or_localhost();
        if let Err(error) = std::net::TcpListener::bind(addr) {
            fatal_bind_error(addr, error);
        }
        dioxus::server::serve(move || async move { Ok(build_router(options)) });
    }

    #[cfg(not(debug_assertions))]
    {
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
            let addr = dioxus::cli_config::fullstack_address_or_localhost();
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(error) => fatal_bind_error(addr, error),
            };
            tracing::info!("server listening on {addr}");
            axum::serve(listener, build_router(options).into_make_service()).await
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

    tokio::spawn(crate::tasks::ip_purge::run_purge());
    tokio::spawn(crate::tasks::session_cleanup::run_cleanup());
    tokio::spawn(crate::tasks::post_purge::run_purge());
    tokio::spawn(crate::tasks::backup::run_scheduler());
    tokio::spawn(crate::tasks::cache_sync::run());
    tokio::spawn(crate::tasks::image_cache_cleanup::run_cleanup());
    tokio::spawn(crate::tasks::orphan_asset_purge::run_purge());
    tokio::spawn(crate::tasks::log_writer::run_writer());
    tokio::spawn(crate::tasks::log_purge::run_purge());
    crate::tasks::sysinfo_sampler::spawn_sampler();
    tokio::spawn(crate::api::code_runner::readiness::log_runner_readiness());
}

fn build_flavor_states() -> FlavorStates {
    validate_public_templates();
    let full = FullstackState::new(ServeConfig::new(), crate::router::AppRouter);
    match load_reader_state() {
        Some(reader) => {
            tracing::info!(
                "双产物模式：公开阅读路径使用 reader 客户端模板（{}），后台/登录/图鉴使用 full 模板",
                READER_TEMPLATE_DIR
            );
            init_cache_namespace("dual");
            FlavorStates {
                reader: Some(reader),
                full,
            }
        }
        None => {
            tracing::info!(
                "完整应用模式：未发现 reader 客户端产物（public/{}/index.html），全部路由使用 full 模板",
                READER_TEMPLATE_DIR
            );
            init_cache_namespace("full");
            FlavorStates { reader: None, full }
        }
    }
}

/// 发布×flavor 命名空间（plan §6.7）：一个 SSR 缓存实例只服务一个组合，
/// 组合变化即整表失效，杜绝缓存命中跨发布/跨 flavor「串味」。release 取
/// 发布清单的 release_id；dev 无清单时回退构建信息（git describe 含 dirty 标记）。
fn init_cache_namespace(mode: &str) {
    let release = std::fs::read_to_string(dioxus_public_root().join("release-manifest.json"))
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

/// 双客户端产物的 SSR 状态。
#[derive(Clone)]
struct FlavorStates {
    /// reader 模板；`None` = 完整应用回退模式（dev 单产物 / full-only 发布）。
    reader: Option<FullstackState>,
    full: FullstackState,
}

/// 复刻 dioxus-server 的 public 目录解析（env 优先，其次可执行文件旁的
/// public/）。dioxus-server 0.7.10 的 `public_path()` 是 pub(crate)，无法
/// 直接调用；这里保持同一优先级，保证模板查找与静态资源目录一致。
fn dioxus_public_root() -> PathBuf {
    if let Some(p) = std::env::var_os("DIOXUS_PUBLIC_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let mut dir = std::env::current_exe().unwrap_or_else(|e| {
        panic!("无法定位可执行文件以推导 public 目录：{e}");
    });
    dir.pop();
    dir.join("public")
}

/// 加载 reader 客户端模板，产出对应 [`FullstackState`]。
///
/// dioxus-server 0.7.10 未公开 `IndexHtml`（lib.rs 中 pub(crate)），
/// `ServeConfig::with_index_html` 无法从外部构造；`ServeConfig::new()` 又只读
/// public 根的 index.html。因此唯一的模板注入通道是 `DIOXUS_PUBLIC_PATH`：
/// 启动期临时把它指向 `{public}/flavors/reader`，读完立即还原。切换只发生
/// 在 build_router 的顺序执行段内，还原发生在注册静态资源之前，运行期不再
/// 读取该变量。
///
/// - 目录/文件不存在（dev 单产物、full-only 发布）→ `None`，走完整应用模式。
/// - 文件存在但读取/解析失败 → panic 明确失败，不静默降级（plan §6.5）。
fn load_reader_state() -> Option<FullstackState> {
    let public_root = dioxus_public_root();
    let reader_dir = public_root.join(READER_TEMPLATE_DIR);
    let reader_index = reader_dir.join("index.html");
    if !reader_index.is_file() {
        return None;
    }

    let html = std::fs::read_to_string(&reader_index).unwrap_or_else(|e| {
        panic!(
            "reader 客户端模板读取失败（{}）：{e}",
            reader_index.display()
        )
    });
    validate_template_assets(&public_root, "reader", &html);

    let saved = std::env::var("DIOXUS_PUBLIC_PATH").ok();
    std::env::set_var("DIOXUS_PUBLIC_PATH", &reader_dir);
    let config = ServeConfig::new();
    match saved {
        Some(v) => std::env::set_var("DIOXUS_PUBLIC_PATH", v),
        None => std::env::remove_var("DIOXUS_PUBLIC_PATH"),
    }
    Some(FullstackState::new(config, crate::router::AppRouter))
}

/// 校验 full 模板存在且其引用的 /assets/* 产物齐全（plan §6.5：缺失产物
/// 明确失败，不静默拼出混合版本）。full 模板缺失仅在 release 拒绝启动；
/// dev 保留 dioxus 既有的 ssr_only 回退（dx serve 冷启动时序需要）。
fn validate_public_templates() {
    let public_root = dioxus_public_root();
    let full_index = public_root.join("index.html");
    if !full_index.is_file() {
        if cfg!(not(debug_assertions)) {
            panic!(
                "full 客户端模板缺失（{}）：发布目录不完整，拒绝启动",
                full_index.display()
            );
        }
        tracing::warn!("public/index.html 不存在，dev 下沿用 dioxus 的 ssr_only 回退");
        return;
    }
    let html = std::fs::read_to_string(&full_index)
        .unwrap_or_else(|e| panic!("full 客户端模板读取失败：{e}"));
    validate_template_assets(&public_root, "full", &html);
}

/// 扫描模板里的 /assets/ 引用，确认文件在 public/assets/ 下真实存在。
/// dx 生成的引用形如 `/./assets/yggdrasil-dxh<hash>.js`，引用缺失即产物闭包
/// 断裂（陈旧模板、合并遗漏），启动期直接失败。
fn validate_template_assets(public_root: &Path, flavor: &str, html: &str) {
    let marker = "/assets/";
    let mut missing: Vec<String> = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = html[cursor..].find(marker) {
        let start = cursor + offset + marker.len();
        let end = html[start..]
            .find(|c: char| c.is_ascii_whitespace() || matches!(c, '"' | '\'' | '>' | ')'))
            .map_or(html.len(), |rel| start + rel);
        let name = html[start..end].trim_start_matches("./");
        if !name.is_empty() {
            let asset = public_root.join("assets").join(name);
            if !asset.is_file() {
                missing.push(format!("/assets/{name}"));
            }
        }
        cursor = end.max(start);
    }
    if !missing.is_empty() {
        panic!("{flavor} 客户端模板引用的产物缺失：{missing:?}——发布资源闭包不完整，拒绝启动");
    }
}

fn build_router(options: ServerOptions) -> axum::Router {
    use axum::extract::Request;
    use axum::http::StatusCode;
    use tower_http::timeout::TimeoutLayer;

    spawn_background_tasks_once();

    // 双客户端产物模板：full 恒在（public/index.html），reader 缺席时整体
    // 回退「完整应用模式」（plan §6.5 允许的回退路径）。
    let flavors = build_flavor_states();
    // 独立笔记入口是公开阅读路径，固定使用 reader 模板（缺席则回退 full）。
    let notes_state = flavors
        .reader
        .clone()
        .unwrap_or_else(|| flavors.full.clone());

    // 公开渲染 fallback：按路径固定选择 flavor（plan §6.5），不依赖登录态。
    // 闭包自带两个 FullstackState——dioxus 0.7.10 的 render_handler 以
    // State<FullstackState> 取模板，这里绕开 Router 单一 state 的限制；
    // register_server_functions 需要 Router<FullstackState>，headless 只作
    // 占位 state（文档示例同款用法，无 handler 消费它）。
    let full_state = flavors.full.clone();
    let reader_state = flavors.reader.clone();
    let dioxus_app = axum::Router::<FullstackState>::new()
        .register_server_functions()
        .serve_static_assets()
        .fallback(axum::routing::get(move |request: Request| {
            let reader = reader_state.clone();
            let full = full_state.clone();
            async move {
                let state = match (&reader, crate::router::is_reader_path(request.uri().path())) {
                    (Some(reader), true) => reader.clone(),
                    (None, _) | (Some(_), false) => full.clone(),
                };
                FullstackState::render_handler(axum::extract::State(state), request).await
            }
        }))
        .with_state(FullstackState::headless());

    // 旧发布资源保留窗口（plan §6.7）：滚动更新后，已打开标签页与缓存 HTML
    // 仍引用上一发布的带 hash 资源；当前发布未收录时按 404 回退到
    // YGG_PREVIOUS_PUBLIC_DIR，保留窗口覆盖部署、HTML 缓存、回滚与长开标签页。
    let dioxus_app = match previous_public_dir() {
        Some(previous_root) => {
            tracing::info!(
                dir = %previous_root.display(),
                "旧发布资源保留窗口已启用（/assets、/wasm 缺失时回退）"
            );
            dioxus_app.layer(axum::middleware::from_fn(
                move |req: axum::extract::Request, next: axum::middleware::Next| {
                    crate::middleware::previous_release_assets(previous_root.clone(), req, next)
                },
            ))
        }
        None => dioxus_app,
    };

    // 笔记可随时撤回公开；独立 renderer 与 HTML 缓存白名单共同保证其不缓存。
    let notes_routes = axum::Router::new()
        .route("/notes", axum::routing::get(dioxus::server::render_handler))
        .route(
            "/notes/{*path}",
            axum::routing::get(dioxus::server::render_handler),
        )
        .with_state(notes_state);
    let dioxus_app = dioxus_app.merge(notes_routes);

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

    let mut app_routes = dioxus_app
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
            crate::middleware::add_cache_control,
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
        // 旧收藏链接直接返回永久重定向，客户端导航由 Route 的 redirect 同步兼容。
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

    if options.expose_version_headers {
        router.layer(axum::middleware::from_fn(
            crate::middleware::version_headers_middleware,
        ))
    } else {
        router
    }
}
