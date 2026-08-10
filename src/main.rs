//! 服务端入口与启动配置
//!
//! 本文件是 Dioxus fullstack 应用的启动入口。
//! 当启用 `server` feature 时，启动 Axum 服务器并挂载：
//! - Dioxus server function（由 `serve_dioxus_application` 自动注册）；
//! - 自定义 Axum 路由：图片上传 `/api/upload`、图片服务 `/uploads/{*path}`；
//! - 增量渲染（Incremental Rendering）缓存配置。
//!
//! 当未启用 `server` feature（例如编译为 WASM 前端）时，
//! 仅调用 `dioxus::launch` 启动客户端应用。

// 全局内存分配器：mimalloc。
// 多线程高频小对象分配场景下吞吐显著优于系统 malloc，且对全静态 musl 链接友好。
// cfg 门控（与项目「双目标编译」约定一致）：
//   - feature = "server"：分配器只服务端二进制需要。
//   - not(wasm32)：mimalloc_rust 在 wasm32 上无法编译（mimalloc_rust Issue #76），
//     WASM 前端走默认分配器。两个门控同时满足才注册。
#[cfg(all(feature = "server", not(target_arch = "wasm32")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// 业务模块
mod api;
mod auth;
// build_info:编译期注入的 git/rustc/构建时间信息。模块内部 gate 在 server feature,
// 模块声明本身不需要再加 cfg(空模块在 WASM 端也能编译)。
mod build_info;
mod cache;
mod components;
mod context;
mod db;
pub mod infra;
// highlight 模块仅在服务端构建时编译
#[cfg(feature = "server")]
mod highlight;
// middleware：Axum 中间件与启动期纯函数（cache-control / admin 守卫 / 压缩层），
// server-only。从 main.rs 抽出以便独立测试，路由组装处以 crate::middleware::xxx 调用。
mod hooks;
#[cfg(feature = "server")]
mod middleware;
mod models;
// mcp：Model Context Protocol 服务器（/mcp Streamable HTTP，bearer token 鉴权）。
// 仅 server feature 编译；WASM 前端不引用任何 mcp 符号。
// allow(dead_code)：原用于掩盖 T1 tracer bullet 期间未接线的 mcp/resources.rs（273 行
// 完整资源子系统，从未 override ServerHandler::list_resources/read_resource）。
// 已删除该模块（D1）—— MCP 现无死代码，allow 同步移除，以免未来再次静默掩盖死代码。
#[cfg(feature = "server")]
mod mcp;
mod pages;
mod router;
// sysinfo_sampler：主机指标快照。
// SystemSnapshot 结构体两端都编译（被 system_status 的 ServerStatus 字段引用）；
// 真正的采样任务 / RwLock / read_snapshot 实现在本模块内部自行 #[cfg(feature = "server")] gate。
mod sysinfo_sampler;
// ssr_cache 仅在 server feature 启用时编译；保存 SSR 世代号失效状态。
#[cfg(feature = "server")]
mod ssr_cache;
mod tasks;
mod theme;
// tiptap_bridge：共享类型（UploadsInFlight/UploadErrorEntry）两端都编译；
// wasm-bindgen extern 与 EditorHandle 在内部的 #[cfg(wasm32)] 子模块里。
mod tiptap_bridge;
// codemirror_bridge：SQL 编辑器的 wasm-bindgen 绑定，结构镜像 tiptap_bridge。
// 共享类型（SqlSchema/SqlTable）两端都编译；extern 与 EditorHandle 在 #[cfg(wasm32)] 子模块里。
mod codemirror_bridge;
mod utils;
mod webp;
mod xterm_bridge;

