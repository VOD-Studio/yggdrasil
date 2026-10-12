//! Form data and the editor lifecycle. Readiness has one state, so failed loads cannot
//! leave the page trapped under the loading skeleton.
use dioxus::prelude::*;

use crate::bridges::library::LibraryResource;
use crate::bridges::tiptap::{UploadErrorEntry, UploadsInFlight};
use crate::models::post::Post;

#[derive(Clone, Debug, PartialEq)]
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
            status: "published".into(),
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
    Loading,
    Initializing,
    Ready,
    Failed(String),
}

impl EditorPhase {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading | Self::Initializing)
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
    Failed(String),
}

impl SaveState {
    pub fn is_saving(&self) -> bool {
        matches!(self, Self::Saving)
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct WriteEditorState {
    pub form: Signal<PostDraft>,
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
}
