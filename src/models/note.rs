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

/// 公开笔记的概况：笔记列表左栏的类型计数与常用标签。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NoteStats {
    pub moments: i64,
    pub topics: i64,
    /// 按使用次数降序的 `(标签, 条数)`。
    pub tags: Vec<(String, i64)>,
}

impl NoteStats {
    pub fn total(&self) -> i64 {
        self.moments + self.topics
    }
}

/// 笔记本目录里的一篇。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteLink {
    pub slug: String,
    pub title: String,
}

/// 阅读页的笔记本上下文：所在笔记本、本篇序号（从 1 起）与同级已发布笔记。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteContext {
    pub notebook: Notebook,
    pub position: usize,
    pub total: usize,
    pub prev: Option<NoteLink>,
    pub next: Option<NoteLink>,
    pub siblings: Vec<NoteLink>,
}

#[cfg(any(feature = "server", test))]
impl NoteContext {
    /// 在有序的同级笔记里定位本篇；本篇不在其中时返回 `None`。
    pub fn locate(notebook: Notebook, siblings: Vec<NoteLink>, slug: &str) -> Option<Self> {
        let index = siblings.iter().position(|s| s.slug == slug)?;
        Some(Self {
            notebook,
            position: index + 1,
            total: siblings.len(),
            prev: index.checked_sub(1).map(|i| siblings[i].clone()),
            next: siblings.get(index + 1).cloned(),
            siblings,
        })
    }
}

/// 阅读页的笔记本：`requested` 是公开候选之一时用它；否则只属于一个公开笔记本时用那一个。
#[cfg(any(feature = "server", test))]
pub fn choose_notebook(candidates: &[i32], requested: Option<i32>) -> Option<i32> {
    match requested {
        Some(id) if candidates.contains(&id) => Some(id),
        _ if candidates.len() == 1 => Some(candidates[0]),
        _ => None,
    }
}

/// 笔记本编排里的一篇：标题与访客能否看到。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotebookEntry {
    pub id: i32,
    pub title: String,
    /// 有公开版。
    pub published: bool,
    /// 工作稿比公开版新。
    pub ahead: bool,
    /// 在回收站里（仍占目录位置）。
    pub trashed: bool,
}

impl NotebookEntry {
    pub fn state_label(&self) -> &'static str {
        match (self.trashed, self.published, self.ahead) {
            (true, _, _) => "在回收站 · 访客不可见",
            (false, true, true) => "已发布 · 有未发布修改",
            (false, true, false) => "已发布",
            (false, false, _) => "私密草稿 · 访客不可见",
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn link(slug: &str) -> NoteLink {
        NoteLink {
            slug: slug.into(),
            title: slug.to_uppercase(),
        }
    }

    #[test]
    fn notebook_entry_label_explains_visitor_visibility() {
        let entry = |published, ahead, trashed| NotebookEntry {
            id: 1,
            title: String::new(),
            published,
            ahead,
            trashed,
        };
        assert_eq!(
            entry(true, true, false).state_label(),
            "已发布 · 有未发布修改"
        );
        assert_eq!(entry(true, false, false).state_label(), "已发布");
        assert_eq!(
            entry(false, false, false).state_label(),
            "私密草稿 · 访客不可见"
        );
        assert_eq!(
            entry(true, false, true).state_label(),
            "在回收站 · 访客不可见"
        );
    }

    #[test]
    fn requested_notebook_wins_only_when_it_is_a_public_candidate() {
        assert_eq!(choose_notebook(&[3, 7], Some(7)), Some(7));
        // 属于多个笔记本又没指明时，不猜。
        assert_eq!(choose_notebook(&[3, 7], None), None);
        // 指向不属于、非公开或已归档的笔记本时忽略参数。
        assert_eq!(choose_notebook(&[3], Some(9)), Some(3));
        assert_eq!(choose_notebook(&[3, 7], Some(9)), None);
        assert_eq!(choose_notebook(&[], Some(3)), None);
    }

    #[test]
    fn locate_gives_neighbours_and_leaves_the_ends_empty() {
        let siblings = vec![link("a"), link("b"), link("c")];
        let first = NoteContext::locate(Notebook::default(), siblings.clone(), "a").unwrap();
        assert_eq!((first.position, first.total), (1, 3));
        assert_eq!(first.prev, None);
        assert_eq!(first.next, Some(link("b")));
        let last = NoteContext::locate(Notebook::default(), siblings.clone(), "c").unwrap();
        assert_eq!(last.prev, Some(link("b")));
        assert_eq!(last.next, None);
        assert!(NoteContext::locate(Notebook::default(), siblings, "draft").is_none());
    }
}
