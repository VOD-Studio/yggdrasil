use crate::bridges::library::{library_ready, use_browser_library, LibraryLoadError};
use crate::bridges::tiptap::{UploadErrorEntry, UploadsInFlight};
use crate::components::assets::{AssetPickerModal, AssetSelection};
use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
type Handle = crate::bridges::tiptap::EditorHandle;
#[cfg(not(target_arch = "wasm32"))]
type Handle = ();

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
///
/// 视图在服务端与浏览器一致（编辑器实例只在 Effect 里创建）；`initial` 只在创建时读取，
/// 之后内容由编辑器自己持有，父级通过更换组件（`editor_epoch`）重建。
#[component]
pub(super) fn NoteComposer(
    initial: String,
    on_change: Callback<String>,
    uploading: RwSignal<bool>,
) -> impl IntoView {
    let library = use_browser_library("tiptap", || true);
    let editor = StoredValue::new_local(None::<Handle>);
    let ready = RwSignal::new(false);
    let error = RwSignal::new(String::new());
    let counts = RwSignal::new(UploadsInFlight::default());
    let errors = RwSignal::new(Vec::<UploadErrorEntry>::new());
    let pick = RwSignal::new(false);
    let picking_upload = RwSignal::new(false);
    let container = NodeRef::<leptos::html::Div>::new();
    // 卸载时丢弃句柄：Drop 先 destroy 实例，再释放回调。
    on_cleanup(move || {
        let _ = editor.try_update_value(|handle| *handle = None);
    });
    Effect::new(move |_| {
        let counts = counts.get();
        uploading.set(counts.uploading > 0 || counts.error > 0 || picking_upload.get());
    });
    Effect::new(move |_| {
        // 库就绪且容器挂入文档后才创建；等一帧确保容器已在文档里。
        if container.get().is_none() || !library_ready(library) {
            return;
        }
        let initial = initial.clone();
        request_animation_frame(move || {
            #[cfg(target_arch = "wasm32")]
            create_editor(
                editor, initial, on_change, ready, error, counts, errors, pick,
            );
            #[cfg(not(target_arch = "wasm32"))]
            let _ = (initial, on_change);
        });
    });

    view! {
        <LibraryLoadError library/>
        {move || {
            let message = error.get();
            (!message.is_empty())
                .then(|| {
                    view! {
                        <p class="write-notice" role="alert">
                            {message}
                        </p>
                    }
                })
        }}
        {move || {
            errors
                .get()
                .into_iter()
                .map(|item| {
                    let id = item.id.clone();
                    view! {
                        <p class="write-notice" role="alert">
                            <span>{item.message}</span>
                            <button
                                class="btn btn-sm btn-ghost"
                                type="button"
                                on:click=move |_| {
                                    #[cfg(target_arch = "wasm32")]
                                    editor
                                        .with_value(|handle| {
                                            if let Some(handle) = handle.as_ref() {
                                                handle.instance().remove_upload_by_upload_id(&id);
                                            }
                                        });
                                    #[cfg(not(target_arch = "wasm32"))]
                                    let _ = &id;
                                }
                            >
                                "移除失败图片"
                            </button>
                        </p>
                    }
                })
                .collect_view()
        }}
        {move || {
            (!ready.get())
                .then(|| {
                    view! {
                        <p class="note-editor-loading" role="status">
                            "正在加载编辑器…"
                        </p>
                    }
                })
        }}
        <div id="note-tiptap" class="write-editor" node_ref=container></div>
        {move || {
            pick.get()
                .then(|| {
                    view! {
                        <AssetPickerModal
                            visible=pick
                            cover_uploading=picking_upload
                            title="选择公开素材"
                            multi=true
                            on_select=Callback::new(move |items: Vec<AssetSelection>| {
                                #[cfg(target_arch = "wasm32")]
                                editor
                                    .with_value(|handle| {
                                        if let Some(handle) = handle.as_ref() {
                                            if let Ok(json) = serde_json::to_string(&items) {
                                                handle.instance().insert_images_from_library(&json);
                                            }
                                        }
                                    });
                                #[cfg(not(target_arch = "wasm32"))]
                                let _ = &items;
                                pick.set(false);
                            })
                        />
                    }
                })
        }}
    }
}

/// 在容器里创建 Tiptap 实例并把回调随句柄一起交给 `editor`。已存在实例或组件已卸载时什么都不做。
#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
fn create_editor(
    editor: StoredValue<Option<Handle>, leptos::reactive::owner::LocalStorage>,
    initial: String,
    on_change: Callback<String>,
    ready: RwSignal<bool>,
    error: RwSignal<String>,
    counts: RwSignal<UploadsInFlight>,
    errors: RwSignal<Vec<UploadErrorEntry>>,
    pick: RwSignal<bool>,
) {
    use crate::bridges::tiptap::{self, EditorOptions};
    use wasm_bindgen::prelude::*;

    // `None`：组件已卸载；`Some(true)`：已有实例。
    if editor.try_with_value(Option::is_some) != Some(false) {
        return;
    }
    let update = Closure::new(move |md: String| {
        on_change.try_run(md);
    });
    let on_ready = Closure::new(move || {
        let _ = ready.try_set(true);
    });
    let upload = Closure::new(move |file: web_sys::File| {
        wasm_bindgen_futures::future_to_promise(async move {
            tiptap::wasm::upload_note_image_file(file)
                .await
                .map(|s| JsValue::from_str(&s))
                .map_err(|e| JsValue::from_str(&e))
        })
    });
    let upload_event = Closure::new(move |ev: tiptap::UploadEventJs| {
        if counts.try_get_untracked().is_some() && errors.try_get_untracked().is_some() {
            tiptap::consume_upload_event(&ev, counts, errors)
        }
    });
    let run = tiptap::make_run_code_closure();
    let picker = Closure::new(move || {
        let _ = pick.try_set(true);
    });
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
            let _ = editor.try_set_value(Some(Handle::new(
                inst,
                update,
                upload,
                on_ready,
                upload_event,
                run,
                picker,
            )));
        }
        _ => {
            let _ = error.try_set("编辑器初始化失败，请刷新后重试".into());
        }
    }
}
