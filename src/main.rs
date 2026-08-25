//! 程序入口与目标选择。
//!
//! 服务端启动流程与 Axum 路由组装位于 [`startup`] 模块；本文件只负责
//! 双目标入口和服务端专用的全局内存分配器。未启用 `server` feature 时，
//! 直接启动 WASM 前端。
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
mod config;
mod context;
mod db;
pub mod infra;
// highlight 模块仅在服务端构建时编译
#[cfg(feature = "server")]
mod highlight;
// middleware：Axum 中间件与启动期纯函数（cache-control / admin 守卫 / 压缩层），
// server-only。从 startup.rs 抽出以便独立测试，路由组装处以 crate::middleware::xxx 调用。
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
// ssr_cache 仅在 server feature 启用时编译；保存 SSR 世代号失效状态。
#[cfg(feature = "server")]
mod ssr_cache;
#[cfg(feature = "server")]
mod startup;

mod tasks;
mod theme;
// bridges：libs/ 下各 JS IIFE（tiptap / codemirror / xterm）的 wasm-bindgen 桥接层。
mod bridges;
mod utils;

/// 程序入口
fn main() {
    #[cfg(feature = "server")]
    startup::run();

    #[cfg(not(feature = "server"))]
    {
        use router::AppRouter;
        dioxus::launch(AppRouter);
    }
}
