//! 封面上传、素材选择与预览，上传临时状态仅在本组件中持有。
use super::state::PostDraft;
use crate::components::assets::{AssetPickerModal, AssetSelection};
use crate::components::enhance_assets::LightboxAssets;
use crate::components::forms::FormInput;
use crate::components::ui::{button_class, ButtonSize, ButtonVariant, Icon, IconName};
use leptos::ev;
use leptos::prelude::*;

/// 从 `<input type="file">` 的 change 事件里取第一个文件。
#[cfg(target_arch = "wasm32")]
fn first_input_file(event: &ev::Event) -> Option<web_sys::File> {
    event_target::<web_sys::HtmlInputElement>(event)
        .files()
        .and_then(|files| files.item(0))
}

/// 封面上传子组件。
///
/// 封装封面图的全部状态与交互：拖拽 / 粘贴 / 选择文件上传、输入链接、素材库、预览、移除。
/// 最终 URL 写入共享表单，上传状态单独用于保存拦截：
/// - `form.cover_image`：子组件写入最终 URL，父组件读取用于保存；
/// - `cover_uploading`：子组件上传时置 true，父组件在保存时读取以拦截
///   上传中的保存（避免封面 URL 尚未落定就提交导致封面丢失）。
///
/// 其余上传中间态（error/url/drag）对本组件私有。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub(super) fn CoverUploader(
    form: RwSignal<PostDraft>,
    cover_uploading: RwSignal<bool>,
) -> impl IntoView {
    let cover_image = Memo::new(move |_| form.with(|f| f.cover_image.clone()));
    let has_cover = Memo::new(move |_| !cover_image.with(String::is_empty));
    // 封面图上传状态：错误消息、URL 输入框展开、拖拽高亮（cover_uploading 由父组件传入）。
    let cover_error = RwSignal::new(None::<String>);
    let cover_url_mode = RwSignal::new(false);
    let cover_drag_active = RwSignal::new(false);
    // 封面 URL 输入框的临时值（确认前不直接写入 cover_image，避免半截 URL 触发预览加载）。
    let cover_url_input = RwSignal::new(String::new());
    // 素材选择 modal 显隐。
    let picker_visible = RwSignal::new(false);
    // 封面预览图加载状态：cover_image 变化时置 false，img onload 置 true。
    // 从素材库选择/拖拽替换/URL 输入后，预览图 ?w=600 版本未缓存需重新加载，
    // 加载期间显示骨架占位，避免空白无反馈（与上传中的骨架语义独立）。
    let cover_img_loaded = RwSignal::new(true);

    // 封面图上传：三条入口（file input / drop / paste）拿到 File 后统一调用。
    #[cfg(target_arch = "wasm32")]
    let spawn_cover_upload = move |file: web_sys::File| {
        cover_uploading.set(true);
        cover_error.set(None);
        leptos::task::spawn_local(async move {
            match crate::bridges::tiptap::upload_image_file(file).await {
                Ok(url) => {
                    let _ = form.try_update(|f| f.cover_image = url);
                }
                Err(msg) => {
                    let _ = cover_error.try_set(Some(format!("封面上传失败：{msg}")));
                }
            }
            let _ = cover_uploading.try_set(false);
        });
    };

    // 灯箱绑定 + 预览图加载占位：封面有图时点击放大查看。
    // 订阅 cover_image：图片出现/更换后重跑，data-lb-bound 守卫保证幂等。
    // 新 src 待加载时置 cover_img_loaded=false，由 img onload 回填 true。
    Effect::new(move |_| {
        let cv = cover_image.get();
        if cv.is_empty() {
            cover_img_loaded.set(true);
            return;
        }
        cover_img_loaded.set(false);
        #[cfg(target_arch = "wasm32")]
        {
            let window = web_sys::window()
                .expect("CoverUploader Effect 仅在 WASM 浏览器上下文执行：无 window");
            let selectors = js_sys::Array::of1(&".cover-preview".into());
            let selectors_val = js_sys::Object::from(selectors).into();
            let _ = js_sys::Reflect::set(&window, &"__lightboxSelectors".into(), &selectors_val);
            crate::utils::js::invoke_optional_global(&window, "__initLightbox", &[selectors_val]);
        }
    });

    // 预览图：/uploads/ 路径加 ?w=600 缩略，外链 URL 原样用。
    let preview_src = move || {
        let cv = cover_image.get();
        if cv.starts_with("/uploads/") {
            let base = cv.split('?').next().unwrap_or(&cv).to_string();
            if cv.contains('?') {
                format!("{base}&w=600")
            } else {
                format!("{base}?w=600")
            }
        } else {
            cv
        }
    };
    let confirm_url = move || {
        let v = cover_url_input.get_untracked().trim().to_string();
        if !v.is_empty() {
            form.update(|f| f.cover_image = v);
            cover_error.set(None);
            cover_url_mode.set(false);
        }
    };

    let on_drop = move |evt: ev::DragEvent| {
        evt.prevent_default();
        cover_drag_active.set(false);
        if !cover_uploading.get_untracked() {
            #[cfg(target_arch = "wasm32")]
            if let Some(file) = evt
                .data_transfer()
                .and_then(|dt| dt.files())
                .and_then(|list| list.item(0))
            {
                spawn_cover_upload(file);
            }
        }
    };
    let on_paste = move |evt: ev::ClipboardEvent| {
        evt.prevent_default();
        if !cover_uploading.get_untracked() {
            #[cfg(target_arch = "wasm32")]
            {
                let file = evt
                    .clipboard_data()
                    .and_then(|dt| dt.files())
                    .and_then(|list| list.item(0));
                if let Some(file) = file {
                    spawn_cover_upload(file);
                }
            }
        }
    };
    let on_file_change = move |evt: ev::Event| {
        #[cfg(target_arch = "wasm32")]
        if let Some(file) = first_input_file(&evt) {
            spawn_cover_upload(file);
        }
    };

    let on_pick_cover = move |picks: Vec<AssetSelection>| {
        if let Some(first) = picks.into_iter().next() {
            form.update(|f| f.cover_image = first.url);
            cover_error.set(None);
        }
    };

    view! {
        // P5 §8.2：封面预览灯箱按需注入（脚本未就绪时迟到调用入队兜底）。
        <LightboxAssets/>
        // 封面区：21:9；空态斜纹底加「拖拽、粘贴或点击上传」，有图时显示预览（点击放大）。
        // 整个区域接收拖拽与粘贴；下方一行按钮放素材库、输入链接或更换、移除。
        <div
            class="write-cover cover-preview"
            data-state=move || {
                if cover_uploading.get() {
                    "uploading"
                } else if has_cover.get() {
                    "filled"
                } else {
                    "empty"
                }
            }
            data-drag=move || cover_drag_active.get().then_some("true")
            // dragover 必须 prevent_default，否则浏览器直接打开文件。
            on:dragover=move |evt: ev::DragEvent| {
                evt.prevent_default();
                if !cover_uploading.get_untracked() {
                    cover_drag_active.set(true);
                }
            }
            on:dragenter=move |evt: ev::DragEvent| evt.prevent_default()
            on:dragleave=move |_| cover_drag_active.set(false)
            on:drop=on_drop
            on:paste=on_paste
        >
            {move || {
                if cover_uploading.get() {
                    view! { <span class="write-cover-note" role="status">"正在上传…"</span> }
                        .into_any()
                } else if has_cover.get() {
                    // lightbox-single：由全局 lightbox.js 接管点击放大，data-src 为原图。
                    view! {
                        <img
                            class="lightbox-single"
                            data-src=move || cover_image.get()
                            src=preview_src
                            alt="封面预览"
                            on:load=move |_| cover_img_loaded.set(true)
                            on:error=move |_| {
                                cover_img_loaded.set(true);
                                cover_error.set(Some("封面图加载失败，请检查链接".to_string()));
                            }
                        />
                        {move || {
                            (!cover_img_loaded.get())
                                .then(|| view! { <span class="write-cover-loading" aria-hidden="true"></span> })
                        }}
                        {move || {
                            cover_drag_active
                                .get()
                                .then(|| {
                                    view! {
                                        <span class="write-cover-note write-cover-drop">
                                            "松开以更换封面"
                                        </span>
                                    }
                                })
                        }}
                    }
                        .into_any()
                } else {
                    // label 包裹空态：点击直接触发隐藏的 file input。
                    view! {
                        <label class="write-cover-empty">
                            <Icon name=IconName::Upload size=18/>
                            {move || {
                                if cover_drag_active.get() { "松开以上传" } else { "拖拽、粘贴或点击上传" }
                            }}
                            <input
                                type="file"
                                accept="image/jpeg,image/png,image/gif,image/webp"
                                class="sr-only"
                                on:change=on_file_change
                            />
                        </label>
                    }
                        .into_any()
                }
            }}
        </div>

        {move || {
            if cover_url_mode.get() && !has_cover.get() {
                view! {
                    <div class="write-cover-url">
                        <FormInput
                            class="field field-sm"
                            r#type="url"
                            aria_label="封面图片链接"
                            placeholder="粘贴图片链接"
                            value=cover_url_input
                            oninput=move |v: String| cover_url_input.set(v)
                            onkeydown=move |e: ev::KeyboardEvent| {
                                if e.key() == "Enter" && !e.is_composing() {
                                    confirm_url();
                                } else if e.key() == "Escape" {
                                    cover_url_mode.set(false);
                                }
                            }
                        />
                        <button
                            class=button_class(ButtonVariant::Primary, ButtonSize::Sm)
                            type="button"
                            on:click=move |_| confirm_url()
                        >
                            "确认"
                        </button>
                        <button
                            class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                            type="button"
                            on:click=move |_| {
                                cover_url_mode.set(false);
                                cover_url_input.set(String::new());
                            }
                        >
                            "取消"
                        </button>
                    </div>
                }
                    .into_any()
            } else {
                view! {
                    <div class="write-cover-actions">
                        {move || {
                            has_cover
                                .get()
                                .then(|| {
                                    view! {
                                        <label class=button_class(
                                            ButtonVariant::Secondary,
                                            ButtonSize::Sm,
                                        )>
                                            "更换"
                                            <input
                                                type="file"
                                                accept="image/jpeg,image/png,image/gif,image/webp"
                                                class="sr-only"
                                                disabled=move || cover_uploading.get()
                                                on:change=on_file_change
                                            />
                                        </label>
                                    }
                                })
                        }}
                        <button
                            class=button_class(ButtonVariant::Secondary, ButtonSize::Sm)
                            type="button"
                            disabled=move || cover_uploading.get()
                            on:click=move |_| picker_visible.set(true)
                        >
                            "素材库"
                        </button>
                        {move || {
                            if has_cover.get() {
                                view! {
                                    <button
                                        class=button_class(ButtonVariant::Secondary, ButtonSize::Sm)
                                        type="button"
                                        on:click=move |_| {
                                            form.update(|f| f.cover_image = String::new());
                                            cover_error.set(None);
                                        }
                                    >
                                        "移除"
                                    </button>
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <button
                                        class=button_class(ButtonVariant::Secondary, ButtonSize::Sm)
                                        type="button"
                                        disabled=move || cover_uploading.get()
                                        on:click=move |_| {
                                            cover_url_input.set(String::new());
                                            cover_url_mode.set(true);
                                        }
                                    >
                                        "输入链接"
                                    </button>
                                }
                                    .into_any()
                            }
                        }}
                    </div>
                }
                    .into_any()
            }
        }}

        {move || {
            cover_error
                .get()
                .map(|err| {
                    view! {
                        <p class="field-hint" data-tone="danger" role="alert">
                            {err}
                        </p>
                    }
                })
        }}

        // 素材选择：选中回填封面 URL（在弹窗里上传新图成功同样回填）。
        <AssetPickerModal
            visible=picker_visible
            cover_uploading=cover_uploading
            on_select=on_pick_cover
        />
    }
}
