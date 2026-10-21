use crate::components::ui::Toaster;
use crate::models::note::{Note, NoteDraft};
use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum SaveState {
    Idle,
    Saving,
    Failed(String),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct EditorState {
    pub draft: Signal<NoteDraft>,
    pub saved: Signal<Option<Note>>,
    pub save: Signal<SaveState>,
    pub uploading: Signal<bool>,
    /// 发布、收录、恢复等明确操作的结果用提示条告知；自动保存只改页头状态。
    pub toast: Toaster,
    pub tags: Signal<String>,
    pub editor_epoch: Signal<u32>,
}

pub(super) fn use_editor_state(initial: Option<Note>, toast: Toaster) -> EditorState {
    let draft = initial.as_ref().map(Note::draft).unwrap_or_default();
    let tags = draft.tags.join(", ");
    EditorState {
        draft: use_signal(|| draft),
        saved: use_signal(|| initial),
        save: use_signal(|| SaveState::Idle),
        uploading: use_signal(|| false),
        toast,
        tags: use_signal(|| tags),
        editor_epoch: use_signal(|| 0),
    }
}

/// Advance only server identity/version when the user edited after submission.
pub(super) fn reconcile_save(current: &mut NoteDraft, submitted: &NoteDraft, saved: NoteDraft) {
    if current == submitted {
        *current = saved;
    } else {
        current.id = saved.id;
        current.expected_version = saved.expected_version;
    }
}

impl EditorState {
    pub fn dirty(self) -> bool {
        self.saved()
            .map(|n| n.draft() != self.draft())
            .unwrap_or_else(|| {
                !self.draft().content_md.is_empty() || !self.draft().title.is_empty()
            })
    }
    pub fn saved(self) -> Option<Note> {
        (self.saved)()
    }
    pub fn draft(self) -> NoteDraft {
        (self.draft)()
    }
    pub fn busy(self) -> bool {
        matches!(&*self.save.read(), SaveState::Saving)
    }
    pub fn error(self) -> String {
        match &*self.save.read() {
            SaveState::Failed(error) => error.clone(),
            _ => String::new(),
        }
    }
    pub fn fail(mut self, message: impl ToString) {
        self.save.set(SaveState::Failed(message.to_string()));
    }
    pub fn begin(mut self) -> bool {
        if self.busy() {
            return false;
        }
        if (self.uploading)() {
            self.fail("请先等待图片上传完成，或移除失败的图片");
            return false;
        }
        self.save.set(SaveState::Saving);
        true
    }
    pub fn restore_local(mut self, mut draft: NoteDraft) {
        if let Some(saved) = self.saved() {
            draft.id = Some(saved.id);
            draft.expected_version = Some(saved.version);
        }
        self.tags.set(draft.tags.join(", "));
        self.draft.set(draft);
        self.editor_epoch += 1;
    }
}

/// 页头保存状态：圆点 + 文字。`Failed` 整条可点重试。
#[derive(Clone, Debug, PartialEq)]
pub(super) enum SaveStatus {
    /// 新笔记还没有内容。
    Empty,
    /// 等待选择是否恢复本地副本。
    Recovering,
    Saving,
    Failed,
    /// 有修改，停止输入后自动保存。
    Dirty,
    Saved {
        version: i32,
    },
}

impl SaveStatus {
    pub fn of(
        saving: bool,
        failed: bool,
        dirty: bool,
        recovering: bool,
        saved: Option<i32>,
    ) -> Self {
        if recovering {
            Self::Recovering
        } else if saving {
            Self::Saving
        } else if failed {
            Self::Failed
        } else if dirty {
            Self::Dirty
        } else if let Some(version) = saved {
            Self::Saved { version }
        } else {
            Self::Empty
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Empty => "新笔记 · 尚未保存".into(),
            Self::Recovering => "等待选择本地副本".into(),
            Self::Saving => "正在保存…".into(),
            Self::Failed => "保存失败 · 点此重试".into(),
            Self::Dirty => "有未保存修改".into(),
            Self::Saved { version } => format!("工作稿 v{version} · 已保存"),
        }
    }

    /// 回归脚本按它等待保存完成。
    pub fn key(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Recovering => "recovering",
            Self::Saving => "saving",
            Self::Failed => "failed",
            Self::Dirty => "dirty",
            Self::Saved { .. } => "saved",
        }
    }

    pub fn tone(&self) -> &'static str {
        match self {
            Self::Failed => "danger",
            Self::Dirty | Self::Saving => "clay",
            _ => "muted",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_save_keeps_new_text_tags_and_notebooks_but_advances_version() {
        let submitted = NoteDraft {
            content_md: "first".into(),
            ..Default::default()
        };
        let mut current = NoteDraft {
            content_md: "new text".into(),
            tags: vec!["new".into()],
            notebook_ids: vec![3],
            ..submitted.clone()
        };
        reconcile_save(
            &mut current,
            &submitted,
            NoteDraft {
                id: Some(4),
                expected_version: Some(2),
                ..submitted.clone()
            },
        );
        assert_eq!(current.content_md, "new text");
        assert_eq!(current.tags, ["new"]);
        assert_eq!(current.notebook_ids, [3]);
        assert_eq!((current.id, current.expected_version), (Some(4), Some(2)));
    }
    #[test]
    fn save_status_prefers_recovery_then_saving_then_failure() {
        assert_eq!(
            SaveStatus::of(true, true, true, true, Some(3)),
            SaveStatus::Recovering
        );
        assert_eq!(
            SaveStatus::of(true, true, true, false, Some(3)),
            SaveStatus::Saving
        );
        assert_eq!(
            SaveStatus::of(false, true, true, false, Some(3)),
            SaveStatus::Failed
        );
        assert_eq!(
            SaveStatus::of(false, false, true, false, Some(3)),
            SaveStatus::Dirty
        );
        let saved = SaveStatus::of(false, false, false, false, Some(3));
        assert_eq!(saved.label(), "工作稿 v3 · 已保存");
        assert_eq!(
            SaveStatus::of(false, false, false, false, None),
            SaveStatus::Empty
        );
    }
    #[test]
    fn unchanged_save_accepts_server_normalization() {
        let submitted = NoteDraft {
            title: "  title  ".into(),
            ..Default::default()
        };
        let mut current = submitted.clone();
        let normalized = NoteDraft {
            title: "title".into(),
            id: Some(1),
            expected_version: Some(1),
            ..submitted.clone()
        };
        reconcile_save(&mut current, &submitted, normalized.clone());
        assert_eq!(current, normalized);
    }
}
