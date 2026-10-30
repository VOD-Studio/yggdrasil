//! 素材（图片）模型。
//!
//! `assets` 表是 `uploads/` 目录的元数据注册表：磁盘是字节唯一存储，
//! 本表承载路径、尺寸、alt 等管理性字段。`asset_refs` 记录文章引用关系。
//! 这些结构体通过 serde 在服务端与客户端之间共享序列化。
//!
//! id 以 String 承载（SQL 侧 `id::text` 读出、`$1::uuid` 写入），
//! 避免把 server-only 的 uuid crate 引入 WASM 前端构建。

#[cfg(feature = "server")]
use rmcp::schemars;
use serde::de::{self, Deserializer};
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
    #[cfg_attr(
        target_arch = "wasm32",
        serde(deserialize_with = "crate::models::rfc3339::deserialize")
    )]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 引用该素材的一处来源（素材详情浮层/删除拦截时列出）。
///
/// 线格式是以 `kind` 为内部标签的扁平对象，WASM 前端按 `kind` 判别分组渲染。
/// 序列化用派生实现；反序列化手写（见下方 `impl Deserialize`），避免内部标签
/// 派生在 WASM 里引入 `Content` 缓冲。来源与
/// `api::assets::ASSET_REF_CLAUSE` 一一对应：
/// 文章引用来自 asset_refs 表（正文 HTML + 封面，含草稿与回收站文章）；
/// 笔记引用来自 note_asset_refs（包含所有历史版本）；
/// 评论/头像引用在查询时按素材路径直接匹配（存活评论、用户头像、友链头像）。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssetRef {
    /// 包括工作稿、收录版、公开版和历史版本，始终链接后台。
    Note { note_id: i32, title: String },
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

impl<'de> Deserialize<'de> for AssetRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Kind {
            Note,
            Post,
            Comment,
            UserAvatar,
            FriendAvatar,
        }

        /// 各变体字段的并集；按 `kind` 取用，多余字段忽略（与派生行为一致）。
        #[derive(Deserialize)]
        struct Wire {
            kind: Kind,
            note_id: Option<i32>,
            title: Option<String>,
            post_id: Option<i32>,
            slug: Option<String>,
            status: Option<AssetRefPostStatus>,
            comment_id: Option<i64>,
            author_name: Option<String>,
            post_title: Option<String>,
            post_slug: Option<String>,
            post_status: Option<AssetRefPostStatus>,
            user_id: Option<i32>,
            label: Option<String>,
            friend_id: Option<i32>,
            name: Option<String>,
        }

        fn req<T, E: de::Error>(value: Option<T>, field: &'static str) -> Result<T, E> {
            value.ok_or_else(|| E::missing_field(field))
        }

        let w = Wire::deserialize(d)?;
        Ok(match w.kind {
            Kind::Note => AssetRef::Note {
                note_id: req(w.note_id, "note_id")?,
                title: req(w.title, "title")?,
            },
            Kind::Post => AssetRef::Post {
                post_id: req(w.post_id, "post_id")?,
                title: req(w.title, "title")?,
                slug: req(w.slug, "slug")?,
                status: req(w.status, "status")?,
            },
            Kind::Comment => AssetRef::Comment {
                comment_id: req(w.comment_id, "comment_id")?,
                author_name: req(w.author_name, "author_name")?,
                post_id: req(w.post_id, "post_id")?,
                post_title: req(w.post_title, "post_title")?,
                post_slug: req(w.post_slug, "post_slug")?,
                post_status: req(w.post_status, "post_status")?,
            },
            Kind::UserAvatar => AssetRef::UserAvatar {
                user_id: req(w.user_id, "user_id")?,
                label: req(w.label, "label")?,
            },
            Kind::FriendAvatar => AssetRef::FriendAvatar {
                friend_id: req(w.friend_id, "friend_id")?,
                name: req(w.name, "name")?,
            },
        })
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

    /// 来源文章的状态标注：已发布不标。
    pub fn note(&self) -> Option<&'static str> {
        match self {
            Self::Published => None,
            Self::Draft => Some("草稿"),
            Self::Trashed => Some("回收站"),
        }
    }
}

