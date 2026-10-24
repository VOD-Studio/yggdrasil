use crate::components::ui::Toaster;
use crate::models::note::{Note, NoteDraft};
use leptos::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum SaveState {
    Idle,
    Saving,
    Failed(String),
}

/// 编辑器的全部可变状态。字段是 `Copy` 的信号句柄；编辑器卸载后异步回调里的写入
/// 一律走 `try_*`，迟到的响应因此不会落进下一个编辑器。
#[derive(Clone, Copy)]
pub(super) struct EditorState {
    pub draft: RwSignal<NoteDraft>,
    pub saved: RwSignal<Option<Note>>,
    pub save: RwSignal<SaveState>,
    pub uploading: RwSignal<bool>,
    /// 发布、收录、恢复等明确操作的结果用提示条告知；自动保存只改页头状态。
    pub toast: Toaster,
    pub tags: RwSignal<String>,
    pub editor_epoch: RwSignal<u32>,
}

pub(super) fn use_editor_state(initial: Option<Note>, toast: Toaster) -> EditorState {
    let draft = initial.as_ref().map(Note::draft).unwrap_or_default();
    let tags = draft.tags.join(", ");
    EditorState {
        draft: RwSignal::new(draft),
        saved: RwSignal::new(initial),
        save: RwSignal::new(SaveState::Idle),
        uploading: RwSignal::new(false),
        toast,
        tags: RwSignal::new(tags),
        editor_epoch: RwSignal::new(0),
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

/// 工作稿相对已保存版本是否有改动；新笔记只要有标题或正文就算有改动。
fn is_dirty(saved: &Option<Note>, draft: &NoteDraft) -> bool {
    match saved {
        Some(note) => note.draft() != *draft,
        None => !draft.content_md.is_empty() || !draft.title.is_empty(),
    }
}

impl EditorState {
    /// 响应式读取，供视图、memo 与 effect 使用。
    pub fn dirty(self) -> bool {
        self.saved
            .with(|saved| self.draft.with(|draft| is_dirty(saved, draft)))
    }
    pub fn busy(self) -> bool {
        self.save.with(|save| matches!(save, SaveState::Saving))
    }
    pub fn error(self) -> String {
        self.save.with(|save| match save {
            SaveState::Failed(error) => error.clone(),
            _ => String::new(),
        })
    }

    /// 以下为异步路径用的非响应式读取：编辑器已卸载时返回 `None`。
    pub fn dirty_now(self) -> Option<bool> {
        self.saved
            .try_with_untracked(|saved| {
                self.draft
                    .try_with_untracked(|draft| is_dirty(saved, draft))
            })
            .flatten()
    }
    pub fn saved_now(self) -> Option<Option<Note>> {
        self.saved.try_get_untracked()
    }
    pub fn draft_now(self) -> Option<NoteDraft> {
        self.draft.try_get_untracked()
    }
    pub fn busy_now(self) -> bool {
        self.save
            .try_with_untracked(|save| matches!(save, SaveState::Saving))
            .unwrap_or(false)
    }
    pub fn error_now(self) -> bool {
        self.save
            .try_with_untracked(|save| matches!(save, SaveState::Failed(_)))
            .unwrap_or(false)
    }

    pub fn fail(self, message: impl ToString) {
        let _ = self.save.try_set(SaveState::Failed(message.to_string()));
    }
    pub fn begin(self) -> bool {
        if self
            .save
            .with_untracked(|save| matches!(save, SaveState::Saving))
        {
            return false;
        }
        if self.uploading.get_untracked() {
            self.fail("请先等待图片上传完成，或移除失败的图片");
            return false;
        }
        self.save.set(SaveState::Saving);
        true
    }
    pub fn restore_local(self, mut draft: NoteDraft) {
        if let Some(Some(saved)) = self.saved_now() {
            draft.id = Some(saved.id);
            draft.expected_version = Some(saved.version);
        }
        let _ = self.tags.try_set(draft.tags.join(", "));
        let _ = self.draft.try_set(draft);
        let _ = self
            .editor_epoch
            .try_update(|epoch| *epoch = epoch.wrapping_add(1));
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
