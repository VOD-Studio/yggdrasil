//! 前端路由页面模块。
//!
//! 每个路由入口文件定义一个或多个 `LazyRoute` 类型，由 [`crate::app`] 的路由表以
//! `Lazy::<T>` 挂载（`cargo leptos --split` 下按路由拆分 WASM 分包）。
//! `admin/` 子目录为后台管理页面（需鉴权），其余为公开访问页面。

pub mod about;
pub mod admin;
pub mod archives;
pub mod changelog;
pub mod components_showcase;
pub mod friends;
pub mod home;
pub mod login;
pub mod not_found;
pub mod notes;
pub mod post_detail;
pub mod register;
pub mod search;
pub mod tags;
pub mod writing_guide;
