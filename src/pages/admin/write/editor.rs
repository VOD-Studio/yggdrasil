//! Initialization and cleanup of the article editor. The post is loaded before this
//! hook runs, so the form starts from server content and the local copy can compare
//! against it.
use super::state::{EditorPhase, PostDraft, SaveState, WriteEditorState};
use crate::bridges::library::use_browser_library;
use crate::bridges::tiptap::UploadsInFlight;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;

/// 正文为空时编辑器里的提示。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const PLACEHOLDER: &str = "开始写作，或输入 / 插入代码、图表、公式和图片";

/// Owns Tiptap callbacks and its handle for exactly the lifetime of the write page.
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn use_write_editor(
    post_id: Option<i32>,
    initial: PostDraft,
    published_at: Option<DateTime<Utc>>,
) -> WriteEditorState {
    let baseline = initial.clone();
    let state = WriteEditorState {
        post_id,
        form: use_signal(move || initial),
        saved: use_signal(move || baseline),
        saved_at: use_signal(|| None),
        published_at: use_signal(move || published_at),
        phase: use_signal(|| EditorPhase::Initializing),
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
        use crate::bridges::library::library_ready;
        use crate::bridges::tiptap::{consume_upload_event, EditorHandle};
        use wasm_bindgen::closure::Closure;

        let mut form = state.form;
        let mut saved = state.saved;
        let mut phase = state.phase;
        let mut editor = state.editor;
        let library = state.library;
        let reload = state.reload;
        let mut ready = use_signal(|| false);
        use_drop(move || editor.set(None));
        use_effect(move || {
            let _ = reload();
            if !library_ready(library) {
                return;
            }
            ready.set(false);
            editor.set(None);
            phase.set(EditorPhase::Initializing);
            // Retrying keeps whatever was typed before the failure.
            let markdown = form.peek().content.clone();
            let on_update = Closure::new(move |md: String| form.write().content = md);
            let on_check_roundtrip = crate::bridges::tiptap::make_roundtrip_check_closure();
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
            options.set_placeholder(PLACEHOLDER);
            options.set_on_update(&on_update);
            options.set_on_image_upload(&on_image_upload);
            options.set_on_upload_event(&on_upload_event);
            options.set_on_run_code(&on_run_code);
            options.set_on_pick_from_library(&on_pick_from_library);
            options.set_check_roundtrip(&on_check_roundtrip);
            match crate::bridges::tiptap::get_module().create("tiptap-editor", &options) {
                Ok(Some(instance)) => {
                    let settled = (!markdown.is_empty()).then(|| instance.set_markdown(&markdown));
                    editor.set(Some(
                        EditorHandle::new(
                            instance,
                            on_update,
                            on_image_upload,
                            // 写作页的就绪以 setMarkdown 往返检查结束为准，不监听 onReady
                            Closure::new(|| {}),
                            on_upload_event,
                            on_run_code,
                            on_pick_from_library,
                        )
                        .with_roundtrip_check(on_check_roundtrip),
                    ));
                    // 往返检查结束、视图模式确定后才就绪：此前骨架屏遮挡，保存不可用。
                    spawn(async move {
                        if let Some(settled) = settled {
                            let _ = wasm_bindgen_futures::JsFuture::from(settled).await;
                        }
                        ready.set(true);
                    });
                }
                Ok(None) => phase.set(EditorPhase::Failed("编辑器容器未就绪".into())),
                Err(error) => {
                    web_sys::console::error_1(&error);
                    phase.set(EditorPhase::Failed("编辑器加载失败，请刷新页面重试".into()))
                }
            }
        });
        let mut baseline_done = use_signal(|| false);
        use_effect(move || {
            if !ready() {
                return;
            }
            // The editor normalizes Markdown on load; on the first ready adopt its
            // form as the saved baseline so an untouched post does not read as
            // modified. Nothing can be typed into the body before that.
            if let Some(handle) = &*editor.peek() {
                let markdown = handle.instance().get_markdown();
                if !*baseline_done.peek() {
                    baseline_done.set(true);
                    saved.write().content = markdown.clone();
                }
                form.write().content = markdown;
            }
            phase.set(EditorPhase::Ready);
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut phase = state.phase;
        use_effect(move || phase.set(EditorPhase::Ready));
    }
    state
}
