//! 友链模型。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 友链：前台 /friends 卡片页与后台 /admin/friends 管理共用的 serde DTO。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FriendLink {
    pub id: i32,
    pub name: String,
    pub url: String,
    pub avatar_url: Option<String>,
    pub description: String,
    pub sort_order: i32,
    pub is_active: bool,
    #[cfg_attr(
        target_arch = "wasm32",
        serde(deserialize_with = "crate::models::rfc3339::deserialize")
    )]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(
        target_arch = "wasm32",
        serde(deserialize_with = "crate::models::rfc3339::deserialize")
    )]
    pub updated_at: DateTime<Utc>,
}
