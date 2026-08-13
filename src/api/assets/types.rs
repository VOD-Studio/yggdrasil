//! 素材管理 API 的请求与响应数据结构。

use serde::{Deserialize, Serialize};

use crate::models::asset::AssetDto;

/// 素材分页列表响应。
///
/// 附带各筛选维度的计数（tabs 展示）与可清理孤儿的统计（清理按钮展示），
/// 避免前端为徽标数字额外发请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetListResponse {
    pub assets: Vec<AssetDto>,
    /// 当前筛选条件下的总数（分页用）。
    pub total: i64,
    /// 全部被引用素材数（「引用中」tab）。
    pub used_count: i64,
    /// 全部无引用素材数（「孤儿」tab，含 7 天保护窗内的）。
    pub orphan_count: i64,
    /// 可一键清理的孤儿数（无引用且 created_at 早于 7 天前）。
    pub purgeable_count: i64,
    /// 可清理孤儿的总字节数。
    pub purgeable_bytes: i64,
}

/// 通用素材操作响应（删除/清理/重建共用）。
///
/// 业务拒绝（如引用中禁删）走 `Ok(success:false)`，遵循仓库约定不走 Err。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetOpResponse {
    pub success: bool,
    pub message: String,
    /// 删除被拦截时的引用文章列表（post_id, title）。
    pub refs: Vec<crate::models::asset::AssetRef>,
}

#[cfg(any(feature = "server", test))]
impl AssetOpResponse {
    pub fn ok(message: String) -> Self {
        Self {
            success: true,
            message,
            refs: Vec::new(),
        }
    }

    pub fn err(message: String) -> Self {
        Self {
            success: false,
            message,
            refs: Vec::new(),
        }
    }
}

/// 一键清理孤儿的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgeOrphansResponse {
    pub success: bool,
    pub message: String,
    pub deleted_count: i64,
    pub freed_bytes: i64,
    /// 删文件失败但 DB 行已删的素材数（文件可能已不存在，属可容忍不一致）。
    pub failures: i64,
}

/// 批量删除的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDeleteAssetsResponse {
    pub success: bool,
    pub message: String,
    pub deleted_count: i64,
    /// 因被文章引用而跳过的数量（保护语义与单删一致）。
    pub skipped_referenced: i64,
    pub freed_bytes: i64,
    /// id 非法或删文件失败的数量（DB 行已删/未处理，属可容忍不一致）。
    pub failures: i64,
}

/// 重建索引的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuildAssetsResponse {
    pub success: bool,
    pub message: String,
    /// 磁盘扫描到的图片文件数。
    pub scanned: i64,
    /// 新登记进 assets 的数量。
    pub inserted: i64,
    /// 已存在并更新技术字段的数量。
    pub updated: i64,
    /// 文件已消失而删除的 DB 行数。
    pub removed: i64,
    /// 重建后的引用关联总数。
    pub ref_count: i64,
}
