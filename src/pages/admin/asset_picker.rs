//! 素材选择 modal（封面或头像上的「从素材库选择」）。
//!
//! 网格展示素材库（默认最新排序，支持文件名/alt 搜索），单击选中回填图片 URL。
//! 内嵌「上传新图」入口（复用 `upload_image_file`），上传成功后留在网格中供用户选择。
//! 纯 Dioxus 组件，不触碰 Tiptap；数据加载仅在 WASM 前端发生。

use crate::components::forms::{FormInput, INPUT_INLINE_CLASS};
use crate::components::ui::{BTN_ICON, BTN_PRIMARY, SPINNER_SVG};
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::assets::list_assets;
use crate::models::asset::AssetDto;
#[cfg(target_arch = "wasm32")]
use crate::models::asset::{AssetFilter, AssetSort};

/// 关闭过渡时长，与 input.css 的 modal-overlay / modal-panel 过渡保持一致。
const EXIT_ANIM_MS: u32 = 200;

/// 素材选择 modal。
///
/// - `visible`：显隐控制（父组件持有，选中/点遮罩/× 都会置 false）。
/// - `on_select`：选中回填，参数为 `/uploads/<path>` URL。
/// - `cover_uploading`：modal 内上传新图时置位，供父页面拦截保存（与 CoverUploader 语义一致）。
/// - `title`：modal 标题；封面场景缺省为「选择封面图」，头像场景传入「选择头像」。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn AssetPickerModal(
    mut visible: Signal<bool>,
    on_select: EventHandler<String>,
    cover_uploading: Signal<bool>,
    #[props(default = "选择封面图")] title: &'static str,
) -> Element {
    #[allow(unused_mut)]
    let mut assets: Signal<Vec<AssetDto>> = use_signal(Vec::new);
    #[allow(unused_mut)]
    let mut loading = use_signal(|| false);
    let mut query = use_signal(String::new);
    #[allow(unused_mut)]
    let mut error = use_signal(|| None::<String>);
    // 上传中的本地对象 URL 只用于当前 modal 预览，上传结束立即释放。
    let mut uploading_preview = use_signal(|| None::<String>);
    // 关闭时保留 DOM 播放淡出；动画结束后再卸载，避免弹窗瞬间消失。
    let mut closing = use_signal(|| false);
    let mut opened = use_signal(|| false);
    // 上传完成后先把新图留在弹窗网格中，用户点击后才真正应用头像。
    let mut uploaded_url = use_signal(|| None::<String>);

    use_effect(move || {
        if visible() {
            opened.set(true);
            closing.set(false);
            uploaded_url.set(None);
        } else if *opened.peek() {
            if !*closing.peek() {
                closing.set(true);
            }
            spawn(async move {
                crate::utils::time::sleep_ms(EXIT_ANIM_MS).await;
                closing.set(false);
            });
        }
    });

    // 打开时与搜索词变化时加载第一页（最新 60 张，封面/头像复用场景足够）。
    use_effect(move || {
        let open = visible();
        let q = query();
        if open {
            #[cfg(target_arch = "wasm32")]
            spawn(async move {
                loading.set(true);
                match list_assets(AssetFilter::All, q, AssetSort::CreatedDesc, 1).await {
                    Ok(resp) => {
                        assets.set(resp.assets);
                        error.set(None);
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                loading.set(false);
            });
        }
    });

    let is_closing = closing();
    if !visible() && !is_closing {
        return rsx! {};
    }

    rsx! {
        // 遮罩：点击关闭
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm sm:p-6 modal-overlay animate-modal-overlay-enter",
            class: if is_closing { "is-closing" } else { "" },
            onclick: move |_| {
                closing.set(true);
                visible.set(false);
            },
            // 面板：阻止点击穿透到遮罩
            div {
                class: "flex max-h-[80vh] min-h-0 w-full max-w-3xl flex-col overflow-hidden rounded-[2rem] border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] shadow-xl modal-panel animate-modal-panel-enter",
                role: "dialog",
                aria_modal: "true",
                aria_label: "{title}",
                onclick: move |evt| evt.stop_propagation(),

                // 头部：标题、搜索、上传与关闭按钮共享同一组内边距。
                div { class: "flex flex-wrap items-center gap-3 border-b border-[var(--color-paper-border)] p-6",
                    h2 { class: "shrink-0 text-lg font-bold text-[var(--color-paper-primary)]",
                        "{title}"
                    }
                    FormInput {
                        r#type: "search",
                        placeholder: "搜索文件名 / alt",
                        value: query(),
                        class: INPUT_INLINE_CLASS,
                        oninput: move |v: String| query.set(v),
                    }
                    // 上传新图：成功后保留在当前网格，用户点击后才选中
                    label { class: "inline-flex shrink-0 cursor-pointer items-center justify-center {BTN_PRIMARY}",
                        class: if cover_uploading() { "pointer-events-none cursor-wait opacity-60" } else { "" },
                        "上传新图"
                        input {
                            r#type: "file",
                            accept: "image/jpeg,image/png,image/gif,image/webp",
                            class: "hidden",
                            disabled: cover_uploading(),
                            onchange: move |evt| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    use dioxus::html::HasFileData;
                                    use dioxus::web::WebFileExt;
                                    if cover_uploading() {
                                        return;
                                    }
                                    if let Some(file) = evt.files().into_iter().next() {
                                        if let Some(web_file) = file.get_web_file() {
                                            let preview_url =
                                                web_sys::Url::create_object_url_with_blob(&web_file).ok();
                                            uploading_preview.set(preview_url.clone());
                                            cover_uploading.set(true);
                                            error.set(None);
                                            spawn(async move {
                                                let result = crate::tiptap_bridge::upload_image_file(web_file).await;
                                                if let Some(preview_url) = preview_url.as_deref() {
                                                    let _ = web_sys::Url::revoke_object_url(preview_url);
                                                }
                                                match result {
                                                    Ok(url) => {
                                                        uploaded_url.set(Some(url));
                                                        uploading_preview.set(None);
                                                        cover_uploading.set(false);
                                                    }
                                                    // 失败留在 modal 内提示，不关闭。
                                                    Err(msg) => {
                                                        error.set(Some(msg));
                                                        uploading_preview.set(None);
                                                        cover_uploading.set(false);
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                            },
                        }
                    }
                    button {
                        class: "{BTN_ICON} shrink-0 rounded-full",
                        aria_label: "关闭",
                        onclick: move |_| {
                            closing.set(true);
                            visible.set(false);
                        },
                        "×"
                    }
                }

                // 网格内容区：与头部统一使用 p-6，min-h-0 保证面板内部滚动。
                div { class: "min-h-0 flex-1 overflow-y-auto p-6",
                    if let Some(err) = error() {
                        div { class: "rounded-2xl bg-red-500/10 px-4 py-3 text-center text-sm text-red-600 dark:text-red-400",
                            "加载失败：{err}"
                        }
                    } else if loading() && assets.read().is_empty() && !cover_uploading() {
                        div { class: "px-4 py-16 text-center text-sm text-[var(--color-paper-secondary)]",
                            "加载中..."
                        }
                    } else if assets.read().is_empty() && !cover_uploading() && uploaded_url().is_none() {
                        div { class: "px-4 py-16 text-center text-sm text-[var(--color-paper-secondary)]",
                            "素材库为空，点击「上传新图」添加"
                        }
                    } else {
                        div { class: "grid grid-cols-3 gap-4 sm:grid-cols-4",
                            if cover_uploading() {
                                div {
                                    class: "relative aspect-square overflow-hidden rounded-2xl border border-[var(--color-paper-accent)]/60 bg-[var(--color-paper-theme)] shadow-sm",
                                    aria_live: "polite",
                                    aria_label: "正在上传图片",
                                    if let Some(preview_url) = uploading_preview() {
                                        img {
                                            class: "h-full w-full scale-110 object-cover blur-md opacity-60",
                                            src: "{preview_url}",
                                            alt: "正在上传",
                                        }
                                    } else {
                                        div { class: "absolute inset-0 animate-pulse bg-[var(--color-paper-code-bg)]" }
                                    }
                                    div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-black/30 backdrop-blur-[2px]",
                                        span {
                                            class: "inline-flex h-5 w-5 text-white",
                                            dangerous_inner_html: SPINNER_SVG,
                                        }
                                        span { class: "text-xs font-medium text-white drop-shadow", "上传中" }
                                    }
                                }
                            }
                            if let Some(uploaded_url) = uploaded_url() {
                                {
                                    let uploaded_url_for_select = uploaded_url.clone();
                                    rsx! {
                                        button {
                                            key: "uploaded-{uploaded_url}",
                                            class: "group relative aspect-square cursor-pointer overflow-hidden rounded-2xl border-2 border-[var(--color-paper-accent)] bg-[var(--color-paper-theme)] shadow-sm transition-all hover:shadow-md",
                                            title: "新上传图片，点击使用",
                                            onclick: move |_| {
                                                on_select.call(uploaded_url_for_select.clone());
                                                closing.set(true);
                                                visible.set(false);
                                            },
                                            img {
                                                class: "h-full w-full object-cover",
                                                src: "{uploaded_url}",
                                                alt: "新上传图片",
                                            }
                                            span { class: "absolute inset-x-2 bottom-2 rounded-full bg-black/55 px-2 py-1 text-center text-xs font-medium text-white backdrop-blur-sm", "刚上传" }
                                        }
                                    }
                                }
                            }
                            for asset in assets.read().iter() {
                                {
                                    let url = format!("/uploads/{}", asset.asset.path);
                                    let thumb = format!("{}?thumb=300x300", url);
                                    rsx! {
                                        button {
                                            key: "{asset.asset.id}",
                                            class: "group relative aspect-square cursor-pointer overflow-hidden rounded-2xl border border-[var(--color-paper-border)] bg-[var(--color-paper-theme)] transition-all hover:border-[var(--color-paper-primary)] hover:shadow-md",
                                            title: "{asset.asset.filename}",
                                            onclick: {
                                                let url = url.clone();
                                                move |_| {
                                                    on_select.call(url.clone());
                                                    closing.set(true);
                                                    visible.set(false);
                                                }
                                            },
                                            img {
                                                class: "h-full w-full object-cover",
                                                src: "{thumb}",
                                                alt: asset.asset.alt.clone().unwrap_or_else(|| { asset.asset.filename.clone() }),
                                                loading: "lazy",
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
