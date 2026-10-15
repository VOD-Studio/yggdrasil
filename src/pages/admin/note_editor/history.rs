use super::{save::restore_revision, state::EditorState};
use crate::api::notes::note_history;
use crate::models::note::NoteRevision;
use dioxus::prelude::*;

#[component]
pub(super) fn HistoryPanel(state: EditorState) -> Element {
    let mut revisions = use_signal(Vec::<NoteRevision>::new);
    let mut open = use_signal(|| false);
    let mut error = use_signal(String::new);
    let toggle = move |_| {
        open.toggle();
        if open() {
            if let Some(note) = state.saved() {
                spawn(async move {
                    error.set(String::new());
                    match note_history(note.id).await {
                        Ok(items) => revisions.set(items),
                        Err(e) => error.set(e.to_string()),
                    }
                });
            }
        }
    };
    rsx! {
        div { class: "notes-editor-section notes-editor-history-section",
            button { class: "note-read notes-editor-history-toggle", onclick: toggle, "历史版本" }
            if open() {
                p { "恢复会生成新草稿，保留现有历史。" }
                if !error().is_empty() { p { class: "notes-error", role: "alert", "{error}" } }
                div { class: "notes-editor-history",
                    for revision in revisions() {
                        button {
                            disabled: state.busy(),
                            onclick: move |_| async move { restore_revision(state, revision.version).await; },
                            {format!("v{} · {} · 恢复", revision.version, revision.created_at.format("%m-%d %H:%M"))}
                        }
                    }
                }
            }
        }
    }
}
