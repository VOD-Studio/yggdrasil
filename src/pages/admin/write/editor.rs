//! 文章编辑器的初始化与清理。文章在本模块运行前已取回，表单以服务端内容起步，
//! 本地恢复副本才能与之比较。
use super::state::{EditorPhase, PostDraft, SaveState, WriteEditorState};
use crate::bridges::library::use_browser_library;
use crate::bridges::tiptap::UploadsInFlight;
use chrono::{DateTime, Utc};
use leptos::prelude::*;

/// 正文为空时编辑器里的提示。
#[cfg(target_arch = "wasm32")]
const PLACEHOLDER: &str = "开始写作，或输入 / 插入代码、图表、公式和图片";

/// 编辑器的 Tiptap 回调与句柄只活在写作页的所有者里：页面卸载时 `editor` 随之释放，
/// 触发 JS 端 destroy。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub(super) fn use_write_editor(
    post_id: Option<i32>,
    initial: PostDraft,
    published_at: Option<DateTime<Utc>>,
) -> WriteEditorState {
    let baseline = initial.clone();
    let state = WriteEditorState {
        post_id,
        form: RwSignal::new(initial),
        saved: RwSignal::new(baseline),
        saved_at: RwSignal::new(None),
        published_at: RwSignal::new(published_at),
        phase: RwSignal::new(EditorPhase::Initializing),
        save: RwSignal::new(SaveState::default()),
        library: use_browser_library("tiptap", || true),
        uploads_in_flight: RwSignal::new(UploadsInFlight::default()),
        upload_errors: RwSignal::new(Vec::new()),
        cover_uploading: RwSignal::new(false),
        body_picker_visible: RwSignal::new(false),
        body_picker_uploading: RwSignal::new(false),
        reload: RwSignal::new(0),
        #[cfg(target_arch = "wasm32")]
        editor: StoredValue::new_local(None),
    };
    #[cfg(target_arch = "wasm32")]
    install_editor(state);
    state
}

/// 库就绪后创建编辑器，并在往返检查结束后把页面置为就绪。
#[cfg(target_arch = "wasm32")]
fn install_editor(state: WriteEditorState) {
    use crate::bridges::library::library_ready;
    use crate::bridges::tiptap::{consume_upload_event, EditorHandle, EditorOptions};
    use wasm_bindgen::closure::Closure;

    let ready = RwSignal::new(false);
    // 每次（重新）创建递增世代号：迟到的帧回调与往返检查结果只认最新一次。
    let generation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        state.reload.track();
        if !library_ready(state.library) {
            return;
        }
        let Some(current) = generation.try_update_value(|g| {
            *g += 1;
            *g
        }) else {
            return;
        };
        ready.set(false);
        state.editor.set_value(None);
        state.phase.set(EditorPhase::Initializing);
        // 重试时保留失败前已输入的内容。
        let markdown = state.form.with_untracked(|form| form.content.clone());
        // 推迟一帧：保证容器 div 已挂入文档，按 id 能找到。
        request_animation_frame(move || {
            if generation.try_get_value() != Some(current) {
                return;
            }
            let on_update: Closure<dyn FnMut(String)> = Closure::new(move |md: String| {
                let _ = state.form.try_update(|form| form.content = md);
            });
            let on_check_roundtrip = crate::bridges::tiptap::make_roundtrip_check_closure();
            let on_image_upload = crate::bridges::tiptap::make_upload_closure();
            let on_upload_event: Closure<dyn FnMut(crate::bridges::tiptap::UploadEventJs)> =
                Closure::new(move |event: crate::bridges::tiptap::UploadEventJs| {
                    consume_upload_event(&event, state.uploads_in_flight, state.upload_errors);
                });
            let on_run_code = crate::bridges::tiptap::make_run_code_closure();
            let on_pick_from_library: Closure<dyn FnMut()> = Closure::new(move || {
                let _ = state.body_picker_visible.try_set(true);
            });
            let options = EditorOptions::new();
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
                    state.editor.set_value(Some(
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
                    leptos::task::spawn_local(async move {
                        if let Some(settled) = settled {
                            let _ = wasm_bindgen_futures::JsFuture::from(settled).await;
                        }
                        if generation.try_get_value() == Some(current) {
                            let _ = ready.try_set(true);
                        }
                    });
                }
                Ok(None) => state
                    .phase
                    .set(EditorPhase::Failed("编辑器容器未就绪".into())),
                Err(error) => {
                    web_sys::console::error_1(&error);
                    state
                        .phase
                        .set(EditorPhase::Failed("编辑器加载失败，请刷新页面重试".into()));
                }
            }
        });
    });

    let baseline_done = StoredValue::new(false);
    Effect::new(move |_| {
        if !ready.get() {
            return;
        }
        // 编辑器加载时会规范化 Markdown；首次就绪时把它的结果采纳为已保存基线，
        // 免得没动过的文章显示成“已修改”。此前正文无法输入。
        state.editor.with_value(|handle| {
            if let Some(handle) = handle {
                let markdown = handle.instance().get_markdown();
                if !baseline_done.get_value() {
                    baseline_done.set_value(true);
                    state.saved.update(|saved| saved.content = markdown.clone());
                }
                state.form.update(|form| form.content = markdown);
            }
        });
        state.phase.set(EditorPhase::Ready);
    });
}
