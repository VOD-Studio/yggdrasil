//! One serialized save pipeline for autosave, explicit actions and revision restoration.
//! Explicit actions report through toasts; plain saves only update the header status.
//!
//! 异步续写全部走 `try_*`：离开笔记（编辑器卸载）后迟到的响应不会写入任何状态，也不弹提示。
use super::state::{reconcile_save, EditorState, SaveState};
use crate::api::notes::{act_on_note, get_owned_note, save_note};
use crate::models::note::{Note, NoteAction};
use crate::utils::error::failure_message;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// 编辑器已卸载：流水线中途放弃，结果由 [`finish`] 丢弃。
fn closed() -> ServerFnError {
    ServerFnError::new("编辑器已关闭")
}

async fn save_snapshot(state: EditorState) -> Result<Note, ServerFnError> {
    let snapshot = state.draft_now().ok_or_else(closed)?;
    let saved = state.saved_now().ok_or_else(closed)?;
    let dirty = state.dirty_now().ok_or_else(closed)?;
    let note = match saved {
        Some(note) if !dirty => note,
        _ => save_note(snapshot.clone()).await?,
    };
    state
        .draft
        .try_update(|current| reconcile_save(current, &snapshot, note.draft()))
        .ok_or_else(closed)?;
    state.saved.try_set(Some(note.clone()));
    Ok(note)
}

fn finish(
    state: EditorState,
    result: Result<Note, ServerFnError>,
    message: Option<&str>,
    failure: &str,
) -> bool {
    match result {
        Ok(note) => {
            if state.saved.try_set(Some(note)).is_some() {
                return false;
            }
            let _ = state.save.try_set(SaveState::Idle);
            if let Some(message) = message {
                state.toast.success(message);
            }
            true
        }
        Err(error) => {
            // 编辑器已卸载时 `try_set` 失败，什么也不会发生。
            if state.save.try_get_untracked().is_some() {
                state.fail(failure_message(failure, &error));
            }
            false
        }
    }
}

/// False also means busy/uploading: callers must not continue to preview or restore.
pub(super) async fn persist(state: EditorState, action: Option<NoteAction>) -> bool {
    if !state.begin() {
        return false;
    }
    let result = async {
        let note = save_snapshot(state).await?;
        match action {
            Some(action) => act_on_note(note.id, note.version, action).await,
            None => Ok(note),
        }
    }
    .await;
    finish(
        state,
        result,
        action.map(|action| match action {
            NoteAction::Publish => "公开版本已更新",
            NoteAction::IncludeKnowledge => "已收录当前版本",
            NoteAction::Unpublish => "已撤回公开版本",
            NoteAction::ExcludeKnowledge => "已移出知识库",
            NoteAction::Trash => "已移入回收站",
            NoteAction::Restore => "已恢复为草稿",
        }),
        match action {
            None => "保存失败",
            Some(NoteAction::Publish) => "发布失败",
            Some(NoteAction::IncludeKnowledge) => "收录失败",
            Some(NoteAction::Unpublish) => "撤回失败",
            Some(NoteAction::ExcludeKnowledge) => "移出知识库失败",
            Some(NoteAction::Trash) => "移入回收站失败",
            Some(NoteAction::Restore) => "恢复失败",
        },
    )
}

pub(super) async fn restore_revision(state: EditorState, version: i32) {
    if !state.begin() {
        return;
    }
    let result = async {
        let note = save_snapshot(state).await?;
        if state.dirty_now().ok_or_else(closed)? {
            return Err(ServerFnError::new(
                "保存期间有新的编辑，已保留当前内容，请再次选择历史版本",
            ));
        }
        let before_fetch = state.draft_now().ok_or_else(closed)?;
        let old = get_owned_note(note.id, Some(version)).await?;
        // The user can keep typing while history loads. Never replace those edits.
        if state.draft_now().as_ref() != Some(&before_fetch) {
            return Err(ServerFnError::new(
                "恢复期间有新的编辑，已保留当前内容，请再次选择历史版本",
            ));
        }
        state.restore_local(old.draft());
        save_snapshot(state).await
    }
    .await;
    finish(
        state,
        result,
        Some("历史版本已恢复为新草稿"),
        "恢复历史版本失败",
    );
}

/// 工作稿停止输入后自动保存。每次状态变化都会让之前排队的保存失效（世代号），
/// 已经发出的请求不受影响。
pub(super) fn use_autosave(
    state: EditorState,
    recovery: RwSignal<Option<crate::models::note::NoteDraft>>,
) {
    let generation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        // 逐项求值，不让 `||` 短路漏掉追踪；`draft` 本身也要追踪：每次输入都重新计时。
        let busy = state.busy();
        let uploading = state.uploading.get();
        let failed = !state.error().is_empty();
        let recovering = recovery.with(Option::is_some);
        let dirty = state.dirty();
        state.draft.track();
        let blocked = busy || uploading || failed || recovering || !dirty;
        let Some(token) = generation.try_update_value(|g| {
            *g += 1;
            *g
        }) else {
            return;
        };
        if blocked {
            return;
        }
        spawn_local(async move {
            crate::utils::time::sleep_ms(1200).await;
            if generation.try_get_value() != Some(token) {
                return;
            }
            let still_due = state.draft_now().is_some()
                && !state.busy_now()
                && !state.uploading.try_get_untracked().unwrap_or(true)
                && !state.error_now()
                && recovery
                    .try_with_untracked(Option::is_none)
                    .unwrap_or(false)
                && state.dirty_now().unwrap_or(false);
            if still_due {
                persist(state, None).await;
            }
        });
    });
}
