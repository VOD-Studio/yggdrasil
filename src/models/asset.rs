//! 素材（图片）模型。
//!
//! `assets` 表是 `uploads/` 目录的元数据注册表：磁盘是字节唯一存储，
//! 本表承载路径、尺寸、alt 等管理性字段。`asset_refs` 记录文章引用关系。
//! 这些结构体通过 serde 在服务端与客户端之间共享序列化。
//!
//! id 以 String 承载（SQL 侧 `id::text` 读出、`$1::uuid` 写入），
//! 避免把 server-only 的 uuid crate 引入 WASM 前端构建。

use serde::{Deserialize, Serialize};

/// 素材记录（对应 assets 表一行）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Asset {
    pub id: String,
    /// 相对路径，如 `2026/07/24/153000.<uuid>.webp`（不含 /uploads/ 前缀）。
    pub path: String,
    pub filename: String,
    pub mime: String,
    pub size_bytes: i64,
    pub width: i32,
    pub height: i32,
    pub alt: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 引用该素材的一处来源（素材详情浮层/删除拦截时列出）。
///
/// serde tagged enum，WASM 前端按 `kind` 判别分组渲染。四个来源与
/// `api::assets::ASSET_REF_CLAUSE` 一一对应：
/// 文章引用来自 asset_refs 表（正文 HTML + 封面，含草稿与回收站文章）；
/// 评论/头像引用在查询时按素材路径直接匹配（存活评论、用户头像、友链头像）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssetRef {
    /// 文章引用（asset_refs 表）。
    Post {
        post_id: i32,
        title: String,
        slug: String,
        status: AssetRefPostStatus,
    },
    /// 存活评论引用（content_html 子串匹配；评论无深链，展示作者 + 所属文章）。
    Comment {
        comment_id: i64,
        author_name: String,
        post_id: i32,
        post_title: String,
        post_slug: String,
        post_status: AssetRefPostStatus,
    },
    /// 用户头像引用（users.avatar_url）。label = display_name 回退 username。
    UserAvatar { user_id: i32, label: String },
    /// 友链头像引用（friend_links.avatar_url）。
    FriendAvatar { friend_id: i32, name: String },
}

impl AssetRef {
    /// 一行可读描述（删除禁用 tooltip 等纯文本场景）。
    pub fn describe(&self) -> String {
        match self {
            AssetRef::Post { title, .. } => format!("文章《{title}》"),
            AssetRef::Comment {
                author_name,
                post_title,
                ..
            } => format!("评论（{author_name} 在《{post_title}》）"),
            AssetRef::UserAvatar { label, .. } => format!("用户头像（{label}）"),
            AssetRef::FriendAvatar { name, .. } => format!("友链头像（{name}）"),
        }
    }
}

/// 引用来源文章的可见性状态（决定后台链接走向与状态徽标）。
///
/// asset_refs 含草稿与回收站文章：回收站引用同样阻止素材删除，
/// 草稿/回收站文章前台不可见，链接须指向后台编辑页。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetRefPostStatus {
    Published,
    Draft,
    /// 回收站（软删除）文章。
    Trashed,
}

impl AssetRefPostStatus {
    /// 由 posts.status + deleted_at 推导（纯函数，便于单测）。
    /// 未识别的 status 一律按草稿处理（保守：不链向前台）。
    #[cfg(any(feature = "server", test))]
    pub fn resolve(status: &str, deleted_at: Option<chrono::DateTime<chrono::Utc>>) -> Self {
        if deleted_at.is_some() {
            Self::Trashed
        } else if status == "published" {
            Self::Published
        } else {
            Self::Draft
        }
    }

    /// 状态徽标（label, class）：已发布不显示徽标，返回 None。
    pub fn badge(&self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Published => None,
            Self::Draft => Some((
                "草稿",
                "bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-400",
            )),
            Self::Trashed => Some((
                "回收站",
                "bg-red-100 dark:bg-red-900/30 text-red-600 dark:text-red-400",
            )),
        }
    }
}

/// 列表页 DTO：素材本体 + 引用计数 + 引用文章列表。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetDto {
    #[serde(flatten)]
    pub asset: Asset,
    pub ref_count: i64,
    pub refs: Vec<AssetRef>,
}

/// 列表筛选：按引用状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum AssetFilter {
    #[default]
    All,
    Used,
    Orphan,
}

/// 列表排序。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum AssetSort {
    #[default]
    CreatedDesc,
    SizeDesc,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_post_status_resolve() {
        let now = chrono::Utc::now();
        // deleted_at 优先于 status：回收站文章即使 status 仍是 published 也判 Trashed。
        assert_eq!(
            AssetRefPostStatus::resolve("published", Some(now)),
            AssetRefPostStatus::Trashed
        );
        assert_eq!(
            AssetRefPostStatus::resolve("published", None),
            AssetRefPostStatus::Published
        );
        assert_eq!(
            AssetRefPostStatus::resolve("draft", None),
            AssetRefPostStatus::Draft
        );
        // 未识别 status 保守按草稿（不链向前台）。
        assert_eq!(
            AssetRefPostStatus::resolve("unknown", None),
            AssetRefPostStatus::Draft
        );
    }

    #[test]
    fn ref_post_status_badge() {
        assert_eq!(AssetRefPostStatus::Published.badge(), None);
        assert_eq!(AssetRefPostStatus::Draft.badge().map(|b| b.0), Some("草稿"));
        assert_eq!(
            AssetRefPostStatus::Trashed.badge().map(|b| b.0),
            Some("回收站")
        );
    }

    #[test]
    fn asset_ref_describe_covers_all_kinds() {
        let status = AssetRefPostStatus::Published;
        let cases = [
            (
                AssetRef::Post {
                    post_id: 1,
                    title: "标题".into(),
                    slug: "s".into(),
                    status,
                },
                "文章《标题》",
            ),
            (
                AssetRef::Comment {
                    comment_id: 1,
                    author_name: "小明".into(),
                    post_id: 1,
                    post_title: "标题".into(),
                    post_slug: "s".into(),
                    post_status: status,
                },
                "评论（小明 在《标题》）",
            ),
            (
                AssetRef::UserAvatar {
                    user_id: 1,
                    label: "xfy".into(),
                },
                "用户头像（xfy）",
            ),
            (
                AssetRef::FriendAvatar {
                    friend_id: 1,
                    name: "某博客".into(),
                },
                "友链头像（某博客）",
            ),
        ];
        for (r, want) in cases {
            assert_eq!(r.describe(), want);
        }
    }

    #[test]
    fn asset_ref_serde_tagged_shape() {
        // 前端按 kind 判别分组渲染，tagged enum 的线格式是跨端契约。
        let r = AssetRef::UserAvatar {
            user_id: 7,
            label: "xfy".into(),
        };
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["kind"], "user_avatar");
        assert_eq!(json["user_id"], 7);
        let back: AssetRef = serde_json::from_value(json).unwrap();
        assert_eq!(back, r);
    }
}
