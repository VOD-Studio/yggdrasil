//! 笔记阅读页传输模型。编辑器、MCP 与知识库继续使用完整 [`crate::models::note::Note`]，
//! 公开笔记详情不再向匿名读者发送编辑 Markdown 与知识库内部版本号。
//!
//! 对应 docs/wasm-optimization-plan.md P4.1；字段以公开详情页（`NoteDocument`）
//! 实际渲染所需为准，参照 [`crate::models::public_post::PublicPost`] 的模式。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::note::{Note, NoteKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublicNote {
    pub id: i32,
    pub slug: String,
    /// 公开展示的版本号（已发布修订版本的版本号）。详情页 meta 直接显示
    /// 「版本 N」，因此不能像其他内部版本号一样省略。
    pub version: i32,
    pub kind: NoteKind,
    pub title: String,
    pub summary: String,
    pub content_html: String,
    pub toc_html: String,
    pub tags: Vec<String>,
    #[cfg_attr(
        target_arch = "wasm32",
        serde(deserialize_with = "crate::models::rfc3339::deserialize")
    )]
    pub created_at: DateTime<Utc>,
    /// 已发布修订版本的创建时间（列表与详情的「更新于」语义）。
    #[cfg_attr(
        target_arch = "wasm32",
        serde(deserialize_with = "crate::models::rfc3339::deserialize")
    )]
    pub updated_at: DateTime<Utc>,
    /// 目录的一级小节数（服务端从 `toc_html` 解析），详情 meta 显示「N 节」。
    #[serde(default)]
    pub sections: u32,
}

impl From<Note> for PublicNote {
    fn from(note: Note) -> Self {
        Self {
            id: note.id,
            slug: note.slug,
            version: note.version,
            kind: note.kind,
            title: note.title,
            summary: note.summary,
            content_html: note.content_html,
            toc_html: note.toc_html,
            tags: note.tags,
            created_at: note.created_at,
            updated_at: note.updated_at,
            sections: note.sections,
        }
    }
}

impl PublicNote {
    /// 与 [`Note::display_title`] 相同的展示标题规则：空标题回退摘要前 48 字，
    /// 再回退类型标签。后台预览把完整 Note 转换为 PublicNote 后复用同一渲染路径。
    pub fn display_title(&self) -> String {
        super::note::display_title(self.kind, &self.title, &self.summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::note::NoteKind;

    fn sample_note() -> Note {
        let now = Utc::now();
        Note {
            id: 7,
            slug: "garden".into(),
            version: 3,
            kind: NoteKind::Topic,
            title: "生长中的想法".into(),
            content_md: "仅编辑需要的原文".into(),
            content_html: "<p>正文</p>".into(),
            toc_html: "<nav>目录</nav>".into(),
            summary: "摘要".into(),
            tags: vec!["rust".into()],
            created_at: now,
            updated_at: now,
            published_at: Some(now),
            published_version: Some(3),
            knowledge_version: Some(2),
            deleted_at: None,
            notebook_ids: vec![1, 2],
            sections: 0,
        }
    }

    #[test]
    fn public_note_omits_markdown_and_internal_fields() {
        let payload = serde_json::to_value(PublicNote::from(sample_note())).unwrap();
        assert!(
            payload.get("content_md").is_none(),
            "公开 DTO 不得携带编辑 Markdown"
        );
        assert!(
            payload.get("published_version").is_none(),
            "工作稿版本不外发"
        );
        assert!(
            payload.get("knowledge_version").is_none(),
            "知识库版本不外发"
        );
        assert!(payload.get("deleted_at").is_none());
        assert!(payload.get("notebook_ids").is_none(), "笔记本归属不外发");
        assert_eq!(payload["version"], 3);
        assert_eq!(payload["content_html"], "<p>正文</p>");
    }

    #[test]
    fn display_title_matches_note_rules() {
        let mut public = PublicNote::from(sample_note());
        assert_eq!(public.display_title(), "生长中的想法");
        public.title = String::new();
        assert_eq!(public.display_title(), "摘要");
        public.summary = String::new();
        assert_eq!(public.display_title(), "主题笔记");
    }
}
