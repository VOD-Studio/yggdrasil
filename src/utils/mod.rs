//! 通用工具函数子模块。
//!
//! - `comment_storage`：评论草稿 localStorage 持久化（WASM 端）。
//! - `html`：HTML 转义（两端通用）。
//! - `js`：WASM 端调用 `window.__init*` 可选全局函数（仅 wasm32）。
//! - `text`：Markdown / 纯文本处理（仅 `server` feature）。
//! - `time`：跨平台时间/睡眠工具（WASM 与原生异步版本）。
//! - `web_upload`：multipart 文件上传 fetch 助手（仅 wasm32）。

/// 评论草稿 localStorage 持久化（仅在 WASM 端实际读写）。
pub mod comment_storage;
/// HTML 转义工具（前端后端通用）。
pub mod html;
/// WASM 端 JS 全局函数调用工具（仅 wasm32 编译）。
#[cfg(target_arch = "wasm32")]
pub mod js;
/// 服务端共享常量与工具（hash、正则、上限）。
#[cfg(feature = "server")]
pub mod server;
/// Markdown / 纯文本处理工具。
#[cfg(feature = "server")]
pub mod text;
/// 跨平台时间/睡眠工具。
pub mod time;
/// WASM 端 multipart 上传助手（仅 wasm32 编译）。
#[cfg(target_arch = "wasm32")]
pub mod web_upload;
