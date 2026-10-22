//! 文章模型。
//!
//! 定义文章状态、文章结构体、标签、统计信息以及前后导航结构体。
//! Post 结构体在服务端渲染、客户端展示以及缓存层之间共享。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 文章发布状态枚举。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PostStatus {
    /// 草稿，仅管理员可见。
    Draft,
    /// 已发布，面向读者公开。
    Published,
}

impl PostStatus {
    /// 将状态序列化为数据库或 API 使用的小写字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            PostStatus::Draft => "draft",
            PostStatus::Published => "published",
        }
    }

    /// 返回中文展示标签（草稿/已发布）。
    pub fn label(&self) -> &'static str {
        match self {
            PostStatus::Draft => "草稿",
            PostStatus::Published => "已发布",
        }
    }

    /// 后台状态圆点的语气：已发布为陶土色，草稿为灰色（`StatusDot` 的 `tone`）。
    pub fn tone(&self) -> &'static str {
        match self {
            PostStatus::Draft => "muted",
            PostStatus::Published => "clay",
        }
    }

    /// 将字符串解析为 PostStatus，无法识别时返回 None。
    #[cfg(feature = "server")]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(PostStatus::Draft),
            "published" => Some(PostStatus::Published),
            _ => None,
        }
    }
}

/// 文章领域模型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Post {
    /// 文章主键。
    pub id: i32,
    /// 作者用户主键。
    pub author_id: i32,
    /// 文章标题。
    pub title: String,
    /// URL slug，用于生成文章链接。
    pub slug: String,
    /// 摘要，可选。
    pub summary: Option<String>,
    /// 原始 Markdown 内容。
    pub content_md: String,
    /// 渲染后的 HTML 内容，可选。
    pub content_html: Option<String>,
    /// 文章发布状态。
    pub status: PostStatus,
    /// 正式发布时间，None 表示尚未发布。
    pub published_at: Option<DateTime<Utc>>,
    /// 创建时间。
    pub created_at: DateTime<Utc>,
    /// 最后更新时间。
    pub updated_at: DateTime<Utc>,
    /// 软删除时间，None 表示未删除。仅回收站查询填充。
    pub deleted_at: Option<DateTime<Utc>>,
    /// 关联标签列表。
    pub tags: Vec<String>,
    /// 封面图片 URL。
    pub cover_image: Option<String>,
    /// 预计阅读时间（分钟）。
    pub reading_time: u32,
    /// 字数统计。
    pub word_count: u32,
    /// 目录 HTML。
    pub toc_html: Option<String>,
    /// 上一篇文章导航信息。
    pub prev_post: Option<PostNav>,
    /// 下一篇文章导航信息。
    pub next_post: Option<PostNav>,
}

/// 页眉展示所需的最小字段集（P4.2）。
///
/// [`crate::components::post::post_header::PostHeader`] 与
/// [`crate::components::post::post_meta::PostMeta`] 只读取这些字段；以小型
/// props 传递可避免为页眉复制/深比较完整正文（content_html 可能达数百 KB）。
#[derive(Debug, Clone, PartialEq)]
pub struct PostHeaderData {
    pub id: i32,
    pub title: String,
    pub status: PostStatus,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub reading_time: u32,
    pub word_count: u32,
}

impl From<&Post> for PostHeaderData {
    fn from(post: &Post) -> Self {
        Self {
            id: post.id,
            title: post.title.clone(),
            status: post.status.clone(),
            summary: post.summary.clone(),
            tags: post.tags.clone(),
            published_at: post.published_at,
            created_at: post.created_at,
            reading_time: post.reading_time,
            word_count: post.word_count,
        }
    }
}

/// 页脚展示所需的最小字段集（P4.2）：前后篇导航（标签已移到页眉）。
#[derive(Debug, Clone, PartialEq)]
pub struct PostFooterData {
    pub prev_post: Option<PostNav>,
    pub next_post: Option<PostNav>,
}

impl From<&Post> for PostFooterData {
    fn from(post: &Post) -> Self {
        Self {
            prev_post: post.prev_post.clone(),
            next_post: post.next_post.clone(),
        }
    }
}

