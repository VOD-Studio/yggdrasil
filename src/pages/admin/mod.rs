// MIGRATION_STUB
//! 管理后台页面模块。
//!
//! `AdminShellRoute` 是 `/admin` 的懒加载父路由，视图渲染后台外壳并包住子路由出口。

use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::{lazy_route, LazyRoute};

pub mod assets;
pub mod comments;
pub mod dashboard;
pub mod friends;
pub mod logs;
pub mod mcp;
mod note_drafts;
pub mod note_editor;
pub mod notes;
pub mod posts;
pub mod posts_trash;
pub mod preview;
pub mod profile;
pub mod runner;
pub mod settings;
pub mod system;
pub mod write;

pub struct AdminShellRoute;

#[lazy_route]
impl LazyRoute for AdminShellRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <Outlet/> }.into_any()
    }
}
