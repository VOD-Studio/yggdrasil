//! Form data, the saved baseline and the editor lifecycle. Readiness has one state, so
//! a failed initialization cannot leave the page trapped under the loading skeleton.
use chrono::{DateTime, Utc};
use leptos::prelude::*;

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

#[derive(Clone, Copy)]
pub(super) struct WriteEditorState {
    /// 编辑中的文章；新文章为 None，首次保存后换到编辑路由。
    pub post_id: Option<i32>,
    pub form: RwSignal<PostDraft>,
    /// 最近一次与服务端一致的内容，form 与它不同即有未保存修改。
    pub saved: RwSignal<PostDraft>,
    pub saved_at: RwSignal<Option<String>>,
    pub published_at: RwSignal<Option<DateTime<Utc>>>,
    pub phase: RwSignal<EditorPhase>,
    pub save: RwSignal<SaveState>,
    pub library: LibraryResource,
    pub uploads_in_flight: RwSignal<UploadsInFlight>,
    pub upload_errors: RwSignal<Vec<UploadErrorEntry>>,
    pub cover_uploading: RwSignal<bool>,
    pub body_picker_visible: RwSignal<bool>,
    pub body_picker_uploading: RwSignal<bool>,
    pub(super) reload: RwSignal<u32>,
    /// 编辑器句柄持有 JS 实例与回调，不是 `Send + Sync`，只能放本地存储；
    /// 随页面所有者销毁时 drop，触发 JS 端 destroy。
    #[cfg(target_arch = "wasm32")]
    pub editor: StoredValue<Option<crate::bridges::tiptap::EditorHandle>, LocalStorage>,
}

impl WriteEditorState {
    pub fn retry(self) {
        self.reload.update(|n| *n += 1);
    }

    /// 非响应式读取与否取决于调用处：在 Effect/视图闭包里调用即订阅。
    pub fn dirty(self) -> bool {
        self.form
            .with(|form| self.saved.with(|saved| form != saved))
    }

    /// 保存或上传尚未完成，此时离开会丢内容。
    pub fn busy(self) -> bool {
        self.save.with(SaveState::is_saving)
            || self.uploads_in_flight.with(|u| u.uploading > 0)
            || self.cover_uploading.get()
            || self.body_picker_uploading.get()
    }

    /// 把正文换成给定 Markdown（起步模板、恢复本地副本）。
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
    pub fn set_content(self, markdown: &str) {
        #[cfg(target_arch = "wasm32")]
        self.editor.with_value(|handle| {
            if let Some(handle) = handle {
                let _ = handle.instance().set_markdown(markdown);
            }
        });
        self.form.update(|form| form.content = markdown.to_owned());
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
        draft.add_tags("架构，Leptos；");
        assert_eq!(draft.tags, "Rust, 架构, Leptos");
        draft.remove_tag("架构");
        assert_eq!(draft.parsed_tags(), ["Rust", "Leptos"]);
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
