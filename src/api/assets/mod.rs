//! 素材管理 API 模块。
//!
//! 管理 `uploads/` 图片的注册表（assets 表）与引用关系（asset_refs 表）：
//! 分页列表、删除保护、孤儿清理、全量重建索引。
//! 全部为 Dioxus server function，仅 admin 可用。
// 与 posts / comments 模块一致：Dioxus `#[server]` 宏触发 deprecated/unit 提示，按项目惯例放行。
#![allow(clippy::unused_unit, deprecated, unused_imports)]

/// 评论引用判定 SQL 片段（表别名固定为 `a`，即 assets）。
///
/// 「素材仍被使用」有两个来源：文章引用（asset_refs 表，重建索引维护）与
/// 评论引用（评论区允许传图后，`<img src="/uploads/<path>">` 直接出现在
/// comments.content_html 里，无单独引用表）。存活评论（未进回收站）的
/// content_html 命中素材相对路径即视为引用——删除保护、孤儿判定、自动清理
/// 三处共用此片段，防止评论图被误判孤儿后删除。
///
/// 路径形如 `2026/08/19/153000.<uuid>.webp`，不含 LIKE 通配符（%/_），
/// 直接拼 `'%' || a.path || '%'` 安全；`?w=` 等查询串不影响子串匹配。
#[cfg(feature = "server")]
pub(crate) const COMMENT_REF_CLAUSE: &str = "EXISTS (SELECT 1 FROM comments c \
     WHERE c.deleted_at IS NULL AND c.content_html LIKE '%' || a.path || '%')";

/// 素材删除与孤儿清理。
pub mod delete;
/// 素材分页列表。
pub mod list;
/// 素材索引全量重建。
pub mod rebuild;
/// 请求与响应数据结构。
pub mod types;

pub use delete::{batch_delete_assets, delete_asset, purge_orphan_assets, update_asset_alt};
pub use list::list_assets;
pub use rebuild::rebuild_assets_index;
pub use types::{
    AssetListResponse, AssetOpResponse, BatchDeleteAssetsResponse, PurgeOrphansResponse,
    RebuildAssetsResponse,
};
