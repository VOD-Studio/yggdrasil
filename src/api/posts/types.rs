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

/// 搜索面板（⌘K）里的一条文章建议：只含标题、摘要、发布时间与标签。
// 浏览器端只经反序列化构造，静态分析看不到。
#[cfg_attr(not(feature = "server"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PostSuggestion {
    /// URL slug。
    pub slug: String,
    /// 文章标题。
    pub title: String,
    /// 摘要，可选。
    pub summary: Option<String>,
    /// 发布时间。
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 标签名（按名称排序）。
    pub tags: Vec<String>,
}

/// 文章建议响应。
#[cfg_attr(not(feature = "server"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PostSuggestionResponse {
    /// 最多 8 条。
    pub posts: Vec<PostSuggestion>,
}

/// 文章建议的条数上限。
#[cfg(feature = "server")]
pub const SUGGESTION_LIMIT: i64 = 8;

/// 首页「最新一篇」的目录预览：一级小节总数与前几个小节标题。
#[cfg_attr(not(feature = "server"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PostOutline {
    /// 对应文章的 slug；前台据此确认与列表首篇一致。
    pub slug: String,
    /// 一级小节总数。
    pub total: u32,
    /// 前 `OUTLINE_PREVIEW_LIMIT` 个小节标题（纯文本）。
    pub headings: Vec<String>,
}

/// 目录预览列出的小节数。
#[cfg(feature = "server")]
pub const OUTLINE_PREVIEW_LIMIT: usize = 6;

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
