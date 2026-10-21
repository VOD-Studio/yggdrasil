//! Form data, the saved baseline and the editor lifecycle. Readiness has one state, so
//! a failed initialization cannot leave the page trapped under the loading skeleton.
use chrono::{DateTime, Utc};
use dioxus::prelude::*;

use crate::bridges::library::LibraryResource;
use crate::bridges::tiptap::{UploadErrorEntry, UploadsInFlight};
use crate::models::post::Post;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct PostDraft {
    pub title: String,
    pub summary: String,
    pub slug: String,
    pub tags: String,
    pub cover_image: String,
    pub status: String,
    pub content: String,
}

impl Default for PostDraft {
    fn default() -> Self {
        Self {
            title: String::new(),
            summary: String::new(),
            slug: String::new(),
            tags: String::new(),
            cover_image: String::new(),
            status: "draft".into(),
            content: String::new(),
        }
    }
}

impl PostDraft {
    pub fn parsed_tags(&self) -> Vec<String> {
        self.tags
            .split([',', '，', ';', '；'])
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_owned)
            .collect()
    }

    /// 追加标签（输入里的分隔符都算），已有的不重复。
    pub fn add_tags(&mut self, input: &str) {
        let mut tags = self.parsed_tags();
        let probe = PostDraft {
            tags: input.to_owned(),
            ..PostDraft::default()
        };
        for tag in probe.parsed_tags() {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        self.tags = tags.join(", ");
    }

    pub fn remove_tag(&mut self, tag: &str) {
        let tags: Vec<String> = self
            .parsed_tags()
            .into_iter()
            .filter(|t| t != tag)
            .collect();
        self.tags = tags.join(", ");
    }

    pub fn is_published(&self) -> bool {
        self.status == "published"
    }
}

impl From<Post> for PostDraft {
    fn from(post: Post) -> Self {
        Self {
            title: post.title,
            summary: post.summary.unwrap_or_default(),
            slug: post.slug,
            tags: post.tags.join(", "),
            cover_image: post.cover_image.unwrap_or_default(),
            status: post.status.as_str().into(),
            content: post.content_md,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(super) enum EditorPhase {
    Initializing,
    Ready,
    Failed(String),
}

impl EditorPhase {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Initializing)
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(super) enum SaveState {
    #[default]
    Idle,
    Saving,
    /// 失败信息与那次要保存成的状态，点状态指示可按原状态重试。
    Failed {
        message: String,
        status: &'static str,
    },
}

impl SaveState {
    pub fn is_saving(&self) -> bool {
        matches!(self, Self::Saving)
    }
}

/// 页头的保存状态指示。
#[derive(Clone, Debug, PartialEq)]
pub(super) enum SaveIndicator {
    /// 新文章还没保存过。
    NotSaved,
    /// 与服务端一致；本页保存过时带时间。
    Saved(Option<String>),
    Dirty,
    Saving,
    Failed,
    /// 本地副本等待选择恢复或放弃。
    Waiting,
}

impl SaveIndicator {
    pub fn of(
        save: &SaveState,
        dirty: bool,
        never_saved: bool,
        saved_at: Option<String>,
        recovering: bool,
    ) -> Self {
        match save {
            _ if recovering => Self::Waiting,
            SaveState::Saving => Self::Saving,
            SaveState::Failed { .. } => Self::Failed,
            SaveState::Idle if dirty => Self::Dirty,
            SaveState::Idle if never_saved => Self::NotSaved,
            SaveState::Idle => Self::Saved(saved_at),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::NotSaved => "尚未保存".into(),
            Self::Saved(Some(at)) => format!("已保存 · {at}"),
            Self::Saved(None) => "已保存".into(),
            Self::Dirty => "有未保存的修改".into(),
            Self::Saving => "正在保存…".into(),
            Self::Failed => "保存失败 · 点击重试".into(),
            Self::Waiting => "等待选择".into(),
        }
    }

    /// 圆点色调，对应 CSS 的 data-tone。
    pub fn tone(&self) -> &'static str {
        match self {
            Self::NotSaved | Self::Saved(_) => "muted",
            Self::Dirty | Self::Saving | Self::Waiting => "clay",
            Self::Failed => "danger",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct WriteEditorState {
    /// 编辑中的文章；新文章为 None，首次保存后换到编辑路由。
    pub post_id: Option<i32>,
    pub form: Signal<PostDraft>,
    /// 最近一次与服务端一致的内容，form 与它不同即有未保存修改。
    pub saved: Signal<PostDraft>,
    pub saved_at: Signal<Option<String>>,
    pub published_at: Signal<Option<DateTime<Utc>>>,
    pub phase: Signal<EditorPhase>,
    pub save: Signal<SaveState>,
    pub library: LibraryResource,
    pub uploads_in_flight: Signal<UploadsInFlight>,
    pub upload_errors: Signal<Vec<UploadErrorEntry>>,
    pub cover_uploading: Signal<bool>,
    pub body_picker_visible: Signal<bool>,
    pub body_picker_uploading: Signal<bool>,
    pub(super) reload: Signal<u32>,
    #[cfg(target_arch = "wasm32")]
    pub editor: Signal<Option<crate::bridges::tiptap::EditorHandle>>,
}

impl WriteEditorState {
    pub fn retry(mut self) {
        self.reload += 1;
    }

    pub fn dirty(self) -> bool {
        *self.form.read() != *self.saved.read()
    }

    /// 保存或上传尚未完成，此时离开会丢内容。
    pub fn busy(self) -> bool {
        let uploads = self.uploads_in_flight.read();
        self.save.read().is_saving()
            || uploads.uploading > 0
            || (self.cover_uploading)()
            || (self.body_picker_uploading)()
    }

    /// 把正文换成给定 Markdown（起步模板、恢复本地副本）。
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
    pub fn set_content(mut self, markdown: &str) {
        #[cfg(target_arch = "wasm32")]
        if let Some(handle) = &*self.editor.peek() {
            handle.instance().set_markdown(markdown);
        }
        self.form.write().content = markdown.to_owned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_add_without_duplicates_and_remove_by_name() {
        let mut draft = PostDraft {
            tags: "Rust, 架构".into(),
            ..PostDraft::default()
        };
        draft.add_tags("架构，Dioxus；");
        assert_eq!(draft.tags, "Rust, 架构, Dioxus");
        draft.remove_tag("架构");
        assert_eq!(draft.parsed_tags(), ["Rust", "Dioxus"]);
    }

    #[test]
    fn new_posts_start_as_drafts() {
        assert!(!PostDraft::default().is_published());
    }

    #[test]
    fn indicator_prefers_saving_and_failure_over_dirty() {
        let failed = SaveState::Failed {
            message: "x".into(),
            status: "draft",
        };
        assert_eq!(
            SaveIndicator::of(&SaveState::Saving, true, false, None, false),
            SaveIndicator::Saving
        );
        assert_eq!(
            SaveIndicator::of(&failed, true, false, None, false),
            SaveIndicator::Failed
        );
        assert_eq!(
            SaveIndicator::of(&SaveState::Idle, true, true, None, false),
            SaveIndicator::Dirty
        );
        assert_eq!(
            SaveIndicator::of(&SaveState::Idle, false, true, None, false),
            SaveIndicator::NotSaved
        );
        assert_eq!(
            SaveIndicator::of(&SaveState::Idle, false, false, Some("15:42".into()), false).label(),
            "已保存 · 15:42"
        );
        assert_eq!(
            SaveIndicator::of(&SaveState::Saving, true, false, None, true),
            SaveIndicator::Waiting
        );
    }
}
