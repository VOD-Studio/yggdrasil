//! HTML 外壳与路由表。
//!
//! 三个页面路由都用 `Lazy::<T>::new()`：路由数据（`LazyRoute::data`）在主包里
//! 同步创建资源，视图代码在 `cargo leptos --split` 下拆进独立 WASM 分包，
//! 导航时与数据并发加载。前台布局（页头、页脚、主题）留在主包。

use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Link, MetaTags, Title};
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::{path, Lazy, SsrMode};

use crate::components::frontend_layout::FrontendLayout;
use crate::pages::admin_comments::AdminCommentsRoute;
use crate::pages::home::HomeRoute;
use crate::pages::not_found::NotFound;
use crate::pages::post_detail::PostDetailRoute;
use crate::theme::{provide_theme, ThemePreload};

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="spike-build" content=crate::BUILD_ID/>
                // leptos_meta::Meta 不支持 media 属性，静态写进外壳。
                <meta name="theme-color" content="#f5f5f3" media="(prefers-color-scheme: light)"/>
                <meta name="theme-color" content="#0f0f0e" media="(prefers-color-scheme: dark)"/>
                // 与 Dioxus.toml [web.resource] 一致的全局样式与启动脚本。
                <link rel="stylesheet" href="/style.css?v=5"/>
                <link rel="stylesheet" href="/highlight.css?v=2"/>
                <link rel="stylesheet" href="/katex/katex.min.css?v=3"/>
                <link rel="stylesheet" href="/yggdrasil-core/yggdrasil-core.css?v=3"/>
                <script src="/yggdrasil-core/yggdrasil-core.js?v=4"></script>
                <ThemePreload/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    provide_theme();

    view! {
        <Title text="Yggdrasil Blog"/>
        <Link rel="icon" href="/favicon.svg" type_="image/svg+xml"/>
        <Link rel="icon" href="/favicon.ico" sizes="16x16 32x32 48x48"/>
        <Link rel="apple-touch-icon" href="/apple-touch-icon.png"/>
        <Link rel="manifest" href="/manifest.webmanifest"/>
        <Router>
            <Routes fallback=|| view! { <NotFound/> }>
                <ParentRoute path=path!("") view=FrontendLayout>
                    <Route path=path!("") view={Lazy::<HomeRoute>::new()}/>
                    <Route path=path!("page/:page") view={Lazy::<HomeRoute>::new()}/>
                    // Async：等文章资源就绪再输出，<Title>/<Meta og:*> 与 404 状态码才能进入 SSR 头部。
                    <Route
                        path=path!("post/:slug")
                        view={Lazy::<PostDetailRoute>::new()}
                        ssr=SsrMode::Async
                    />
                </ParentRoute>
                <Route
                    path=path!("admin/comments")
                    view={Lazy::<AdminCommentsRoute>::new()}
                    ssr=SsrMode::Async
                />
            </Routes>
        </Router>
    }
}
