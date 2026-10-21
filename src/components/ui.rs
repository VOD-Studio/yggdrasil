//! 通用 UI 原子组件与类名常量。
//!
//! 提供跨页面共享的样式常量（卡片、按钮、徽章外层等）与可复用组件
//! （分页导航、状态徽章、空状态）。样式常量用于消除散落在各页面的重复
//! Tailwind 类字符串；组件用于封装结构固定的 UI 单元。
//!
//! 与 `forms.rs`（表单控件）并列，本模块聚焦通用展示类原子。

// 分页/状态徽章等控制原子：仅后台页面与图鉴预览使用（plan §6.1），
// reader 客户端不编译。
#[cfg(not(ygg_client_reader))]
mod controls;
mod display;
mod navigation;
mod styles;
mod surfaces;

#[cfg(not(ygg_client_reader))]
pub use controls::*;
pub use display::*;
pub use navigation::*;
pub use styles::*;
pub use surfaces::*;
