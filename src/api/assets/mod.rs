//! 素材管理 API 模块。
//!
//! 管理 `uploads/` 图片的注册表（assets 表）与引用关系（asset_refs 表）：
//! 分页列表、删除保护、孤儿清理、全量重建索引。
//! 全部为 Leptos server function，仅 admin 可用。
// 与 posts / comments 模块一致：Leptos `#[server]` 宏触发 deprecated/unit 提示，按项目惯例放行。
#![allow(clippy::unused_unit, deprecated, unused_imports)]

/// 素材引用判定 SQL 片段（表别名固定为 `a`，即 assets）。
///
/// 五类引用都是外键引用表：文章（`asset_refs`）、笔记（`note_asset_refs`）、
/// 评论（`comment_asset_refs`，含回收站评论）、用户头像与友链头像。
#[cfg(feature = "server")]
pub(crate) const ASSET_REF_CLAUSE: &str =
    "(EXISTS (SELECT 1 FROM asset_refs r WHERE r.asset_id = a.id) \
     OR EXISTS (SELECT 1 FROM note_asset_refs r WHERE r.asset_id = a.id) \
     OR EXISTS (SELECT 1 FROM comment_asset_refs r WHERE r.asset_id = a.id) \
     OR EXISTS (SELECT 1 FROM user_avatar_asset_refs r WHERE r.asset_id = a.id) \
     OR EXISTS (SELECT 1 FROM friend_avatar_asset_refs r WHERE r.asset_id = a.id))";

/// 评论引用判定（含回收站评论），供单删接口返回更具体的业务提示。
#[cfg(feature = "server")]
pub(crate) const COMMENT_REF_CLAUSE: &str =
    "EXISTS (SELECT 1 FROM comment_asset_refs r WHERE r.asset_id = a.id)";

/// 用户头像与友链头像引用判定，供单删接口区分评论引用。
#[cfg(feature = "server")]
pub(crate) const AVATAR_REF_CLAUSE: &str =
    "(EXISTS (SELECT 1 FROM user_avatar_asset_refs r WHERE r.asset_id = a.id) \
     OR EXISTS (SELECT 1 FROM friend_avatar_asset_refs r WHERE r.asset_id = a.id))";

/// 素材删除与孤儿清理。
pub mod delete;
/// 素材分页列表。
pub mod list;
/// 素材索引全量重建。
pub mod rebuild;
/// 评论与头像的素材引用登记（写入时锁定并核对素材）。
#[cfg(feature = "server")]
pub(crate) mod refs;
/// 请求与响应数据结构。
pub mod types;

pub use delete::{batch_delete_assets, delete_asset, purge_orphan_assets, update_asset_alt};
pub use list::list_assets;
pub use rebuild::rebuild_assets_index;
pub use types::{
    AssetListResponse, AssetOpResponse, BatchDeleteAssetsResponse, PurgeOrphansResponse,
    RebuildAssetsResponse,
};
