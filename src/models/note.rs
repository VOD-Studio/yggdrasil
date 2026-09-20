//! 笔记、不可变版本和有序笔记本的共享模型。
use chrono::{DateTime, Utc};
#[cfg(feature = "server")]
use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(rmcp::schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    #[default]
    Moment,
    Topic,
}

impl NoteKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Moment => "moment",
            Self::Topic => "topic",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Moment => "随记",
            Self::Topic => "主题笔记",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(rmcp::schemars::JsonSchema))]
pub struct NoteDraft {
    pub id: Option<i32>,
    /// 更新必须携带读到的版本；过期写入被拒绝。
    pub expected_version: Option<i32>,
    #[serde(default)]
    pub kind: NoteKind,
    #[serde(default)]
    pub title: String,
    pub content_md: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notebook_ids: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: i32,
    pub slug: String,
    pub version: i32,
    pub kind: NoteKind,
    pub title: String,
    pub content_md: String,
    pub content_html: String,
    pub toc_html: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    /// 仅管理接口填充。公开接口不暴露工作稿或知识库版本。
    pub published_version: Option<i32>,
    pub knowledge_version: Option<i32>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub notebook_ids: Vec<i32>,
}

impl Note {
    pub fn display_title(&self) -> String {
        if self.title.trim().is_empty() {
            if self.summary.trim().is_empty() {
                self.kind.label().to_string()
            } else {
                self.summary.chars().take(48).collect()
            }
        } else {
            self.title.clone()
        }
    }
    pub fn draft(&self) -> NoteDraft {
        NoteDraft {
            id: Some(self.id),
            expected_version: Some(self.version),
            kind: self.kind,
            title: self.title.clone(),
            content_md: self.content_md.clone(),
            tags: self.tags.clone(),
            notebook_ids: self.notebook_ids.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NotePage {
    pub notes: Vec<Note>,
    pub total: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(rmcp::schemars::JsonSchema))]
pub struct NoteFilter {
    #[serde(default)]
    pub query: String,
    pub kind: Option<NoteKind>,
    pub notebook_id: Option<i32>,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub trash: bool,
    /// 仅管理接口应用；公开查询不能借此探测私密状态。
    pub published: Option<bool>,
    pub knowledge: Option<bool>,
    #[serde(default)]
    pub page: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Notebook {
    pub id: i32,
    pub title: String,
    pub description: String,
    pub is_public: bool,
    pub archived_at: Option<DateTime<Utc>>,
    pub note_count: i64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NotebookInput {
    pub id: Option<i32>,
    pub title: String,
    pub description: String,
    pub is_public: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteRevision {
    pub version: i32,
    pub title: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteAction {
    Publish,
    Unpublish,
    IncludeKnowledge,
    ExcludeKnowledge,
    Trash,
    Restore,
}
