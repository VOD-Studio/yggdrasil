//! libs/ 下各 JS IIFE 库的 wasm-bindgen 桥接层。
//!
//! 每个子模块对应一个 IIFE 产物（挂在 `window` 上的对象字面量）：
//! 共享纯数据类型两端编译，wasm-bindgen extern 与 `EditorHandle`/`TerminalHandle`
//! 仅在 `#[cfg(target_arch = "wasm32")]` 子模块里编译（server 构建无 window）。

/// CodeMirror SQL/代码编辑器桥接：共享类型（SqlSchema/SqlTable）两端都编译；
/// extern 与 EditorHandle 在 #[cfg(wasm32)] 子模块里。
pub mod codemirror;
/// Tiptap 富文本编辑器桥接：共享类型（UploadsInFlight/UploadErrorEntry）两端都编译；
/// wasm-bindgen extern 与 EditorHandle 在内部的 #[cfg(wasm32)] 子模块里。
pub mod tiptap;
/// xterm.js 终端桥接：结构镜像 codemirror。
pub mod xterm;
