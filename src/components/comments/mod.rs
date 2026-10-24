//! 评论组件模块
//!
//! 提供评论相关组件：评论列表、单条评论、待审核评论、评论表单与评论区段。

/// 评论卡片外壳组件（已审核/待审核评论共享布局骨架）。
pub mod card;
/// 评论表单组件。
pub mod form;
/// 单条评论组件。
pub mod item;
/// 评论列表组件。
pub mod list;
/// 待审核评论组件。
pub mod pending_item;
/// 评论区段组件。
pub mod section;
