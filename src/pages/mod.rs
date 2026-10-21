//! 前端路由页面模块。
//!
//! 汇总所有路由对应的全栈页面组件，由 [`crate::router`] 统一挂载。
//! `admin/` 子目录为后台管理页面（需鉴权），其余为公开访问页面。
//!
//! reader 客户端（`--no-default-features --features web`，无 `client-full`）
//! 不编译后台、登录、注册与组件图鉴四个模块树——它们只被 full 客户端的
//! Route 变体引用（plan §6.1）。模块声明处门控保证整棵树不进编译单元。

// 以下四个模块是 full 客户端专属：后台管理、登录、注册与组件图鉴。
pub mod about;
#[cfg(not(ygg_client_reader))]
pub mod admin;
pub mod archives;
pub mod changelog;
#[cfg(not(ygg_client_reader))]
pub mod components_showcase;
pub mod friends;
pub mod home;
#[cfg(not(ygg_client_reader))]
pub mod login;
pub mod not_found;
pub mod notes;
pub mod post_detail;
#[cfg(not(ygg_client_reader))]
pub mod register;
pub mod search;
pub mod tags;
pub mod writing_guide;
