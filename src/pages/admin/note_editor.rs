//! 自动保存工作稿；公开与知识库快照只由明确操作更新。
use super::note_drafts::use_draft_protection;
use super::notes::client_request;
use crate::api::notes::*;
use crate::components::forms::FormSelect;
use crate::components::ui::{Checkbox, BTN_PRIMARY_SM, BTN_SECONDARY};
use crate::models::note::*;
use crate::router::Route;
use dioxus::prelude::*;

#[derive(Clone, Copy)]
struct EditorState {
    draft: Signal<NoteDraft>,
    saved: Signal<Option<Note>>,
    busy: Signal<bool>,
    uploading: Signal<bool>,
    error: Signal<String>,
    status: Signal<String>,
}

impl EditorState {
    fn dirty(self) -> bool {
        self.saved()
            .map(|n| n.draft() != self.draft())
            .unwrap_or_else(|| {
                !self.draft().content_md.is_empty() || !self.draft().title.is_empty()
            })
    }
    fn saved(self) -> Option<Note> {
        (self.saved)()
    }
    fn draft(self) -> NoteDraft {
        (self.draft)()
    }
}

async fn persist(mut state: EditorState, action: Option<NoteAction>) {
    if (state.busy)() {
        return;
    }
    if (state.uploading)() {
        state
            .error
            .set("请先等待图片上传完成，或移除失败的图片".into());
        return;
    }
    state.busy.set(true);
    state.error.set(String::new());
    state.status.set("正在保存…".into());
    let snapshot = state.draft();
    let result = async {
        let note = if state.dirty() || state.saved().is_none() {
            save_note(snapshot.clone()).await?
        } else {
            state.saved().unwrap()
        };
        // 保存期间仍可输入，仅推进 ID / 版本，绝不覆盖新的文字。
        if state.draft() == snapshot {
            state.draft.set(note.draft());
        } else {
            state.draft.with_mut(|d| {
                d.id = Some(note.id);
                d.expected_version = Some(note.version);
            });
        }
        state.saved.set(Some(note.clone()));
        if let Some(action) = action {
            act_on_note(note.id, note.version, action).await
        } else {
            Ok(note)
        }
    }
    .await;
    match result {
        Ok(note) => {
            state.saved.set(Some(note));
            state.status.set(
                match action {
                    Some(NoteAction::Publish) => "公开版本已更新",
                    Some(NoteAction::IncludeKnowledge) => "已收录当前版本",
                    Some(NoteAction::Unpublish) => "已撤回公开版本",
                    Some(NoteAction::ExcludeKnowledge) => "已移出知识库",
                    Some(NoteAction::Trash) => "已移入回收站",
                    _ => "草稿已保存",
                }
                .into(),
            );
        }
        Err(e) => {
            state.error.set(e.to_string());
            state.status.set("保存未完成".into());
        }
    }
    state.busy.set(false);
}

#[component]
pub fn NewNote() -> Element {
    rsx! { NoteEditor { initial: None } }
}

