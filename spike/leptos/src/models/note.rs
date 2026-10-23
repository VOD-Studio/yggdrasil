//! 笔记的共享模型（移植主 crate `src/models/note.rs` 中首页用到的部分）。
//!
//! 首页尾部「最近笔记」只读公开列表：类型、笔记本体、分页结果与筛选条件。
//! 字段与主 crate 保持一致，序列化格式相同。
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: i32,
    pub slug: String,
    pub version: i32,
    pub kind: NoteKind,
    pub title: String,
    /// 编辑 Markdown。公开列表不传输本字段（服务端置空）。
    #[serde(default)]
    pub content_md: String,
    pub content_html: String,
    pub toc_html: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    /// 仅管理接口填充。公开接口不暴露工作稿或知识库版本。
    #[serde(default)]
    pub published_version: Option<i32>,
    #[serde(default)]
    pub knowledge_version: Option<i32>,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub notebook_ids: Vec<i32>,
    /// 目录的一级小节数（服务端从 `toc_html` 解析）；列表不传 `toc_html` 时仍可显示「N 节」。
    #[serde(default)]
    pub sections: u32,
}

/// 没有标题的笔记（多为随记）取摘要前 48 字，摘要也为空时用类型名。
pub fn display_title(kind: NoteKind, title: &str, summary: &str) -> String {
    if title.trim().is_empty() {
        if summary.trim().is_empty() {
            kind.label().to_string()
        } else {
            summary.chars().take(48).collect()
        }
    } else {
        title.to_string()
    }
}

impl Note {
    pub fn display_title(&self) -> String {
        display_title(self.kind, &self.title, &self.summary)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NotePage {
    pub notes: Vec<Note>,
    pub total: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
