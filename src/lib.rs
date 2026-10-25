#![recursion_limit = "256"]
#![doc(html_favicon_url = "/favicon.svg")]
//! Yggdrasil 库目标：承载全部业务模块。
//!
//! 服务端二进制（`src/main.rs`）调用 [`startup::run`]；浏览器 WASM 由 cargo-leptos
//! 以 `hydrate` feature 编译本库，入口是文件末尾的 `hydrate()`。
//! `server` 与 `hydrate` 互斥，不要同时开启。

// 业务模块
pub mod api;
pub mod app;
pub mod auth;
pub mod bridges;
// build_info:编译期注入的 git/rustc/构建时间信息。模块内部 gate 在 server feature,
// 模块声明本身不需要再加 cfg(空模块在 WASM 端也能编译)。
pub mod build_info;
pub mod cache;
pub mod components;
pub mod config;
pub mod context;
pub mod db;
// highlight 模块仅在服务端构建时编译
#[cfg(feature = "server")]
pub mod highlight;
pub mod hooks;
pub mod infra;
// mcp：Model Context Protocol 服务器（/mcp Streamable HTTP，bearer token 鉴权）。
// 仅 server feature 编译；WASM 前端不引用任何 mcp 符号。
#[cfg(feature = "server")]
pub mod mcp;
// middleware：Axum 中间件与启动期纯函数（cache-control / admin 守卫 / 压缩层），
// server-only。从 startup.rs 抽出以便独立测试，路由组装处以 crate::middleware::xxx 调用。
#[cfg(feature = "server")]
pub mod middleware;
pub mod models;
pub mod navigation;
// og：分享图（og:image）渲染，字体随二进制嵌入，server-only。
#[cfg(feature = "server")]
pub mod og;
pub mod pages;
pub mod request_ctx;
pub mod routes;
// ssr_cache 仅在 server feature 启用时编译；保存 SSR 世代号失效状态。
#[cfg(feature = "server")]
pub mod ssr_cache;
#[cfg(feature = "server")]
pub mod startup;
pub mod tasks;
pub mod theme;
pub mod utils;

/// 浏览器端入口：懒加载路由的水合。
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    leptos::mount::hydrate_lazy(crate::app::App);
}

/// `--split` 的数据段腾挪余量。
///
/// wasm-split 会把只被某个分包引用的只读数据（页面文案、class 名、fixture 字符串）从主包的
/// `.rodata` 挪进那个分包；但重新排布后每个分包的数据要按段对齐，总长度若超过输入 `.rodata`
/// 哪怕几百字节，工具就放弃腾挪、把整段留在主包（实测超出 735 B，主包因此多背约 95 KB
/// brotli 的后台/图鉴文案）。这个静态量保留在 `.rodata` 里却没有任何代码引用，不会被输出到
/// 任何模块，等于给重新排布留出 16 KiB 余量。删掉它，`make test-release-split` 的体积门槛
/// 会失败。
#[cfg(feature = "hydrate")]
#[used]
static SPLIT_DATA_SLACK: [u8; 16384] = [0xA5; 16384];
