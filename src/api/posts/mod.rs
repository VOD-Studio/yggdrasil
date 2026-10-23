//! 文章模块：提供文章的 CRUD、列表、搜索、标签聚合与统计等 server function。
//!
//! 所有 Leptos server function 均注册在 `/api` 路径下，供前端与服务端调用。
//! 仅在 `feature = "server"` 启用的服务端构建中执行数据库操作与缓存失效。

#![allow(clippy::unused_unit, deprecated, unused_imports)]

mod archive;
mod batch;
mod create;
mod delete;
pub(crate) mod helpers;
mod list;
mod outline;
mod read;
mod rebuild;
mod roundtrip;
mod search;
#[cfg(feature = "server")]
pub(crate) mod service;
mod stats;
mod suggest;
mod tags;
mod trash;
mod types;
mod update;
#[cfg(feature = "server")]
mod validation;

pub use archive::{
    list_archive_posts, posts_per_year, ArchiveResponse, YearCount, ARCHIVE_PAGE_SIZE,
};
/// 后台列表批量操作（转为草稿、移到回收站）。
pub use batch::batch_posts;
/// 创建新文章。
pub use create::create_post;
/// 删除指定文章。
pub use delete::delete_post;
/// 获取回收站中已软删除的文章列表。
pub use list::list_deleted_posts;
/// 获取管理员视角的全部文章分页列表。
pub use list::list_posts;
/// 获取已发布文章分页列表。
pub use list::{get_posts_by_tag, list_published_posts};
/// 首页最新一篇的目录预览。
pub use outline::latest_post_outline;
/// 根据 id 获取文章详情。
/// 根据 slug 获取已发布文章详情（公开）。
/// 按 slug 预览文章（管理员，含草稿）。
pub use read::{get_post_by_id, get_post_by_slug, get_post_preview, get_public_post_by_slug};
/// 重新渲染文章的 Markdown HTML 与目录。
pub use rebuild::rebuild_content_html;
/// 重新渲染指定文章的 Markdown HTML 与目录（单篇）。
pub use rebuild::rebuild_post_content_html;
/// 富文本编辑器往返渲染一致性检查。
pub use roundtrip::check_markdown_roundtrip;
/// 全文搜索已发布文章。
pub use search::search_posts;
/// 获取文章统计信息。
pub use stats::get_post_stats;
#[cfg(feature = "server")]
pub(crate) use stats::load_post_stats;
#[cfg(feature = "server")]
pub(crate) use suggest::query_suggestions;
/// 搜索面板（⌘K）的文章建议。
pub use suggest::{recommend_for_path, suggest_posts, PathRecommendations};
/// 获取全部标签及其文章数量。
pub use tags::list_tags;
/// 恢复已删除文章。
pub use trash::{batch_purge_posts, batch_restore_posts, empty_trash, purge_post, restore_post};
/// 文章 API 的请求与响应数据结构。
pub use types::*;
/// 更新指定文章。
pub use update::update_post;

#[cfg(feature = "server")]
pub(crate) use trash::restore_post_impl;
