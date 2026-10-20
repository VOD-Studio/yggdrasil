//! 通用 UI 原子组件与类名常量。
//!
//! 提供跨页面共享的样式常量（卡片、按钮、徽章外层等）与可复用组件
//! （分页导航、状态徽章、空状态）。样式常量用于消除散落在各页面的重复
//! Tailwind 类字符串；组件用于封装结构固定的 UI 单元。
//!
//! 与 `forms.rs`（表单控件）并列，本模块聚焦通用展示类原子。

mod controls;
mod display;
mod feedback;
mod icons;
mod menu;
mod navigation;
mod sheet;
mod styles;
mod surfaces;

pub use controls::*;
pub use display::*;
pub use feedback::*;
pub use icons::*;
pub use menu::*;
pub use navigation::*;
pub use sheet::*;
pub use styles::*;
pub use surfaces::*;
