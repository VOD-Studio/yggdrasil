// 服务端 bin 同样单态化完整视图类型，与 lib.rs 一致放宽查询深度。
#![recursion_limit = "256"]

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use std::path::PathBuf;

    use axum::body::Body;
    use axum::http::Request;
    use axum::Router;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use ygg_leptos_spike::app::{shell, App};
    use ygg_leptos_spike::server::{ssr_cache, static_files};

    let conf = get_configuration(None).expect("读取 Leptos 配置失败");
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    async fn fallback(
        axum::extract::State(options): axum::extract::State<LeptosOptions>,
        req: Request<Body>,
    ) -> axum::response::Response {
        let site_root = PathBuf::from(options.site_root.as_ref());
        if let Some(resp) =
            static_files::serve(req.method(), req.uri(), req.headers(), &site_root).await
        {
            return resp;
        }
        // 未命中静态资源：交给 Leptos 渲染（NotFound 路由会设置 404）。
        let handler = leptos_axum::render_app_to_stream(move || shell(options.clone()));
        handler(req).await
    }
    let app = Router::new()
        .leptos_routes(&leptos_options, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(fallback)
        // 内层：分享元信息上下文（站点 base URL 等），外层：匿名 HTML 缓存。
        .layer(axum::middleware::from_fn(
            ygg_leptos_spike::components::share_meta::insert_share_context,
        ))
        .layer(axum::middleware::from_fn(ssr_cache::cache_public_html))
        .with_state(leptos_options);

    println!("spike listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("端口绑定失败");
    axum::serve(listener, app.into_make_service())
        .await
        .expect("服务异常退出");
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
