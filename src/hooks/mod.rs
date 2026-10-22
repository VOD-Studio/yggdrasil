//! 共享的 Dioxus Hooks 模块。
//!
//! 该模块集中管理可在组件树中复用的自定义 Hook，包括：
//! - 通用 DOM 事件监听（注册 + 自动卸载清理）
//! - 客户端分页数据加载
//!
//! 评论草稿持久化（localStorage 读写）已迁至 `crate::utils::comment_storage`，
//! 因为它是纯工具函数而非渲染期 Hook。
//!
//! 骨架屏延迟渲染已统一至 `crate::components::skeletons::delayed_skeleton::DelayedSkeleton` 组件。

/// 通用 DOM 事件监听 Hook（add/remove_event_listener 生命周期封装）。
pub mod event_listener;

/// 元素接近视口的单向可见性激活 Hook（use_near_viewport）。
pub mod near_viewport;

/// 客户端数据加载 Hook（use_paginated）。
pub mod query;

pub use near_viewport::use_near_viewport;
