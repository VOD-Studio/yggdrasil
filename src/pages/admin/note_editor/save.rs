//! One serialized save pipeline for autosave, explicit actions and revision restoration.
//! Explicit actions report through toasts; plain saves only update the header status.
use super::state::{reconcile_save, EditorState, SaveState};
use crate::api::notes::{act_on_note, get_owned_note, save_note};
use crate::models::note::{Note, NoteAction, NoteDraft};
use dioxus::prelude::*;

async fn save_snapshot(mut state: EditorState) -> Result<Note, ServerFnError> {
    let snapshot = state.draft();
    let note = if state.dirty() || state.saved().is_none() {
        save_note(snapshot.clone()).await?
    } else {
        state.saved().expect("saved note was checked")
    };
    state
        .draft
        .with_mut(|current| reconcile_save(current, &snapshot, note.draft()));
    state.saved.set(Some(note.clone()));
    Ok(note)
}

fn finish(
    mut state: EditorState,
    result: Result<Note, ServerFnError>,
    message: Option<&str>,
) -> bool {
    match result {
        Ok(note) => {
            state.saved.set(Some(note));
            state.save.set(SaveState::Idle);
            if let Some(message) = message {
                state.toast.success(message);
            }
            true
        }
        Err(error) => {
            state.fail(error);
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
    )
}

pub(super) async fn restore_revision(state: EditorState, version: i32) {
    if !state.begin() {
        return;
    }
    let result = async {
        let note = save_snapshot(state).await?;
        if state.dirty() {
            return Err(ServerFnError::new(
                "保存期间有新的编辑，已保留当前内容，请再次选择历史版本",
            ));
        }
        let before_fetch = state.draft();
        let old = get_owned_note(note.id, Some(version)).await?;
        // The user can keep typing while history loads. Never replace those edits.
        if state.draft() != before_fetch {
            return Err(ServerFnError::new(
                "恢复期间有新的编辑，已保留当前内容，请再次选择历史版本",
            ));
        }
        state.restore_local(old.draft());
        save_snapshot(state).await
    }
    .await;
    finish(state, result, Some("历史版本已恢复为新草稿"));
}

pub(super) fn use_autosave(state: EditorState, recovery: Signal<Option<NoteDraft>>) {
    #[cfg(target_arch = "wasm32")]
    {
        let pending =
            use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(None::<dioxus::core::Task>)));
        use_effect(move || {
            if let Some(task) = pending.borrow_mut().take() {
                task.cancel();
            }
            let snapshot = state.draft();
            if state.busy()
                || (state.uploading)()
                || !state.error().is_empty()
                || recovery().is_some()
                || !state.dirty()
            {
                return;
            }
            *pending.borrow_mut() = Some(spawn(async move {
                crate::utils::time::sleep_ms(1200).await;
                if state.draft() == snapshot
                    && !state.busy()
                    && !(state.uploading)()
                    && state.error().is_empty()
                    && recovery().is_none()
                    && state.dirty()
                {
                    // Separate debounce from the request: a state update must not cancel
                    // an in-flight save. Both tasks remain owned by this component scope.
                    spawn(async move {
                        persist(state, None).await;
                    });
                }
            }));
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (state, recovery);
}
