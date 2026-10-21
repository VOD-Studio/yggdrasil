//! 评论 API 的请求与响应数据结构。

use crate::models::comment::{AdminComment, PublicComment};
use serde::{Deserialize, Serialize};

/// 创建/审核/删除评论的统一响应结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentResponse {
    /// 操作是否成功。
    pub success: bool,
    /// 提示信息。
    pub message: String,
    /// 错误码，成功时为 None。
    pub error_code: Option<String>,
    /// 新评论 id。
    #[serde(default)]
    pub comment_id: Option<i64>,
    /// 评论者头像 URL。
    #[serde(default)]
    pub avatar_url: Option<String>,
    /// 评论嵌套深度。
    #[serde(default)]
    pub depth: Option<i32>,
}

/// 这些构造器只在 server function body 内调用，WASM 端因 cfg gate 剥离了调用点，
/// 故对非 server 构建允许 dead_code。
#[cfg_attr(not(feature = "server"), allow(dead_code))]
impl CommentResponse {
    /// 构造失败响应，携带错误码（comment_id / avatar_url / depth 均为 None）。
    pub fn error(error_code: &str, message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            error_code: Some(error_code.into()),
            comment_id: None,
            avatar_url: None,
            depth: None,
        }
    }

    /// 构造成功响应（无关联数据，用于审核/删除等操作）。
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            error_code: None,
            comment_id: None,
            avatar_url: None,
            depth: None,
        }
    }

    /// 构造成功响应，携带新评论 id / 头像 / 深度（用于创建评论）。
    pub fn created(
        message: impl Into<String>,
        comment_id: i64,
        avatar_url: impl Into<String>,
        depth: i32,
    ) -> Self {
        Self {
            success: true,
            message: message.into(),
            error_code: None,
            comment_id: Some(comment_id),
            avatar_url: Some(avatar_url.into()),
            depth: Some(depth),
        }
    }
}

/// 评论树响应：包含文章下的全部已审核评论。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentTreeResponse {
    /// 评论列表。
    pub comments: Vec<PublicComment>,
    /// 评论总数。
    pub count: i64,
}

/// 全部评论列表响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllCommentsResponse {
    /// 评论列表。
    pub comments: Vec<AdminComment>,
    /// 当前筛选下的总数（分页用）。
    pub total: i64,
    /// 各状态的条数（选项卡数量）。
    #[serde(default)]
    pub counts: CommentCounts,
}

/// 各状态的评论条数；回收站按软删除计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentCounts {
    pub pending: i64,
    pub approved: i64,
    pub spam: i64,
    pub trash: i64,
}

/// 批量更新状态响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStatusResponse {
    /// 操作是否成功。
    pub success: bool,
    /// 实际更新的行数。
    pub updated_count: i64,
    /// 提示信息。
    pub message: String,
}
