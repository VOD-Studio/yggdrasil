//! libs/ 下各 JS IIFE 库的 wasm-bindgen 桥接层。
//!
//! 每个子模块对应一个 IIFE 产物（挂在 `window` 上的对象字面量）：
//! 共享纯数据类型两端编译，wasm-bindgen extern 与 `EditorHandle`/`TerminalHandle`
//! 仅在 `#[cfg(target_arch = "wasm32")]` 子模块里编译（服务端构建无 window）。
//!
//! 使用约定（Leptos）：
//! - 挂载点是带稳定 `id` 的 DOM 元素，`create(container_id, opts)` 按 id 查找；
//! - 先用 [`library::use_browser_library`] 等待库脚本就绪，再在 `Effect` 内构造 Options 并 `create`；
//! - 句柄（`EditorHandle`/`TerminalHandle`）含 JS 引用，不是 `Send + Sync`，
//!   必须放进 `StoredValue::new_local`/`RwSignal::new_local`；句柄 `Drop` 时销毁 JS 实例，
//!   组件卸载时在 `on_cleanup` 里 `take` 出来丢弃即可；
//! - 回调以 wasm-bindgen `Closure` 注入，随句柄同生共死。

/// CodeMirror SQL/代码编辑器桥接：extern 与 EditorHandle 在 #[cfg(wasm32)] 子模块里。
/// SQL 补全 schema DTO（SqlSchema/SqlTable）是 server function 返回值，
/// 定义在 `crate::models::sql_schema`，不在本模块。
pub mod codemirror;
/// 浏览器库的按需加载与错误重试。
pub mod library;
/// 页面过渡与按历史记录保存的客户端状态。
pub mod navigation;
/// Tiptap 富文本编辑器桥接：共享类型（UploadsInFlight/UploadErrorEntry）两端都编译；
/// wasm-bindgen extern 与 EditorHandle 在内部的 #[cfg(wasm32)] 子模块里。
pub mod tiptap;
/// xterm.js 终端桥接：结构镜像 codemirror。
pub mod xterm;
