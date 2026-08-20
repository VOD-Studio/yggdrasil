//! 素材管理 API 模块。
//!
//! 管理 `uploads/` 图片的注册表（assets 表）与引用关系（asset_refs 表）：
//! 分页列表、删除保护、孤儿清理、全量重建索引。
//! 全部为 Dioxus server function，仅 admin 可用。
// 与 posts / comments 模块一致：Dioxus `#[server]` 宏触发 deprecated/unit 提示，按项目惯例放行。
#![allow(clippy::unused_unit, deprecated, unused_imports)]

/// 素材引用判定 SQL 片段（表别名固定为 `a`，即 assets）。
///
/// 引用来源包括文章关联（`asset_refs`）、存活评论 HTML、用户头像和友链头像。
/// 文章关联由索引重建维护；后三者保存的是直接 URL，因此在查询时按素材路径判断。
#[cfg(feature = "server")]
pub(crate) const ASSET_REF_CLAUSE: &str =
    "(EXISTS (SELECT 1 FROM asset_refs r WHERE r.asset_id = a.id) \
     OR EXISTS (SELECT 1 FROM comments c \
               WHERE c.deleted_at IS NULL AND c.content_html LIKE '%' || a.path || '%') \
     OR EXISTS (SELECT 1 FROM users u WHERE u.avatar_url = '/uploads/' || a.path) \
     OR EXISTS (SELECT 1 FROM friend_links f WHERE f.avatar_url = '/uploads/' || a.path))";

/// 存活评论引用判定，供单删接口返回更具体的业务提示。
#[cfg(feature = "server")]
pub(crate) const COMMENT_REF_CLAUSE: &str = "EXISTS (SELECT 1 FROM comments c \
     WHERE c.deleted_at IS NULL AND c.content_html LIKE '%' || a.path || '%')";

/// 直接头像引用判定，供单删接口区分评论引用。
#[cfg(feature = "server")]
pub(crate) const AVATAR_REF_CLAUSE: &str =
    "(EXISTS (SELECT 1 FROM users u WHERE u.avatar_url = '/uploads/' || a.path) \
     OR EXISTS (SELECT 1 FROM friend_links f WHERE f.avatar_url = '/uploads/' || a.path))";

/// 列表页引用计数表达式，按每个文章/评论/头像使用次数累计。
#[cfg(feature = "server")]
pub(crate) const ASSET_REF_COUNT_EXPR: &str =
    "((SELECT COUNT(*) FROM asset_refs r WHERE r.asset_id = a.id) + \
     (SELECT COUNT(*) FROM comments c WHERE c.deleted_at IS NULL \
      AND c.content_html LIKE '%' || a.path || '%') + \
     (SELECT COUNT(*) FROM users u WHERE u.avatar_url = '/uploads/' || a.path) + \
     (SELECT COUNT(*) FROM friend_links f WHERE f.avatar_url = '/uploads/' || a.path))";

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

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::ASSET_REF_CLAUSE;

    #[test]
    fn asset_reference_clause_protects_profile_avatar() {
        assert!(
            ASSET_REF_CLAUSE.contains("users u")
                && ASSET_REF_CLAUSE.contains("u.avatar_url = '/uploads/' || a.path"),
            "素材引用判定必须包含用户头像路径"
        );
    }
}
