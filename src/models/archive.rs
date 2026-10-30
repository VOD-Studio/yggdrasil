//! 归档只传输时间线展示字段，不携带摘要、封面或正文。
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchivePost {
    pub id: i32,
    pub title: String,
    pub slug: String,
    #[cfg_attr(
        target_arch = "wasm32",
        serde(deserialize_with = "crate::models::rfc3339::deserialize")
    )]
    pub published_at: DateTime<Utc>,
    pub reading_time: u32,
    pub tags: Vec<String>,
}
