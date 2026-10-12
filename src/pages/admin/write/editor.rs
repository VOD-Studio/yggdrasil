//! Loading, initialization and cleanup of the article editor.
use super::state::{EditorPhase, PostDraft, SaveState, WriteEditorState};
use crate::bridges::library::use_browser_library;
use crate::bridges::tiptap::UploadsInFlight;
use dioxus::prelude::*;

/// Owns Tiptap callbacks and its handle for exactly the lifetime of the write page.
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn use_write_editor(post_id: Option<i32>) -> WriteEditorState {
    let state = WriteEditorState {
        form: use_signal(PostDraft::default),
        phase: use_signal(|| EditorPhase::Loading),
        save: use_signal(SaveState::default),
        library: use_browser_library("tiptap", || true),
        uploads_in_flight: use_signal(UploadsInFlight::default),
        upload_errors: use_signal(Vec::new),
        cover_uploading: use_signal(|| false),
        body_picker_visible: use_signal(|| false),
        body_picker_uploading: use_signal(|| false),
        reload: use_signal(|| 0),
        #[cfg(target_arch = "wasm32")]
        editor: use_signal(|| None),
    };
    #[cfg(target_arch = "wasm32")]
    {
        use crate::api::posts::{get_post_by_id, SinglePostResponse};
        use crate::bridges::library::library_ready;
        use crate::bridges::tiptap::{consume_upload_event, EditorHandle};
        use wasm_bindgen::closure::Closure;

        let mut form = state.form;
        let mut phase = state.phase;
        let mut editor = state.editor;
        let library = state.library;
        let reload = state.reload;
        // Keep the loaded Markdown separately from the editable form. Form updates
        // cannot race initialization or cause an editor instance to be recreated.
        let mut initial_content = use_signal(|| None::<String>);
        let mut ready = use_signal(|| false);
        use_effect(move || {
            let generation = reload();
            ready.set(false);
            initial_content.set(None);
            editor.set(None);
            phase.set(EditorPhase::Loading);
            if let Some(id) = post_id {
                spawn(async move {
                    let result = get_post_by_id(id).await;
                    if *reload.peek() != generation {
                        return;
                    }
                    match result {
                        Ok(SinglePostResponse { post: Some(post) }) => {
                            let draft = PostDraft::from(post);
                            let markdown = draft.content.clone();
                            form.set(draft);
                            initial_content.set(Some(markdown));
                            phase.set(EditorPhase::Initializing);
                        }
                        Ok(SinglePostResponse { post: None }) => {
                            phase.set(EditorPhase::Failed("文章不存在".into()));
                        }
                        Err(error) => {
                            phase.set(EditorPhase::Failed(format!("加载失败: {error}")));
                        }
                    }
                });
            } else {
                // Retain a new article's form when retrying failed initialization.
                initial_content.set(Some(form.peek().content.clone()));
                phase.set(EditorPhase::Initializing);
            }
        });
        use_drop(move || editor.set(None));
        use_effect(move || {
            let Some(markdown) = initial_content() else {
                return;
            };
            if editor.peek().is_some() || !library_ready(library) {
                return;
            }
            let on_update = Closure::new(move |md: String| form.write().content = md);
            let on_ready = Closure::new(move || ready.set(true));
            let on_image_upload = crate::bridges::tiptap::make_upload_closure();
            let on_upload_event =
                Closure::new(move |event: crate::bridges::tiptap::UploadEventJs| {
                    consume_upload_event(&event, state.uploads_in_flight, state.upload_errors);
                });
            let on_run_code = crate::bridges::tiptap::make_run_code_closure();
            let on_pick_from_library = Closure::new({
                let mut visible = state.body_picker_visible;
                move || visible.set(true)
            });
            let options = crate::bridges::tiptap::EditorOptions::new();
            options.set_placeholder("在此输入内容...");
            options.set_on_update(&on_update);
            options.set_on_ready(&on_ready);
            options.set_on_image_upload(&on_image_upload);
            options.set_on_upload_event(&on_upload_event);
            options.set_on_run_code(&on_run_code);
            options.set_on_pick_from_library(&on_pick_from_library);
            match crate::bridges::tiptap::get_module().create("tiptap-editor", &options) {
                Ok(Some(instance)) => {
                    if !markdown.is_empty() {
                        instance.set_markdown(&markdown);
                    }
                    editor.set(Some(EditorHandle::new(
                        instance,
                        on_update,
                        on_image_upload,
                        on_ready,
                        on_upload_event,
                        on_run_code,
                        on_pick_from_library,
                    )));
                }
                Ok(None) => phase.set(EditorPhase::Failed("编辑器容器未就绪".into())),
                Err(error) => {
                    phase.set(EditorPhase::Failed(format!("编辑器初始化错误: {error:?}")))
                }
            }
        });
        use_effect(move || {
            if ready() {
                phase.set(EditorPhase::Ready);
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut phase = state.phase;
        use_effect(move || phase.set(EditorPhase::Ready));
    }
    state
}
