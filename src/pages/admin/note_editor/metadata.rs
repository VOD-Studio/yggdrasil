use super::{history::HistoryPanel, save::persist, state::EditorState};
use crate::api::notes::owned_notebooks;
use crate::components::forms::FormSelect;
use crate::components::ui::{Checkbox, BTN_SECONDARY};
use crate::models::note::*;
use crate::pages::admin::notes::client_request;
use crate::router::Route;
use dioxus::prelude::*;

#[component]
pub(super) fn MetadataPanel(mut state: EditorState) -> Element {
    let books = use_resource(|| client_request(owned_notebooks()));
    let mut tags = state.tags;
    let mut confirm_trash = use_signal(|| false);
    let navigator = navigator();
    rsx! {
    aside {class:"notes-editor-side",
        div {class:"notes-editor-field",label {r#for:"note-kind","记录方式"}
            FormSelect {
                id: Some("note-kind".to_string()),
                value: state.draft().kind,
                options: vec![(NoteKind::Moment, "随记"), (NoteKind::Topic, "主题笔记")],
                onchange: move |kind: NoteKind| state.draft.with_mut(|d| d.kind = kind),
            }
        }
        div {class:"notes-editor-field",label {r#for:"note-tags","标签"} input {id:"note-tags",r#type:"text",placeholder:"Rust, 排错, 读书",value:"{tags}",oninput:move |ev| {
            let value=ev.value();state.draft.with_mut(|d|d.tags=value.split([',','，']).map(str::trim).filter(|s|!s.is_empty()).map(str::to_string).collect());tags.set(value);
        }}}
        fieldset {class:"notes-editor-section notes-editor-notebooks",legend {"收录到笔记本"}
            if let Some(Ok(items))=books.read().as_ref() {
                for book in items {label {class:"notebook-check",key:"{book.id}",
                    Checkbox {checked:state.draft().notebook_ids.contains(&book.id),onchange:{let id=book.id;move |checked|state.draft.with_mut(|d| {d.notebook_ids.retain(|b|*b!=id);if checked{d.notebook_ids.push(id);d.notebook_ids.sort_unstable();}})}} "{book.title}" if book.archived_at.is_some(){"（已归档）"}
                }}
                if items.is_empty() {Link {to:Route::AdminNotebooks {},"创建第一个笔记本 →"}}
            }
        }
        div {class:"notes-editor-section notes-editor-knowledge",h2 {"AI 知识库"}
            p {"收录后，获得授权的 AI 可以读取这一版内容。无需公开发布。"}
            if let Some(version)=state.saved().and_then(|n|n.knowledge_version) {p {"当前收录：v{version}"}}
            button {class:BTN_SECONDARY,disabled:state.busy(),onclick:move |_|async move {persist(state,Some(NoteAction::IncludeKnowledge)).await;},"收录当前版本"}
            if state.saved().is_some_and(|n|n.knowledge_version.is_some()) {button {class:"note-read",disabled:state.busy(),onclick:move |_|async move {persist(state,Some(NoteAction::ExcludeKnowledge)).await;},"移出知识库"}}
        }
        if let Some(note)=state.saved() {
            div {class:"notes-editor-section notes-editor-public",h2 {"公开状态"}
                if let Some(version)=note.published_version {p {"已公开 v{version}，草稿修改不会自动发布。"}
                    div {class:"notes-editor-inline-actions",
                        a {class:"note-read",href:format!("/notes/{}",note.slug),target:"_blank",rel:"noopener","查看公开页面 ↗"}
                        button {class:"note-read",disabled:state.busy(),onclick:move |_|async move {persist(state,Some(NoteAction::Unpublish)).await;},"撤回公开版"}
                    }
                } else {p {"仅自己可见。"}}
            }
            HistoryPanel { state }
            div {class:"notes-editor-section notes-editor-danger",
                if confirm_trash() {
                    p {"移入回收站将撤回公开版并移出知识库，之后可以恢复。"}
                    button {class:BTN_SECONDARY,disabled:state.busy(),onclick:move |_|async move {if persist(state, Some(NoteAction::Trash)).await { navigator.push(Route::AdminNotes {}); }},"确认移入回收站"}
                    button {class:"note-read",onclick:move |_|confirm_trash.set(false),"取消"}
                } else {button {class:"note-read",onclick:move |_|confirm_trash.set(true),"移入回收站"}}
            }
        }
    }
    }
}
