//! 前端路由页面模块。
//!
//! 汇总所有路由对应的全栈页面组件，由 [`crate::router`] 统一挂载。
//! `admin/` 子目录为后台管理页面（需鉴权），其余为公开访问页面。

pub mod about;
pub mod admin;
pub mod archives;
pub mod changelog;
pub mod friends;
pub mod home;
pub mod login;
pub mod not_found;
pub mod post_detail;
pub mod register;
pub mod search;
pub mod tags;
pub mod writing_guide;