/// 程序入口
fn main() {
    // server feature：启动服务端
    #[cfg(feature = "server")]
    {
        // 加载 .env 环境变量
        dotenvy::dotenv().ok();
        // 初始化 tracing 日志，默认级别为 info
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .init();

        // 打印构建元信息(版本 / git / 提交时间 / rustc / 编译时刻)。
        // 必须在 tracing 初始化之后,否则日志被丢弃。
        build_info::log_build_info();

        // 校验数据库连接串，未设置则直接退出
        if std::env::var("DATABASE_URL").is_err() {
            tracing::error!("DATABASE_URL environment variable not set. Make sure .env exists or the variable is exported.");
            eprintln!("ERROR: DATABASE_URL environment variable not set");
            eprintln!(
                "HINT: create a .env file with DATABASE_URL=postgres://user:pass@host:5432/dbname"
            );
            std::process::exit(1);
        }

        // 前置校验 DATABASE_URL 格式 + DB_POOL_SIZE，避免触发 DB_POOL LazyLock 闭包里
        // 不可达的 .expect() panic——让用户可修复的配置错误走统一友好的 exit(1) 路径。
        // 此处必须在任何 DB_POOL.get() 调用之前执行（即迁移之前）。
        if let Err(e) = db::pool::validate_database_url() {
            tracing::error!("{e}");
            eprintln!("ERROR: {e}");
            if e.starts_with("DB_POOL_SIZE") {
                eprintln!("HINT: DB_POOL_SIZE must be a positive integer (e.g. 20).");
            } else {
                eprintln!("HINT: expected something like postgres://user:pass@host:5432/dbname");
            }
            std::process::exit(1);
        }

        // 提醒部署者显式设置 APP_BASE_URL：未设置时 CSRF 会回退到 Host 头，
        // 反向代理后存在绕过风险。本地开发未设置时也会打一条 WARN（代价可接受）。
        api::csrf::warn_if_app_base_url_unset();

        // 启动前执行数据库迁移。阻塞：完成前不监听端口。
        // 失败用 exit(1) 退出（不 panic），避免启动一个 schema 不一致的半残服务。
        // 多实例滚动发布时由咨询锁串行化，详见 src/db/migrate.rs。
        //
        // main() 是同步函数，这里用一个独立的多线程 runtime 驱动迁移的异步逻辑，
        // 完成后再交给 dioxus::server::serve() 启动它自己的 runtime。
        // 两个 runtime 不重叠，避免与 Dioxus 内部 runtime 产生交互。
        let migrate_rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build migration runtime");
        migrate_rt.block_on(async {
            tracing::info!("running database migrations");

            // 连接池指向目标库，但目标库可能尚不存在（全新部署）。
            // 先连 postgres 维护库确保目标库存在，复用启动超时窗口应对 DB 起得慢。
            if let Err(e) = db::pool::ensure_database().await {
                tracing::error!("failed to ensure target database exists: {e}");
                eprintln!("ERROR: failed to ensure target database exists: {e}");
                eprintln!("HINT: verify DATABASE_URL; the role needs CREATEDB (or CREATE privilege on the 'postgres' DB) to auto-create the target database.");
                std::process::exit(1);
            }

            // 启动期用长重试窗口拿连接：DB 可能还在初始化（docker-compose 无 healthcheck、
            // 本机忘启 Postgres 等）。窗口由 MIGRATE_STARTUP_TIMEOUT_SECS 控制，默认 30s。
            let mut conn = match db::pool::get_conn_for_startup().await {
                Ok(conn) => conn,
                Err(e) => {
                    let secs = crate::utils::server::parse_migrate_startup_timeout();
                    tracing::error!("could not connect to database within {secs}s startup window: {e}");
                    eprintln!("ERROR: could not connect to database within {secs}s startup window: {e}");
                    eprintln!("HINT: is PostgreSQL running and reachable at the configured DATABASE_URL?");
                    eprintln!("HINT: raise MIGRATE_STARTUP_TIMEOUT_SECS if the DB needs longer to start.");
                    std::process::exit(1);
                }
            };

            // 连接拿到后再执行迁移主体（咨询锁 + 建表 + 应用迁移）。
            if let Err(e) = db::migrate::run_on_conn(&mut conn).await {
                tracing::error!("database migration failed: {e}");
                eprintln!("ERROR: database migration failed: {e}");
                eprintln!("HINT: check the logs above; verify DATABASE_URL and that PostgreSQL is healthy.");
                std::process::exit(1);
            }

            // BACKUP_* 环境变量播种自动备份初始配置（仅键缺失时生效，之后以面板为准）。
            // 播种失败不阻断启动——面板默认值仍可用。
            if let Err(e) = crate::api::settings::seed_backup_settings_from_env(&conn).await {
                tracing::warn!("backup settings env seeding failed: {e:?}");
            }

            // UPLOAD_CONCURRENCY 环境变量播种素材上传并发数（语义同上）。
            if let Err(e) = crate::api::settings::seed_upload_settings_from_env(&conn).await {
                tracing::warn!("upload settings env seeding failed: {e:?}");
            }

            // ADMIN_* 环境变量同步初始管理员（env 为凭据源：每次启动覆盖密码、
            // 确保 admin 角色，可免去首次注册）。失败只告警，不阻断启动。
            if let Err(e) = crate::api::auth::sync_admin_from_env(&conn).await {
                tracing::warn!("初始管理员 env 同步失败: {e:?}");
            }

            // 端口预探测：dioxus::server::serve() 内部对
            // `TcpListener::bind(addr).await...unwrap()`（dioxus-server 0.7.10 launch.rs:143）
            // 失败会直接 panic（SIGABRT）。这里在交接给 serve() 之前先探测同一地址，
            // 失败则走统一的 exit(1) 路径，输出可操作的提示，而不是丢一个裸 panic 栈。
            // 探测用的 listener 立即 drop，由 serve() 重新绑定（同进程内快速 rebind，
            // 不经过 TIME_WAIT，无窗口问题）。
            let addr = dioxus::cli_config::fullstack_address_or_localhost();
            if let Err(e) = tokio::net::TcpListener::bind(addr).await {
                tracing::error!("无法绑定监听地址 {addr}: {e}");
                eprintln!("ERROR: 无法绑定监听地址 {addr}: {e}");
                eprintln!(
                    "HINT: 端口 {} 可能已被占用。用 `lsof -i :{}` 查看占用进程，\
                     或设置 PORT 环境变量换一个端口。",
                    addr.port(),
                    addr.port()
                );
                std::process::exit(1);
            }
        });
        // 迁移 runtime 用完即弃，显式 drop 以在 serve() 前释放其线程资源。
        drop(migrate_rt);

        // 启动 Dioxus 服务端，返回构建好的 Axum Router
        dioxus::server::serve(|| async move {
            use axum::http::StatusCode;
            use dioxus::server::{axum, DioxusRouterExt, ServeConfig};
            use std::time::Duration;
            use tower_http::timeout::TimeoutLayer;

            // 启动后台定时任务：IP 信息清理
            tokio::spawn(async {
                tasks::ip_purge::run_purge().await;
            });

            // 启动后台定时任务：过期 session 清理
            tokio::spawn(async {
                tasks::session_cleanup::run_cleanup().await;
            });

            // 启动后台定时任务：回收站自动清理
            tokio::spawn(async {
                tasks::post_purge::run_purge().await;
            });

            // 启动后台定时任务：每天定点自动备份（设置存 DB，面板可改）
            tokio::spawn(async {
                tasks::backup::run_scheduler().await;
            });

            // 启动后台定时任务：图片磁盘缓存清理
            tokio::spawn(async {
                tasks::image_cache_cleanup::run_cleanup().await;
            });

            // 启动后台采样任务：sysinfo 主机指标（CPU/内存/磁盘），server function 只读快照。
            sysinfo_sampler::spawn_sampler();

            // 启动期探测代码运行器就绪度（Docker daemon + runner 镜像），缺失时打印可操作日志。
            // 不阻塞启动、不 exit——代码运行是可选功能（博客本身不依赖 Docker）。
            tokio::spawn(async {
                crate::api::code_runner::readiness::log_runner_readiness().await;
            });

            // 配置增量渲染缓存，默认缓存 3600 秒，可通过 SSR_CACHE_SECS 覆盖。
            // 注意：src/ssr_cache.rs 中的世代号是未来就绪基础设施，当前并不会使
            // Dioxus 0.7 的 SSR 缓存实际失效（Dioxus 未暴露相应 API）。在 API 可用
            // 之前，SSR_CACHE_SECS 仍是唯一有效的兜底 TTL——它就是内容写入后
            // SSR 页面可见滞后的上界。
            let ssr_cache_secs = std::env::var("SSR_CACHE_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3600);
            tracing::info!(
                ssr_cache_secs,
                "增量渲染缓存生效（写入后内容可见滞后的上界）；\
                 调小可缩短滞后，代价是 SSR 重渲染更频繁"
            );
            let config = ServeConfig::builder().incremental(
                dioxus::server::IncrementalRendererConfig::default()
                    .invalidate_after(std::time::Duration::from_secs(ssr_cache_secs)),
            );

            // 版本响应头开关：默认开启。设 0/false/no 关闭（注重安全、不想对外暴露版本/commit 时）。
            // bool 解析约定与 COOKIE_SECURE 一致(matches "1"/"true"/"yes")；这里取反为
            // "非 false 值即开"，使默认行为(unwrap_or(true))对应「暴露」。
            let expose_version_headers = std::env::var("EXPOSE_VERSION_HEADERS")
                .ok()
                .map(|v| !matches!(v.as_str(), "0" | "false" | "no"))
                .unwrap_or(true);
            tracing::info!(
                expose_version_headers,
                "版本响应头开关(Server / X-Yggdrasil-Version / X-Yggdrasil-Git / X-Yggdrasil-Hash)"
            );

            // 自定义 API 路由：图片上传（大文件，需要更长超时）
            // CSRF 校验置于最外层，先拦截非法来源再做超时/限体。
            let upload_route = axum::Router::new()
                .route(
                    "/api/upload",
                    axum::routing::post(crate::api::upload::upload_image),
                )
                .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024))
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    Duration::from_secs(300),
                ))
                .layer(axum::middleware::from_fn(crate::api::csrf::csrf_middleware));

            // MCP bearer 上传端点（带外二进制传输）：bearer 鉴权在 handler 内部完成，
            // 不挂 CSRF（bearer 在请求头，浏览器不自动附带，无 CSRF 风险）。
            // 10MiB body + 300s 超时与 web 上传一致；二进制不经 JSON-RPC，绕开
            // rmcp 的 4MiB 请求体上限。token-keyed 限流在 handler 内 check。
            let mcp_upload_route = axum::Router::new()
                .route(
                    "/api/mcp/upload",
                    axum::routing::post(crate::api::upload::mcp_upload_image),
                )
                .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024))
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    Duration::from_secs(300),
                ));

            // 数据导出：流式响应，走 GET + query（参数较短）。
            // 鉴权在 handler 内部从 cookie 校验 admin；CSRF 最外层拦截非法来源。
            let export_route = axum::Router::new()
                .route(
                    "/api/database/export",
                    axum::routing::get(crate::api::database::export::export_data),
                )
                // 备份下载：admin 鉴权 + 路径白名单（backups/ 不直接暴露静态目录）
                .route(
                    "/api/database/backups/{filename}",
                    axum::routing::get(crate::api::database::backup::download_backup),
                )
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    Duration::from_secs(120),
                ))
                .layer(axum::middleware::from_fn(crate::api::csrf::csrf_middleware));

            // SSE 流式输出端点：GET /api/exec/stream?task_id=X
            // 不挂 TimeoutLayer！SSE 是长连接，30s timeout 会杀掉流。
            // 鉴权 + 限流已在 start_exec_stream server function 完成（校验链），
            // 此处只校验 task_id 存在；CSRF 对 GET 放行（is_write_method 返回 false）。
            // CompressionLayer 跳过 text/event-stream（见 compression_layer_from_env 注释），
            // 但 sse_route 本身不挂 compression，更安全。
            let sse_route = axum::Router::new()
                .route(
                    "/api/exec/stream",
                    axum::routing::get(crate::api::code_runner::sse::exec_stream),
                )
                .layer(axum::middleware::from_fn(crate::api::csrf::csrf_middleware));

            // Dioxus 应用路由：自动挂载所有 server function 并渲染前端组件
            let dioxus_app =
                axum::Router::new().serve_dioxus_application(config, router::AppRouter);

            // 合并 Dioxus + CSRF/世代号/缓存头/可选压缩/30s 超时中间件
            // layer 顺序：后加的最外层先执行。CSRF 最外层先拦截非法来源。
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
                Duration::from_secs(30),
            ));
            // admin_guard 置于最外层（最后添加 = 最先执行）：未登录的 /admin* 请求
            // 在 CSRF / cache / SSR 渲染之前就被 302 短路，零渲染开销。
            let app_routes =
                app_routes.layer(axum::middleware::from_fn(crate::middleware::admin_guard));

            // 静态资源路由：图片文件服务。
            // 注意：`dioxus::server::serve()` 接管了 listener 与 `into_make_service`
            // 调用，没有机会换成 `into_make_service_with_connect_info::<SocketAddr>()`，
            // 所以手动 merge 进来的路由（含 static_routes）拿不到 `ConnectInfo` 扩展。
            // serve_image / upload_image 因此都用 `Option<Extension<ConnectInfo<SocketAddr>>>`
            // 优雅降级。生产环境应在反向代理后部署并配置 TRUSTED_PROXY_COUNT，
            // 使限流能拿到真实客户端 IP。
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
                .merge(export_route)
                .merge(sse_route)
                .merge(app_routes)
                .merge(static_routes)
                .merge(crate::mcp::router::mcp_route());

            // 版本头中间件置于最终合并 router 的最外层：所有端点（含 /healthz、/uploads/*、
            // 被 CSRF 拒/超时/admin_guard 重定向的响应）都会带上版本头。受 EXPOSE_VERSION_HEADERS 控制。
            let router = if expose_version_headers {
                router.layer(axum::middleware::from_fn(
                    crate::middleware::version_headers_middleware,
                ))
            } else {
                router
            };

            Ok(router)
        });
    }

    // 非 server feature（通常为 WASM 前端）：启动客户端应用
    #[cfg(not(feature = "server"))]
    {
        use router::AppRouter;
        dioxus::launch(AppRouter);
    }
}
