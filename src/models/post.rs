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

    /// 返回状态徽章在 light/dark 模式下的 Tailwind 背景与颜色类。
    pub fn badge_class(&self) -> &'static str {
        match self {
            PostStatus::Published => {
                "bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300"
            }
            PostStatus::Draft => "bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-400",
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

/// 将时间戳格式化为展示用的 `YYYY-MM-DD` 字符串。
///
/// 此前 `Post::formatted_date` 与 `PostListItem::formatted_date` 各有一份逐字相同的实现。
fn format_date(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d").to_string()
}

impl Post {
    /// 返回用于展示的文章日期：优先使用发布时间，否则回退到创建时间。
    pub fn formatted_date(&self) -> String {
        format_date(self.published_at.unwrap_or(self.created_at))
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

impl PostListItem {
    /// 返回用于展示的文章日期：优先使用发布时间，否则回退到创建时间。
    pub fn formatted_date(&self) -> String {
        format_date(self.published_at.unwrap_or(self.created_at))
    }

    /// 返回中文状态标签。
    pub fn status_label(&self) -> &'static str {
        self.status.label()
    }

    /// 返回状态文本在 light/dark 模式下的 Tailwind 颜色类。
    pub fn status_class(&self) -> &'static str {
        match self.status {
            PostStatus::Published => "text-green-600 dark:text-green-400",
            PostStatus::Draft => "text-gray-400 dark:text-gray-500",
        }
    }

    /// 返回状态徽章在 light/dark 模式下的 Tailwind 背景与颜色类。
    pub fn status_badge_class(&self) -> &'static str {
        self.status.badge_class()
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn sample_post() -> Post {
        Post {
            id: 1,
            author_id: 1,
            title: "Test".to_string(),
            slug: "test".to_string(),
            summary: None,
            content_md: "content".to_string(),
            content_html: None,
            status: PostStatus::Draft,
            published_at: None,
            created_at: Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
            deleted_at: None,
            tags: vec![],
            cover_image: None,
            reading_time: 1,
            word_count: 10,
            toc_html: None,
            prev_post: None,
            next_post: None,
        }
    }

    fn sample_post_list_item() -> PostListItem {
        PostListItem {
            id: 1,
            author_id: 1,
            title: "Test".to_string(),
            slug: "test".to_string(),
            summary: None,
            status: PostStatus::Draft,
            published_at: None,
            created_at: Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
            deleted_at: None,
            tags: vec![],
            cover_image: None,
            reading_time: 1,
            word_count: 10,
        }
    }

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
    fn formatted_date_uses_published_at_when_available() {
        let mut post = sample_post();
        post.published_at = Some(Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap());
        assert_eq!(post.formatted_date(), "2024-06-01");
    }

    #[test]
    fn formatted_date_falls_back_to_created_at() {
        let post = sample_post();
        assert_eq!(post.formatted_date(), "2024-01-15");
    }

    #[test]
    fn post_list_item_formatted_date_uses_published_at_when_available() {
        let mut post = sample_post_list_item();
        post.published_at = Some(Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap());
        assert_eq!(post.formatted_date(), "2024-06-01");
    }

    #[test]
    fn post_list_item_formatted_date_falls_back_to_created_at() {
        let post = sample_post_list_item();
        assert_eq!(post.formatted_date(), "2024-01-15");
    }

    #[test]
    fn post_list_item_status_label() {
        let mut post = sample_post_list_item();
        post.status = PostStatus::Published;
        assert_eq!(post.status_label(), "已发布");
        post.status = PostStatus::Draft;
        assert_eq!(post.status_label(), "草稿");
    }

    #[test]
    fn post_list_item_status_class_returns_non_empty() {
        let mut post = sample_post_list_item();
        post.status = PostStatus::Published;
        assert_eq!(post.status_class(), "text-green-600 dark:text-green-400");
        post.status = PostStatus::Draft;
        assert_eq!(post.status_class(), "text-gray-400 dark:text-gray-500");
    }

    #[test]
    fn post_list_item_status_badge_class_returns_non_empty() {
        let mut post = sample_post_list_item();
        post.status = PostStatus::Published;
        assert_eq!(
            post.status_badge_class(),
            "bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300"
        );
        post.status = PostStatus::Draft;
        assert_eq!(
            post.status_badge_class(),
            "bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-400"
        );
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
