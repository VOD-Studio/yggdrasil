#![recursion_limit = "256"]
//! 服务端程序入口。
//!
//! 全部模块位于库目标（`src/lib.rs`），服务端启动流程与 Axum 路由组装见
//! [`yggdrasil::startup`]；本文件只负责服务端专用的全局内存分配器与入口调用。

// 全局内存分配器：mimalloc。
// 多线程高频小对象分配场景下吞吐显著优于系统 malloc，且对全静态 musl 链接友好。
// cfg 门控（与项目「双目标编译」约定一致）：
//   - feature = "server"：分配器只服务端二进制需要。
//   - not(wasm32)：mimalloc_rust 在 wasm32 上无法编译（mimalloc_rust Issue #76），
//     WASM 前端走默认分配器。两个门控同时满足才注册。
#[cfg(all(feature = "server", not(target_arch = "wasm32")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "server")]
fn main() {
    yggdrasil::startup::run()
}

#[cfg(not(feature = "server"))]
fn main() {}