#[component]
pub fn EditNote(id: i32) -> Element {
    rsx! {for key in std::iter::once(id) {LoadNoteEditor {key:"{key}",id:key}}}
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
    let mut state = EditorState {
        draft: use_signal(|| initial.as_ref().map(Note::draft).unwrap_or_default()),
        saved: use_signal(|| initial.clone()),
        busy: use_signal(|| false),
        uploading: use_signal(|| false),
        error: use_signal(String::new),
        status: use_signal(|| "自动保存工作稿".to_string()),
    };
    let mut editor_epoch = use_signal(|| 0_u32);
    let mut tags = use_signal(|| state.draft().tags.join(", "));
    let slot = state
        .draft()
        .id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "new".into());
    let (mut recovery, cache_failed) = use_draft_protection(slot, state.draft, move || {
        state.dirty() || (state.busy)() || (state.uploading)()
    });
    let books = use_resource(|| client_request(owned_notebooks()));
    let mut preview = use_signal(|| false);
    let mut revisions = use_signal(Vec::<NoteRevision>::new);
    let mut show_history = use_signal(|| false);
    let mut confirm_trash = use_signal(|| false);
    let navigator = navigator();

    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let snapshot = state.draft();
        let busy = (state.busy)();
        let uploading = (state.uploading)();
        let failed = !(state.error)().is_empty();
        if busy || uploading || failed || recovery().is_some() || !state.dirty() {
            return;
        }
        spawn(async move {
            crate::utils::time::sleep_ms(1200).await;
            if state.draft() == snapshot
                && !(state.busy)()
                && !(state.uploading)()
                && state.dirty()
                && (state.error)().is_empty()
                && recovery().is_none()
            {
                persist(state, None).await;
            }
        });
    });

    let deleted = state.saved().is_some_and(|n| n.deleted_at.is_some());
    rsx! {
        div {class:"notes-admin",
            div {class:"notes-admin-heading",
                div {
                    Link {class:"note-read",to:Route::AdminNotes {},"← 笔记管理"}
                    p {class:"notes-status",role:"status",aria_live:"polite","{state.status}" if state.dirty() {" · 有未保存修改"}}
                }
                div {class:"notes-admin-actions",
                    button {class:BTN_SECONDARY,disabled:(state.busy)() || deleted || recovery().is_some(),onclick:move |_|async move {persist(state,None).await;},"保存草稿"}
                    button {class:BTN_SECONDARY,disabled:(state.busy)() || state.saved().is_none() || recovery().is_some(),onclick:move |_| {spawn(async move {persist(state,None).await;if (state.error)().is_empty(){preview.set(!preview());}});},if preview(){"返回编辑"}else{"预览"}}
                    button {class:BTN_PRIMARY_SM,disabled:(state.busy)() || deleted || recovery().is_some(),onclick:move |_|async move {persist(state,Some(NoteAction::Publish)).await;},
                        if state.saved().is_some_and(|n|n.published_version.is_some()) {"更新公开版"} else {"发布到前台"}
                    }
                }
            }
            if !(state.error)().is_empty() {p {class:"notes-error",role:"alert","{state.error}"}}
            if cache_failed(){p {class:"notes-error",role:"alert","浏览器无法保存本地恢复副本，请先保存到服务端再离开。"}}
            if let Some(local) = recovery() {
                div {class:"notes-error",role:"alert",
                    p {"此标签页留有未保存内容。恢复只修改工作稿，不会发布；若服务端已有新版本，请先核对内容。"}
                    button {class:BTN_SECONDARY,onclick:move |_| {
                        let mut draft=local.clone();
                        if let Some(saved)=state.saved(){draft.id=Some(saved.id);draft.expected_version=Some(saved.version);}
                        tags.set(draft.tags.join(", "));state.draft.set(draft);editor_epoch+=1;recovery.set(None);
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
                if let Some(note)=state.saved() {crate::pages::notes::NoteDocument {key:"{note.version}",note,preview:true}}
            } else {
                div {class:"notes-editor-grid",
                    div {class:"notes-editor-main",
                        input {aria_label:"笔记标题",placeholder:if state.draft().kind==NoteKind::Moment {"标题可选，先记下来…"} else {"给这篇笔记一个标题"},maxlength:200,
                            value:state.draft().title,oninput:move |ev|state.draft.with_mut(|d|d.title=ev.value())}
                        for epoch in std::iter::once(editor_epoch()) {
                            NoteComposer {key:"{epoch}",initial:state.draft().content_md,on_change:move |md|state.draft.with_mut(|d|d.content_md=md),uploading:state.uploading}
                        }
                    }
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
                            button {class:BTN_SECONDARY,disabled:(state.busy)(),onclick:move |_|async move {persist(state,Some(NoteAction::IncludeKnowledge)).await;},"收录当前版本"}
                            if state.saved().is_some_and(|n|n.knowledge_version.is_some()) {button {class:"note-read",disabled:(state.busy)(),onclick:move |_|async move {persist(state,Some(NoteAction::ExcludeKnowledge)).await;},"移出知识库"}}
                        }
                        if let Some(note)=state.saved() {
                            div {class:"notes-editor-section notes-editor-public",h2 {"公开状态"}
                                if let Some(version)=note.published_version {p {"已公开 v{version}，草稿修改不会自动发布。"}
                                    div {class:"notes-editor-inline-actions",
                                        a {class:"note-read",href:format!("/notes/{}",note.slug),target:"_blank",rel:"noopener","查看公开页面 ↗"}
                                        button {class:"note-read",disabled:(state.busy)(),onclick:move |_|async move {persist(state,Some(NoteAction::Unpublish)).await;},"撤回公开版"}
                                    }
                                } else {p {"仅自己可见。"}}
                            }
                            div {class:"notes-editor-section notes-editor-history-section",
                                button {class:"note-read notes-editor-history-toggle",onclick:move |_| {show_history.set(!show_history());if let Some(n)=state.saved(){spawn(async move {match note_history(n.id).await {Ok(items)=>revisions.set(items),Err(e)=>state.error.set(e.to_string())}});}},"历史版本"}
                                if show_history() {
                                    p {"恢复会生成新草稿，保留现有历史。"}
                                    div {class:"notes-editor-history",
                                        for revision in revisions() {
                                            button {disabled:(state.busy)(),onclick:move |_| {
                                                if let Some(current)=state.saved() {spawn(async move {
                                                    persist(state,None).await;
                                                    if !(state.error)().is_empty(){return;}
                                                    match get_owned_note(current.id,Some(revision.version)).await {
                                                        Ok(old)=>{let mut draft=old.draft();draft.expected_version=state.saved().map(|n|n.version);tags.set(draft.tags.join(", "));state.draft.set(draft);editor_epoch+=1;persist(state,None).await;},
                                                        Err(e)=>state.error.set(e.to_string()),
                                                    }
                                                });}
                                            },{format!("v{} · {} · 恢复",revision.version,revision.created_at.format("%m-%d %H:%M"))}}
                                        }
                                    }
                                }
                            }
                            div {class:"notes-editor-section notes-editor-danger",
                                if confirm_trash() {
                                    p {"移入回收站将撤回公开版并移出知识库，之后可以恢复。"}
                                    button {class:BTN_SECONDARY,disabled:(state.busy)(),onclick:move |_|async move {persist(state,Some(NoteAction::Trash)).await;if (state.error)().is_empty(){navigator.push(Route::AdminNotes {});}},"确认移入回收站"}
                                    button {class:"note-read",onclick:move |_|confirm_trash.set(false),"取消"}
                                } else {button {class:"note-read",onclick:move |_|confirm_trash.set(true),"移入回收站"}}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 富文本编辑与笔记专用的受保护图片上传。
#[component]
fn NoteComposer(
    initial: String,
    on_change: EventHandler<String>,
    mut uploading: Signal<bool>,
) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        use crate::bridges::library::{library_ready, use_browser_library};
        use crate::bridges::tiptap::{
            self, EditorHandle, EditorOptions, UploadErrorEntry, UploadsInFlight,
        };
        use wasm_bindgen::prelude::*;
        let library = use_browser_library("tiptap", || true);
        let mut editor = use_signal(|| None::<EditorHandle>);
        let mut ready = use_signal(|| false);
        let mut error = use_signal(String::new);
        let counts = use_signal(UploadsInFlight::default);
        let errors = use_signal(Vec::<UploadErrorEntry>::new);
        let mut pick = use_signal(|| false);
        let picking_upload = use_signal(|| false);
        use_drop(move || editor.set(None));
        use_effect(move || {
            uploading.set(counts().uploading > 0 || counts().error > 0 || picking_upload())
        });
        use_effect(move || {
            if editor.peek().is_some() || !library_ready(library) {
                return;
            }
            let update = Closure::new(move |md: String| on_change.call(md));
            let on_ready = Closure::new(move || ready.set(true));
            let upload = Closure::new(move |file: web_sys::File| {
                wasm_bindgen_futures::future_to_promise(async move {
                    tiptap::wasm::upload_note_image_file(file)
                        .await
                        .map(|s| JsValue::from_str(&s))
                        .map_err(|e| JsValue::from_str(&e))
                })
            });
            let upload_event = Closure::new(move |ev: tiptap::UploadEventJs| {
                tiptap::consume_upload_event(&ev, counts, errors)
            });
            let run = tiptap::make_run_code_closure();
            let picker = Closure::new(move || pick.set(true));
            let opts = EditorOptions::new();
            opts.set_placeholder("从一个想法开始。输入 / 插入代码、图片、公式或引用…");
            opts.set_on_update(&update);
            opts.set_on_ready(&on_ready);
            opts.set_on_image_upload(&upload);
            opts.set_on_upload_event(&upload_event);
            opts.set_on_run_code(&run);
            opts.set_on_pick_from_library(&picker);
            match tiptap::get_module().create("note-tiptap", &opts) {
                Ok(Some(inst)) => {
                    inst.set_markdown(&initial);
                    editor.set(Some(EditorHandle::new(
                        inst,
                        update,
                        upload,
                        on_ready,
                        upload_event,
                        run,
                        picker,
                    )));
                }
                _ => error.set("编辑器初始化失败，请刷新后重试".into()),
            }
        });
        return rsx! {
            div {class:"notes-editor-surface",
                crate::bridges::library::LibraryLoadError {library}
                if !ready() {p {role:"status","正在加载编辑器…"}}
                if !error().is_empty() {p {class:"notes-error",role:"alert","{error}"}}
                for item in errors() {p {class:"notes-error",key:"{item.id}","{item.message}"
                    button {class:"note-read",onclick:move |_| {if let Some(handle)=editor.peek().as_ref(){handle.instance().remove_upload_by_upload_id(&item.id);}},"移除失败图片"}
                }}
                div {id:"note-tiptap"}
            }
            if pick() {
                p {class:"notes-status","素材库中的图片原本就是公开素材。需要私密图片时，请在编辑器中上传。"}
                crate::components::assets::AssetPickerModal {visible:pick,cover_uploading:picking_upload,title:"选择公开素材",multi:true,on_select:move |items:Vec<crate::components::assets::AssetSelection>| {
                    if let Some(handle)=editor.peek().as_ref(){if let Ok(json)=serde_json::to_string(&items){handle.instance().insert_images_from_library(&json);}}
                    pick.set(false);
                }}
            }
        };
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (initial, on_change, uploading);
        rsx! {div {class:"notes-editor-surface",p {role:"status","正在加载编辑器…"}}}
    }
}
