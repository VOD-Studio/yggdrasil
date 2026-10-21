//! 自动保存工作稿；公开与知识库快照只由明确操作更新。
mod composer;
mod history;
mod metadata;
mod save;
mod state;

use super::note_drafts::use_draft_protection;
use super::notes::client_request;
use crate::api::notes::get_owned_note;
use crate::components::ui::{BTN_PRIMARY_SM, BTN_SECONDARY};
use crate::models::note::*;
use crate::router::Route;
use composer::NoteComposer;
use dioxus::prelude::*;
use metadata::MetadataPanel;
use save::persist;

#[component]
pub fn NewNote() -> Element {
    rsx! { NoteEditor { initial: None } }
}

#[component]
pub fn EditNote(id: i32) -> Element {
    rsx! {
        for key in std::iter::once(id) {
            LoadNoteEditor { key: "{key}", id: key }
        }
    }
}

#[component]
fn LoadNoteEditor(id: i32) -> Element {
    let response = use_resource(move || client_request(get_owned_note(id, None)));
    let data = response.read();
    match data.as_ref() {
        Some(Ok(note)) => rsx! {NoteEditor {initial:Some(note.clone())}},
        Some(Err(e)) => rsx! {p {class:"notes-error",role:"alert","{e}"}},
        None => rsx! {crate::pages::notes::NotesSkeleton {}},
    }
}

#[component]
fn NoteEditor(initial: Option<Note>) -> Element {
    let mut state = state::use_editor_state(initial);
    let editor_epoch = state.editor_epoch;
    let slot = state
        .draft()
        .id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "new".into());
    let (mut recovery, cache_failed) = use_draft_protection(slot, state.draft, move || {
        state.dirty() || state.busy() || (state.uploading)()
    });
    let mut preview = use_signal(|| false);
    save::use_autosave(state, recovery);

    let toggle_preview = move |_| async move {
        if persist(state, None).await {
            preview.toggle();
        }
    };
    let error = state.error();
    let deleted = state.saved().is_some_and(|n| n.deleted_at.is_some());
    rsx! {
        div {class:"notes-admin",
            div {class:"notes-admin-heading",
                div {
                    Link {class:"note-read",to:Route::AdminNotes {},"← 笔记管理"}
                    p {class:"notes-status",role:"status",aria_live:"polite","{state.status}" if state.dirty() {" · 有未保存修改"}}
                }
                div {class:"notes-admin-actions",
                    button {class:BTN_SECONDARY,disabled:state.busy() || deleted || recovery().is_some(),onclick:move |_|async move {persist(state,None).await;},"保存草稿"}
                    button {class:BTN_SECONDARY,disabled:state.busy() || state.saved().is_none() || recovery().is_some(),onclick: toggle_preview,if preview(){"返回编辑"}else{"预览"}}
                    button {class:BTN_PRIMARY_SM,disabled:state.busy() || deleted || recovery().is_some(),onclick:move |_|async move {persist(state,Some(NoteAction::Publish)).await;},
                        if state.saved().is_some_and(|n|n.published_version.is_some()) {"更新公开版"} else {"发布到前台"}
                    }
                }
            }
            if !state.error().is_empty() {p {class:"notes-error",role:"alert","{error}"}}
            if cache_failed(){p {class:"notes-error",role:"alert","浏览器无法保存本地恢复副本，请先保存到服务端再离开。"}}
            if let Some(local) = recovery() {
                div {class:"notes-error",role:"alert",
                    p {"此标签页留有未保存内容。恢复只修改工作稿，不会发布；若服务端已有新版本，请先核对内容。"}
                    button {class:BTN_SECONDARY,onclick:move |_| {
                        state.restore_local(local.clone());
                        recovery.set(None);
                    },"恢复未保存内容"}
                    button {class:"note-read",onclick:move |_|recovery.set(None),"使用服务端版本"}
                }
            }
            if recovery().is_some() {
                p {class:"notes-status","请选择恢复或使用服务端版本，再继续编辑。"}
            } else if deleted {
                p {class:"notes-error","这条笔记在回收站中。恢复后可继续编辑，公开状态与知识库收录需要重新确认。"}
                button {class:BTN_PRIMARY_SM,onclick:move |_|async move {persist(state,Some(NoteAction::Restore)).await;},"恢复笔记"}
            } else if preview() {
                // 预览复用公开渲染路径：完整 Note 转为 PublicNote（编辑器的工作稿
                // 版本号保留展示），显示字段不变，Markdown 不会进入公开渲染组件。
                if let Some(note)=state.saved() {crate::pages::notes::NoteDocument {key:"{note.version}",note:crate::models::public_note::PublicNote::from(note.clone()),preview:true}}
            } else {
                div {class:"notes-editor-grid",
                    div {class:"notes-editor-main",
                        input {aria_label:"笔记标题",placeholder:if state.draft().kind==NoteKind::Moment {"标题可选，先记下来…"} else {"给这篇笔记一个标题"},maxlength:200,
                            value:state.draft().title,oninput:move |ev|state.draft.with_mut(|d|d.title=ev.value())}
                        for epoch in std::iter::once(editor_epoch()) {
                            NoteComposer {key:"{epoch}",initial:state.draft().content_md,on_change:move |md|state.draft.with_mut(|d|d.content_md=md),uploading:state.uploading}
                        }
                    }
                    MetadataPanel { state }
                }
            }
        }
    }
}
