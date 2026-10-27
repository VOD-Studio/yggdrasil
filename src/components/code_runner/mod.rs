//! 代码运行器（Code Runner）组件。
//!
//! 提供可运行代码块的交互 UI：显示源码、运行按钮、运行阶段、输出区。
//! 点击 Run 时调用 `start_exec_stream` server function 提交，经 SSE（或轮询
//! `get_exec_result`）取回结果直到终态。
//!
//! CodeMirror 编辑器实例由组件在 WASM 端按容器 id 挂载，激活后的编辑器、终端与执行逻辑
//! 位于分包函数 [`CodeRunner`][crate::components::code_runner::CodeRunner] 所在模块的 `code_runner_body` 中；静态代码块与
//! 激活判断留在调用方所在的包。

pub mod runner;
pub use runner::CodeRunner;
