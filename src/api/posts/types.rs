//! 文章 API 的请求与响应数据结构。

use crate::models::post::{Post, PostListItem, PostStats, Tag};

/// 创建/更新/删除文章的统一响应结构。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreatePostResponse {
    /// 操作是否成功。
    pub success: bool,
    /// 提示信息。
    pub message: String,
    /// 新文章 id，失败时为 None。
    pub post_id: Option<i32>,
    /// 最终 slug，失败时为 None。
    pub slug: Option<String>,
}

/// 这些构造器只在 server function body 内调用，WASM 端因 cfg gate 剥离了调用点，
/// 故对非 server 构建允许 dead_code。
#[cfg_attr(not(feature = "server"), allow(dead_code))]
impl CreatePostResponse {
    /// 构造失败响应（post_id / slug 均为 None）。
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            post_id: None,
            slug: None,
        }
    }

    /// 构造成功响应，携带新文章 id 与 slug。
    pub fn ok(message: impl Into<String>, post_id: i32, slug: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            post_id: Some(post_id),
            slug: Some(slug.into()),
        }
    }

    /// 构造成功响应（无关联 id/slug，用于批量操作）。
    pub fn ok_msg(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            post_id: None,
            slug: None,
        }
    }
}

/// 文章列表响应。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PostListResponse {
    /// 文章列表（轻量 DTO，不含正文）。
    pub posts: Vec<PostListItem>,
    /// 符合查询条件的总数。
    pub total: i64,
}

/// 标签列表响应。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TagListResponse {
    /// 标签列表。
    pub tags: Vec<Tag>,
}

/// 文章统计响应。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PostStatsResponse {
    /// 文章统计信息。
    pub stats: PostStats,
}

/// 单篇文章详情响应。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SinglePostResponse {
    /// 文章详情，不存在时为 None。
    pub post: Option<Post>,
}

/// 阅读页详情不携带原始 Markdown；完整响应保留给编辑、预览和旧客户端。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PublicPostResponse {
    pub post: Option<crate::models::public_post::PublicPost>,
}

/// Markdown 重建结果响应。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RebuildResult {
    /// 成功重建的文章数量。
    pub rebuilt: u64,
    /// 重建失败的文章数量。
    pub failed: u64,
    /// 失败信息摘要（最多 5 条）。
    pub errors: Vec<String>,
}

/// 浏览器协议适配：业务拒绝保留 success=false，内部错误使用统一脱敏转换。
#[cfg(feature = "server")]
pub(super) fn write_response(
    result: Result<super::service::WriteOutcome, super::service::WriteError>,
    message: &str,
) -> Result<CreatePostResponse, dioxus::prelude::ServerFnError> {
    use super::service::WriteError;
    match result {
        Ok(value) => Ok(CreatePostResponse::ok(message, value.post_id, value.slug)),
        Err(WriteError::Invalid(message)) => Ok(CreatePostResponse::err(message)),
        Err(WriteError::Conflict(_)) => Ok(CreatePostResponse::err(
            "文章已被修改，请重新读取后合并更改",
        )),
        Err(WriteError::Failed(error)) => Err(error.into()),
    }
}
