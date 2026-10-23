//! HTML 外壳与应用根组件。
//!
//! `shell()` 是服务端渲染的完整 HTML 文档；全局样式与启动脚本的资源版本号
//! （`?v=N`）在这里维护，修改对应静态文件后必须同步递增，浏览器才会重新拉取。

use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Title};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

/// 服务端渲染的 HTML 文档外壳。
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="theme-color" content="#f5f5f3" media="(prefers-color-scheme: light)"/>
                <meta name="theme-color" content="#0f0f0e" media="(prefers-color-scheme: dark)"/>
                <link rel="icon" href="/favicon.svg" type="image/svg+xml"/>
                <link rel="icon" href="/favicon.ico" sizes="16x16 32x32 48x48"/>
                <link rel="apple-touch-icon" href="/apple-touch-icon.png"/>
                <link rel="manifest" href="/manifest.webmanifest"/>
                <link rel="stylesheet" href="/style.css?v=5"/>
                <link rel="stylesheet" href="/highlight.css?v=2"/>
                <link rel="stylesheet" href="/katex/katex.min.css?v=3"/>
                <link rel="stylesheet" href="/yggdrasil-core/yggdrasil-core.css?v=3"/>
                <script src="/yggdrasil-core/yggdrasil-core.js?v=4"></script>
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

// MIGRATION_STUB
/// 应用根组件（迁移中的临时版本，Phase 2A 替换为完整路由表）。
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Title text="Yggdrasil Blog"/>
        <Router>
            <Routes fallback=|| "迁移中">
                <Route path=path!("__migration_stub") view=|| ()/>
            </Routes>
        </Router>
    }
}
