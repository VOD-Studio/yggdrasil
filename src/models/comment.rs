//! 评论模型。
//!
//! 定义评论状态、服务端内部使用的 Comment 结构体，
//! 以及面向前端展示的 PublicComment 与面向后台管理的 AdminComment。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 评论审核状态枚举，序列化时使用小写字符串。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CommentStatus {
    /// 待审核。
    Pending,
    /// 已通过。
    Approved,
    /// 垃圾评论。
    Spam,
    /// 已删除/回收站。
    Trash,
}

impl CommentStatus {
    /// 将数据库或 API 中的状态字符串解析为 CommentStatus，未知值默认回退到 Pending。
    #[cfg(feature = "server")]
    pub fn from_str(s: &str) -> Self {
        match s {
            "approved" => Self::Approved,
            "spam" => Self::Spam,
            "trash" => Self::Trash,
            _ => Self::Pending,
        }
    }

    /// 将 CommentStatus 序列化为小写字符串。
    #[cfg(test)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Spam => "spam",
            Self::Trash => "trash",
        }
    }
}

/// 面向前端展示的评论结构体，已脱敏。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublicComment {
    /// 评论主键。
    pub id: i64,
    /// 父评论主键，None 表示顶层评论。
    pub parent_id: Option<i64>,
    /// 嵌套深度。
    pub depth: i32,
    /// 评论者名称。
    pub author_name: String,
    /// 评论者个人主页 URL。
    pub author_url: Option<String>,
    /// 评论者头像 URL。
    pub avatar_url: String,
    /// 是否为登录用户（作者）发表的评论。
    pub is_author: bool,
    /// 渲染后的 HTML 内容。
    pub content_html: Option<String>,
    /// 用于展示的人类可读创建时间。
    pub created_at: String,
    /// ISO 8601 格式的创建时间。
    pub created_at_iso: String,
}

/// 面向后台管理的评论结构体，包含审核所需字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdminComment {
    /// 评论主键。
    pub id: i64,
    /// 所属文章主键。
    pub post_id: i32,
    /// 所属文章标题。
    pub post_title: String,
    /// 所属文章 slug。
    pub post_slug: String,
    /// 父评论主键。
    pub parent_id: Option<i64>,
    /// 嵌套深度。
    pub depth: i32,
    /// 评论者名称。
    pub author_name: String,
    /// 评论者邮箱。
    pub author_email: String,
    /// 评论者个人主页 URL。
    pub author_url: Option<String>,
    /// 评论者头像 URL。
    pub avatar_url: String,
    /// 原始 Markdown 内容。
    pub content_md: String,
    /// 当前审核状态。
    pub status: CommentStatus,
    /// 评论创建时间。
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "server")]
    fn comment_status_from_str() {
        assert_eq!(CommentStatus::from_str("pending"), CommentStatus::Pending);
        assert_eq!(CommentStatus::from_str("approved"), CommentStatus::Approved);
        assert_eq!(CommentStatus::from_str("spam"), CommentStatus::Spam);
        assert_eq!(CommentStatus::from_str("trash"), CommentStatus::Trash);
    }

    #[test]
    #[cfg(feature = "server")]
    fn comment_status_from_str_unknown_defaults_to_pending() {
        assert_eq!(CommentStatus::from_str("unknown"), CommentStatus::Pending);
        assert_eq!(CommentStatus::from_str(""), CommentStatus::Pending);
    }

    #[test]
    fn comment_status_as_str() {
        assert_eq!(CommentStatus::Pending.as_str(), "pending");
        assert_eq!(CommentStatus::Approved.as_str(), "approved");
        assert_eq!(CommentStatus::Spam.as_str(), "spam");
        assert_eq!(CommentStatus::Trash.as_str(), "trash");
    }

    #[test]
    fn comment_status_serde_roundtrip() {
        let statuses = vec![
            CommentStatus::Pending,
            CommentStatus::Approved,
            CommentStatus::Spam,
            CommentStatus::Trash,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let expected = format!("\"{}\"", status.as_str());
            assert_eq!(json, expected);
            let deserialized: CommentStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, status);
        }
    }

    #[test]
    fn comment_status_deserialize_from_lowercase() {
        let pending: CommentStatus = serde_json::from_str("\"pending\"").unwrap();
        assert_eq!(pending, CommentStatus::Pending);
        let approved: CommentStatus = serde_json::from_str("\"approved\"").unwrap();
        assert_eq!(approved, CommentStatus::Approved);
    }
}
