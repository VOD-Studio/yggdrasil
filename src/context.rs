//! 前端全局上下文定义。
//!
//! 保存当前登录用户与组件图鉴筛选状态，供组件在客户端与服务端渲染期间共享访问。
//! 由 `crate::app::App` 通过 `provide_context` 提供，组件用 `expect_context` 读取。

use leptos::prelude::*;
use std::sync::Arc;

use crate::models::user::PublicUser;

/// 用户上下文，用于在组件树中传递登录状态。
#[derive(Clone, Copy)]
pub struct UserContext {
    /// 当前登录用户，未登录时为 `None`。
    pub user: RwSignal<Option<Arc<PublicUser>>>,
    /// 是否已完成会话校验，避免重复触发验证请求。
    pub checked: RwSignal<bool>,
}

impl UserContext {
    /// 创建未登录、未校验的初始上下文。
    pub fn new() -> Self {
        Self {
            user: RwSignal::new(None),
            checked: RwSignal::new(false),
        }
    }
}

impl Default for UserContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 组件图鉴的筛选状态：在图鉴列表与详情页之间保持，返回列表时恢复。
#[derive(Clone, Default, PartialEq)]
pub struct ShowcaseFilters {
    pub category: String,
    pub search: String,
    /// 宽屏详情栏当前展示的组件；`None` 时取筛选结果的第一项。
    pub selected: Option<String>,
    pub return_to: Option<String>,
}
