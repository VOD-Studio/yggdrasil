//! 封面上传、素材选择与预览，上传临时状态仅在本组件中持有。
use super::state::PostDraft;
#[cfg(target_arch = "wasm32")]
use crate::bridges::tiptap::upload_image_file;
use crate::components::assets::{AssetPickerModal, AssetSelection};
use crate::components::ui::{button_class, ButtonSize, ButtonVariant};
#[cfg(target_arch = "wasm32")]
use crate::utils::js::invoke_optional_global;
#[cfg(target_arch = "wasm32")]
use dioxus::html::HasFileData;
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use dioxus::web::{WebEventExt, WebFileExt};

/// 封面上传子组件。
///
/// 封装封面图的全部状态与交互：拖拽 / 粘贴 / 选择文件上传、输入链接、素材库、预览、移除。
/// 最终 URL 写入共享表单，上传状态单独用于保存拦截：
/// - `form.cover_image`：子组件写入最终 URL，父组件读取用于保存；
/// - `cover_uploading`：子组件上传时置 true，父组件在 `on_submit` 读取以拦截
///   上传中的保存（避免封面 URL 尚未落定就提交导致封面丢失）。
/// 其余上传中间态（error/url/drag）对本组件私有。
///
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn CoverUploader(
    mut form: Signal<PostDraft>,
    mut cover_uploading: Signal<bool>,
) -> Element {
    let cover_image = use_memo(move || form.read().cover_image.clone());
    // 封面图上传状态：错误消息、URL 输入框展开、拖拽高亮（cover_uploading 由父组件传入）。
    let mut cover_error = use_signal(|| None::<String>);
    let mut cover_url_mode = use_signal(|| false);
    let mut cover_drag_active = use_signal(|| false);
    // 封面 URL 输入框的临时值（确认前不直接写入 cover_image，避免半截 URL 触发预览加载）。
    let mut cover_url_input = use_signal(|| "".to_string());
    // 素材选择 modal 显隐。
    let mut picker_visible = use_signal(|| false);
    // 封面预览图加载状态：cover_image 变化时置 false，img onload 置 true。
    // 从素材库选择/拖拽替换/URL 输入后，预览图 ?w=600 版本未缓存需重新加载，
    // 加载期间显示骨架占位，避免空白无反馈（与上传中的骨架语义独立）。
    let mut cover_img_loaded = use_signal(|| true);

    // 封面图上传：spawn 一个 async 调用 upload_image_file。
    // 三条入口（file input / drop / paste）收敛成拿到 web_sys::File 后统一调用此闭包。
    // 仅在 WASM 端有意义（upload_image_file 与 spawn 都依赖 WASM 运行时），
    // server SSR 不渲染上传逻辑，故整体 cfg-gate 避免引用 wasm-only 符号。
    #[cfg(target_arch = "wasm32")]
    let mut spawn_cover_upload = move |file: web_sys::File| {
        cover_uploading.set(true);
        cover_error.set(None);
        spawn(async move {
            match upload_image_file(file).await {
                Ok(url) => {
                    form.write().cover_image = url;
                }
                Err(msg) => {
                    cover_error.set(Some(format!("封面上传失败：{msg}")));
                }
            }
            cover_uploading.set(false);
        });
    };
    // 灯箱绑定 + 预览图加载占位：封面有图时点击放大查看（lightbox 按需注入，
    // 见下方 rsx 的 LightboxAssets 声明）。
    // 订阅 cover_image：图片出现/更换后 effect 重跑，data-lb-bound 守卫保证幂等。
    // 新 src 待加载时置 cover_img_loaded=false，由 img onload 回填 true。
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let cv = cover_image();
        if cv.is_empty() {
            cover_img_loaded.set(true);
            return;
        }
        cover_img_loaded.set(false);
        let window = web_sys::window()
            .expect("CoverUploader use_effect 仅在 WASM 浏览器上下文执行：无 window");
        let selectors = js_sys::Array::of1(&".cover-preview".into());
        let selectors_val = js_sys::Object::from(selectors).into();
        let _ = js_sys::Reflect::set(&window, &"__lightboxSelectors".into(), &selectors_val);
        invoke_optional_global(&window, "__initLightbox", &[selectors_val]);
    });

    // 预览图：/uploads/ 路径加 ?w=600 缩略，外链 URL 原样用。
    let preview_src = {
        let cv = cover_image();
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
    let has_cover = !cover_image().is_empty();
    let mut confirm_url = move || {
        let v = cover_url_input().trim().to_string();
        if !v.is_empty() {
            form.write().cover_image = v;
            cover_error.set(None);
            cover_url_mode.set(false);
        }
    };

    rsx! {
        // P5 §8.2：封面预览灯箱按需注入（脚本未就绪时迟到调用入队兜底）。
        crate::components::enhance_assets::LightboxAssets {}
        // 封面区：21:9；空态斜纹底加「拖拽、粘贴或点击上传」，有图时显示预览（点击放大）。
        // 整个区域接收拖拽与粘贴；下方一行按钮放素材库、输入链接或更换、移除。
        div {
            class: "write-cover cover-preview",
            "data-state": if cover_uploading() { "uploading" } else if has_cover { "filled" } else { "empty" },
            "data-drag": cover_drag_active().then_some("true"),
            // ondragover 必须 prevent_default，否则浏览器直接打开文件。
            ondragover: move |evt| {
                evt.prevent_default();
                if !cover_uploading() {
                    cover_drag_active.set(true);
                }
            },
            ondragenter: move |evt| evt.prevent_default(),
            ondragleave: move |_| cover_drag_active.set(false),
            ondrop: move |evt| {
                evt.prevent_default();
                cover_drag_active.set(false);
                if !cover_uploading() {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(web_file) = evt.files().into_iter().next().and_then(|f| f.get_web_file()) {
                        spawn_cover_upload(web_file);
                    }
                }
            },
            onpaste: move |evt| {
                evt.prevent_default();
                if !cover_uploading() {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use wasm_bindgen::JsCast;
                        let file = evt
                            .try_as_web_event()
                            .and_then(|raw| raw.dyn_into::<web_sys::ClipboardEvent>().ok())
                            .and_then(|ce| ce.clipboard_data())
                            .and_then(|dt| dt.files())
                            .and_then(|list| list.item(0));
                        if let Some(file) = file {
                            spawn_cover_upload(file);
                        }
                    }
                }
            },

            if cover_uploading() {
                span { class: "write-cover-note", role: "status", "正在上传…" }
            } else if has_cover {
                // lightbox-single：由全局 lightbox.js 接管点击放大，data-src 为原图。
                img {
                    class: "lightbox-single",
                    "data-src": "{cover_image()}",
                    src: "{preview_src}",
                    alt: "封面预览",
                    onload: move |_| cover_img_loaded.set(true),
                    onerror: move |_| {
                        cover_img_loaded.set(true);
                        cover_error.set(Some("封面图加载失败，请检查链接".to_string()));
                    },
                }
                if !cover_img_loaded() {
                    span { class: "write-cover-loading", aria_hidden: "true" }
                }
                if cover_drag_active() {
                    span { class: "write-cover-note write-cover-drop", "松开以更换封面" }
                }
            } else {
                // label 包裹空态：点击直接触发隐藏的 file input。
                label { class: "write-cover-empty",
                    crate::components::ui::Icon { name: crate::components::ui::IconName::Upload, size: 18 }
                    if cover_drag_active() { "松开以上传" } else { "拖拽、粘贴或点击上传" }
                    input {
                        r#type: "file",
                        accept: "image/jpeg,image/png,image/gif,image/webp",
                        class: "sr-only",
                        onchange: move |evt| {
                            #[cfg(target_arch = "wasm32")]
                            if let Some(web_file) = evt.files().into_iter().next().and_then(|f| f.get_web_file()) {
                                spawn_cover_upload(web_file);
                            }
                        },
                    }
                }
            }
        }

        if cover_url_mode() && !has_cover {
            div { class: "write-cover-url",
                input {
                    class: "field field-sm",
                    r#type: "url",
                    aria_label: "封面图片链接",
                    placeholder: "粘贴图片链接",
                    value: "{cover_url_input}",
                    oninput: move |e| cover_url_input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !e.data().is_composing() {
                            confirm_url();
                        } else if e.key() == Key::Escape {
                            cover_url_mode.set(false);
                        }
                    },
                }
                button {
                    class: button_class(ButtonVariant::Primary, ButtonSize::Sm),
                    r#type: "button",
                    onclick: move |_| confirm_url(),
                    "确认"
                }
                button {
                    class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                    r#type: "button",
                    onclick: move |_| {
                        cover_url_mode.set(false);
                        cover_url_input.set(String::new());
                    },
                    "取消"
                }
            }
        } else {
            div { class: "write-cover-actions",
                if has_cover {
                    label { class: "{button_class(ButtonVariant::Secondary, ButtonSize::Sm)}",
                        "更换"
                        input {
                            r#type: "file",
                            accept: "image/jpeg,image/png,image/gif,image/webp",
                            class: "sr-only",
                            disabled: cover_uploading(),
                            onchange: move |evt| {
                                #[cfg(target_arch = "wasm32")]
                                if let Some(web_file) = evt.files().into_iter().next().and_then(|f| f.get_web_file()) {
                                    spawn_cover_upload(web_file);
                                }
                            },
                        }
                    }
                }
                button {
                    class: button_class(ButtonVariant::Secondary, ButtonSize::Sm),
                    r#type: "button",
                    disabled: cover_uploading(),
                    onclick: move |_| picker_visible.set(true),
                    "素材库"
                }
                if has_cover {
                    button {
                        class: button_class(ButtonVariant::Secondary, ButtonSize::Sm),
                        r#type: "button",
                        onclick: move |_| {
                            form.write().cover_image = String::new();
                            cover_error.set(None);
                        },
                        "移除"
                    }
                } else {
                    button {
                        class: button_class(ButtonVariant::Secondary, ButtonSize::Sm),
                        r#type: "button",
                        disabled: cover_uploading(),
                        onclick: move |_| {
                            cover_url_input.set(String::new());
                            cover_url_mode.set(true);
                        },
                        "输入链接"
                    }
                }
            }
        }

        if let Some(err) = cover_error() {
            p { class: "field-hint", "data-tone": "danger", role: "alert", "{err}" }
        }

        // 素材选择：选中回填封面 URL（在弹窗里上传新图成功同样回填）。
        AssetPickerModal {
            visible: picker_visible,
            cover_uploading,
            on_select: move |picks: Vec<AssetSelection>| {
                if let Some(first) = picks.into_iter().next() {
                    form.write().cover_image = first.url;
                    cover_error.set(None);
                }
            },
        }
    }
}
