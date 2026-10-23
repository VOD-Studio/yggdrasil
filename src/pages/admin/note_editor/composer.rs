use dioxus::prelude::*;

/// 把焦点放进正文编辑区。
pub(super) fn focus_editor() {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| {
                d.query_selector("#note-tiptap [contenteditable]")
                    .ok()
                    .flatten()
            })
            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = el.focus();
        }
    }
}

/// 富文本编辑与笔记专用的受保护图片上传。素材库中的图片本来就是公开素材，
/// 选择面板的标题写明这一点；私密图片在编辑器里上传。
#[component]
pub(super) fn NoteComposer(
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
                    let _ = inst.set_markdown(&initial);
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
            crate::bridges::library::LibraryLoadError { library }
            if !error().is_empty() {
                p { class: "write-notice", role: "alert", "{error}" }
            }
            for item in errors() {
                p { class: "write-notice", role: "alert", key: "{item.id}",
                    span { "{item.message}" }
                    button {
                        class: "btn btn-sm btn-ghost",
                        r#type: "button",
                        onclick: move |_| {
                            if let Some(handle) = editor.peek().as_ref() {
                                handle.instance().remove_upload_by_upload_id(&item.id);
                            }
                        },
                        "移除失败图片"
                    }
                }
            }
            if !ready() {
                p { class: "note-editor-loading", role: "status", "正在加载编辑器…" }
            }
            div { id: "note-tiptap", class: "write-editor" }
            if pick() {
                crate::components::assets::AssetPickerModal {
                    visible: pick,
                    cover_uploading: picking_upload,
                    title: "选择公开素材",
                    multi: true,
                    on_select: move |items: Vec<crate::components::assets::AssetSelection>| {
                        if let Some(handle) = editor.peek().as_ref() {
                            if let Ok(json) = serde_json::to_string(&items) {
                                handle.instance().insert_images_from_library(&json);
                            }
                        }
                        pick.set(false);
                    },
                }
            }
        };
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (initial, on_change, uploading);
        rsx! {
            p { class: "note-editor-loading", role: "status", "正在加载编辑器…" }
            div { id: "note-tiptap", class: "write-editor" }
        }
    }
}