/// 列表页 DTO：素材本体 + 引用计数 + 引用文章列表。
///
/// 线格式把素材字段平铺在顶层（序列化用 `flatten`）；反序列化手写，避免
/// `flatten` 派生在 WASM 里引入 `Content` 缓冲。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AssetDto {
    #[serde(flatten)]
    pub asset: Asset,
    pub ref_count: i64,
    pub refs: Vec<AssetRef>,
}

impl<'de> Deserialize<'de> for AssetDto {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: String,
            path: String,
            filename: String,
            mime: String,
            size_bytes: i64,
            width: i32,
            height: i32,
            alt: Option<String>,
            #[cfg_attr(
                target_arch = "wasm32",
                serde(deserialize_with = "crate::models::rfc3339::deserialize")
            )]
            created_at: chrono::DateTime<chrono::Utc>,
            ref_count: i64,
            refs: Vec<AssetRef>,
        }

        let w = Wire::deserialize(d)?;
        Ok(AssetDto {
            asset: Asset {
                id: w.id,
                path: w.path,
                filename: w.filename,
                mime: w.mime,
                size_bytes: w.size_bytes,
                width: w.width,
                height: w.height,
                alt: w.alt,
                created_at: w.created_at,
            },
            ref_count: w.ref_count,
            refs: w.refs,
        })
    }
}

/// 列表筛选：按引用状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "server", derive(rmcp::schemars::JsonSchema))]
pub enum AssetFilter {
    #[default]
    All,
    Used,
    Orphan,
}

/// 列表排序。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "server", derive(rmcp::schemars::JsonSchema))]
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
    fn ref_post_status_note() {
        assert_eq!(AssetRefPostStatus::Published.note(), None);
        assert_eq!(AssetRefPostStatus::Draft.note(), Some("草稿"));
        assert_eq!(AssetRefPostStatus::Trashed.note(), Some("回收站"));
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

    #[test]
    fn asset_refs_round_trip_all_kinds() {
        for r in [
            AssetRef::Note {
                note_id: 1,
                title: "笔记".into(),
            },
            AssetRef::Post {
                post_id: 2,
                title: "文章".into(),
                slug: "post".into(),
                status: AssetRefPostStatus::Draft,
            },
            AssetRef::Comment {
                comment_id: 3,
                author_name: "访客".into(),
                post_id: 2,
                post_title: "文章".into(),
                post_slug: "post".into(),
                post_status: AssetRefPostStatus::Trashed,
            },
            AssetRef::UserAvatar {
                user_id: 4,
                label: "xfy".into(),
            },
            AssetRef::FriendAvatar {
                friend_id: 5,
                name: "友链".into(),
            },
        ] {
            let json = serde_json::to_value(&r).unwrap();
            assert_eq!(serde_json::from_value::<AssetRef>(json).unwrap(), r);
        }
        assert!(serde_json::from_str::<AssetRef>(r#"{"kind":"post","post_id":1}"#).is_err());
        assert!(serde_json::from_str::<AssetRef>(r#"{"kind":"teapot"}"#).is_err());
    }

    #[test]
    fn asset_dto_round_trips_flat_json() {
        let dto = AssetDto {
            asset: Asset {
                id: "00000000-0000-0000-0000-000000000001".into(),
                path: "2026/07/24/153000.a.webp".into(),
                filename: "a.webp".into(),
                mime: "image/webp".into(),
                size_bytes: 1024,
                width: 640,
                height: 480,
                alt: Some("示意图".into()),
                created_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            },
            ref_count: 1,
            refs: vec![AssetRef::Post {
                post_id: 2,
                title: "文章".into(),
                slug: "post".into(),
                status: AssetRefPostStatus::Published,
            }],
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["path"], "2026/07/24/153000.a.webp");
        assert_eq!(serde_json::from_value::<AssetDto>(json).unwrap(), dto);
    }
}