/// 文章列表项 DTO。
///
/// 仅包含列表/标签/搜索/归档等场景需要的字段，不含 `content_md` 与 `content_html`，
/// 以降低缓存内存占用与序列化体积。`deleted_at` 保留，供回收站列表使用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostListItem {
    /// 文章主键。
    pub id: i32,
    /// 作者用户主键。
    pub author_id: i32,
    /// 文章标题。
    pub title: String,
    /// URL slug，用于生成文章链接。
    pub slug: String,
    /// 摘要，可选。
    pub summary: Option<String>,
    /// 文章发布状态。
    pub status: PostStatus,
    /// 正式发布时间，None 表示尚未发布。
    pub published_at: Option<DateTime<Utc>>,
    /// 创建时间。
    pub created_at: DateTime<Utc>,
    /// 最后更新时间。
    pub updated_at: DateTime<Utc>,
    /// 软删除时间，None 表示未删除。仅回收站查询填充。
    pub deleted_at: Option<DateTime<Utc>>,
    /// 关联标签列表。
    pub tags: Vec<String>,
    /// 封面图片 URL。
    pub cover_image: Option<String>,
    /// 预计阅读时间（分钟）。
    pub reading_time: u32,
    /// 字数统计。
    pub word_count: u32,
}

#[cfg(any(feature = "server", test))]
/// Feed 条目 DTO（RSS 2.0 / JSON Feed 共享）。
///
/// 与 `PostListItem` 不同，这里携带已渲染的 `content_html` 全文，
/// 供订阅端点直接输出；缓存单键 `CacheKey::Feed` 存 `Vec<FeedItem>`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedItem {
    /// 文章标题。
    pub title: String,
    /// URL slug，用于生成文章链接。
    pub slug: String,
    /// 摘要，可选。
    pub summary: Option<String>,
    /// 渲染后的 HTML 内容，可选（历史文章可能为空）。
    pub content_html: Option<String>,
    /// 正式发布时间（已发布文章必有值，行映射时回退 updated_at）。
    pub published_at: DateTime<Utc>,
    /// 最后更新时间。
    pub updated_at: DateTime<Utc>,
    /// 关联标签列表。
    pub tags: Vec<String>,
}

/// 前后文章导航结构体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostNav {
    /// 文章标题。
    pub title: String,
    /// 文章 slug。
    pub slug: String,
}

/// 标签领域模型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    /// 标签主键。
    pub id: i32,
    /// 标签名称。
    pub name: String,
    /// 关联文章数量。
    pub post_count: i64,
}

/// 文章统计信息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostStats {
    /// 文章总数。
    pub total: i64,
    /// 草稿数量。
    pub drafts: i64,
    /// 已发布数量。
    pub published: i64,
    /// 回收站（软删除）数量。
    pub trash: i64,
    /// 近 30 天新建文章数（不含软删除），供仪表盘趋势徽章展示真实增量。
    pub recent_30d: i64,
    /// 近 12 个月（含本月）每月发布篇数，旧 → 新，无文章的月为 0；按 UTC 月份
    /// 归入，供仪表盘「每月发布篇数」柱图使用。
    pub monthly_published: Vec<MonthCount>,
}

/// 某个月的篇数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonthCount {
    pub year: i32,
    /// 1–12。
    pub month: u32,
    pub count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "server")]
    fn post_status_from_str() {
        assert_eq!(PostStatus::from_str("draft"), Some(PostStatus::Draft));
        assert_eq!(
            PostStatus::from_str("published"),
            Some(PostStatus::Published)
        );
        assert_eq!(PostStatus::from_str("unknown"), None);
        assert_eq!(PostStatus::from_str(""), None);
    }

    #[test]
    fn post_status_as_str() {
        assert_eq!(PostStatus::Draft.as_str(), "draft");
        assert_eq!(PostStatus::Published.as_str(), "published");
    }

    #[test]
    #[cfg(feature = "server")]
    fn post_status_roundtrip() {
        for status in [PostStatus::Draft, PostStatus::Published] {
            assert_eq!(PostStatus::from_str(status.as_str()), Some(status.clone()));
        }
    }

    #[test]
    fn post_status_serde_roundtrip() {
        let json = serde_json::to_string(&PostStatus::Draft).unwrap();
        assert_eq!(
            serde_json::from_str::<PostStatus>(&json).unwrap(),
            PostStatus::Draft
        );
    }
}
