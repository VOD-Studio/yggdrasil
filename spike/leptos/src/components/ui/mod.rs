//! 通用 UI 原子组件与类名常量。
//!
//! 提供跨页面共享的样式常量（徽章外层、按钮等）与可复用组件
//! （分页导航、状态徽章、输入栏、分段控件、抽屉）。原型只移植切片页面用到的部分。
//!
//! 与 `forms.rs`（表单控件）并列，本模块聚焦通用展示类原子。

mod composer;
mod controls;
mod display;
mod feedback;
mod icons;
mod navigation;
mod segmented;
mod sheet;
mod styles;

pub use composer::*;
pub use controls::*;
pub use display::*;
pub use feedback::*;
pub use icons::*;
pub use navigation::*;
pub use segmented::*;
pub use sheet::*;
pub use styles::*;
