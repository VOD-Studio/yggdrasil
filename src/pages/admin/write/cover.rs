//! 封面上传、素材选择与预览，上传临时状态仅在本组件中持有。
use super::state::PostDraft;
#[cfg(target_arch = "wasm32")]
use crate::bridges::tiptap::upload_image_file;
use crate::components::assets::{AssetPickerModal, AssetSelection};
use crate::components::forms::{FormInput, INPUT_INLINE_CLASS};
use crate::components::ui::{BTN_CLOSE_ICON, BTN_PRIMARY_SM};
#[cfg(target_arch = "wasm32")]
use crate::utils::js::invoke_optional_global;
#[cfg(target_arch = "wasm32")]
use dioxus::html::HasFileData;
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use dioxus::web::{WebEventExt, WebFileExt};

/// 封面上传子组件。
///
/// 封装封面图的全部状态与交互：拖拽/粘贴/选择文件上传、URL 输入、预览、移除。
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
                    cover_error.set(Some(msg));
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

    rsx! {
        // P5 §8.2：封面预览灯箱按需注入（脚本未就绪时迟到调用入队兜底）。
        crate::components::enhance_assets::LightboxAssets {}
        // 封面图上传区：空态矮横条（不挤压编辑器），有图时展开成 21:9 超宽预览。
        // 21:9 与首页卡片封面统一比例，比 16:9 更扁，适合宽屏横幅式封面。
        // 容器统一绑定拖拽与粘贴事件；内部按 cover_image / cover_uploading 切换空态、上传中、预览。
        div {
            class: "relative w-full border border-dashed rounded-2xl overflow-hidden transition-all duration-200 group/cover",
            class: "cover-preview",
            // 空态矮横条；有图/上传中展开成 21:9。
            class: if cover_image().is_empty() && !cover_uploading() { "h-14" } else { "aspect-[21/9]" },
            class: if cover_drag_active() { "border-[var(--color-paper-primary)] bg-[var(--color-paper-entry)]" } else if cover_image().is_empty() { "border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] hover:border-[var(--color-paper-primary)]" } else { "border-[var(--color-paper-border)] bg-[var(--color-paper-entry)]" },

            // 整个容器可接收拖拽与粘贴（ondragover 必须 prevent_default，否则浏览器直接打开文件）。
            ondragover: move |evt| {
                evt.prevent_default();
                if !cover_uploading() {
                    cover_drag_active.set(true);
                }
            },
            ondragenter: move |evt| {
                evt.prevent_default();
            },
            ondragleave: move |_| {
                cover_drag_active.set(false);
            },
            ondrop: move |evt| {
                evt.prevent_default();
                if !cover_uploading() {
                    #[cfg(target_arch = "wasm32")]
                    {
                        if let Some(file) = evt.files().into_iter().next() {
                            if let Some(web_file) = file.get_web_file() {
                                spawn_cover_upload(web_file);
                            }
                        }
                    }
                }
            },
            onpaste: move |evt| {
                evt.prevent_default();
                if !cover_uploading() {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use wasm_bindgen::JsCast;
                        if let Some(raw) = evt.try_as_web_event() {
                            if let Some(ce) = raw.dyn_ref::<web_sys::ClipboardEvent>() {
                                if let Some(dt) = ce.clipboard_data() {
                                    if let Some(file_list) = dt.files() {
                                        if let Some(file) = file_list.item(0) {
                                            spawn_cover_upload(file);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },

            // —— 上传中：骨架占位 + 文案 ——
            if cover_uploading() {
                div { class: "absolute inset-0 flex flex-col items-center justify-center gap-3 bg-[var(--color-paper-tertiary)]/30 animate-pulse",
                    span { class: "text-sm font-medium text-[var(--color-paper-secondary)]",
                        "正在上传..."
                    }
                }
            }

            // —— 有图：预览（点击放大）+ 拖拽替换 + hover 工具栏 ——
            if !cover_image().is_empty() && !cover_uploading() {
                // 预览图：/uploads/ 路径加 ?w=600 缩略，外链 URL 原样用。
                // cursor-zoom-in 提示可点击放大；lightbox-single 标记为单张，
                // 由全局 lightbox.js 接管点击放大；data-src 为原图 URL。
                img {
                    class: "absolute inset-0 w-full h-full object-cover cursor-zoom-in lightbox-single",
                    // data-src：原图 URL，lightbox.js 点击放大时读取（originalUrl 剥离 /uploads/ 的 ?w= 查询）。
                    "data-src": "{cover_image()}",
                    src: {
                        let cv = cover_image();
                        if cv.starts_with("/uploads/") {
                            let base = cv.split('?').next().unwrap_or(&cv);
                            if cv.contains('?') {
                                format!("{}&w=600", base)
                            } else {
                                format!("{}?w=600", base)
                            }
                        } else {
                            cv
                        }
                    },
                    alt: "封面预览",
                    // 加载完成清除骨架占位；失败也清除并提示（避免骨架永驻）。
                    onload: move |_| cover_img_loaded.set(true),
                    onerror: move |_| {
                        cover_img_loaded.set(true);
                        cover_error.set(Some("封面图加载失败，请检查 URL".to_string()));
                    },
                }
                // 预览图加载中骨架占位（素材库选择/拖拽替换/URL 切换后，?w=600 未缓存时）。
                if !cover_img_loaded() {
                    div { class: "absolute inset-0 animate-pulse bg-[var(--color-paper-tertiary)]/40 pointer-events-none" }
                }
                // 拖拽替换高亮：有图时拖入文件，提示"松开以更换"。
                if cover_drag_active() {
                    div { class: "absolute inset-0 flex items-center justify-center bg-black/50 pointer-events-none",
                        span { class: "text-sm font-medium text-white drop-shadow-md",
                            "松开以更换封面"
                        }
                    }
                }
                // hover 工具栏：拖拽时隐藏（由拖拽替换高亮接管），避免两个覆盖层叠加。
                // pointer-events-none 容器穿透点击到预览图触发灯箱，按钮设 pointer-events-auto。
                if !cover_drag_active() {
                    div { class: "absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/60 to-transparent opacity-0 group-hover/cover:opacity-100 transition-opacity flex items-center justify-center gap-1 py-2 pointer-events-none",
                        // 更换：label 包裹隐藏 file input，点击触发文件选择。
                        label { class: "pointer-events-auto px-3 py-1.5 rounded-full text-xs font-medium text-white/90 hover:bg-white/15 cursor-pointer transition-colors",
                            "更换"
                            input {
                                r#type: "file",
                                accept: "image/jpeg,image/png,image/gif,image/webp",
                                class: "hidden",
                                onchange: move |evt| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        if let Some(file) = evt.files().into_iter().next() {
                                            if let Some(web_file) = file.get_web_file() {
                                                spawn_cover_upload(web_file);
                                            }
                                        }
                                    }
                                },
                            }
                        }
                        // 从素材库选择。
                        button {
                            class: "pointer-events-auto px-3 py-1.5 rounded-full text-xs font-medium text-white/90 hover:bg-white/15 transition-colors cursor-pointer",
                            onclick: move |_| picker_visible.set(true),
                            "素材库"
                        }
                        // 移除封面。
                        button {
                            class: "pointer-events-auto px-3 py-1.5 rounded-full text-xs font-medium text-white/90 hover:bg-white/15 transition-colors cursor-pointer",
                            onclick: move |_| {
                                form.write().cover_image = String::new();
                                cover_error.set(None);
                                cover_url_mode.set(false);
                                cover_url_input.set(String::new());
                            },
                            "移除"
                        }
                    }
                }
            }

            // —— 空态：横向矮横条，图标+提示+URL 链 ——
            if cover_image().is_empty() && !cover_uploading() {
                // label 包裹整个空态：点击天然触发隐藏的 file input，无需 JS。
                label { class: "absolute inset-0 flex flex-row items-center gap-3 cursor-pointer px-4 text-left",
                    // 上传图标（Feather 风格线框，与项目现有图标体系一致）。
                    svg {
                        class: "w-5 h-5 shrink-0 text-[var(--color-paper-secondary)]",
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.8",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                        polyline { points: "17 8 12 3 7 8" }
                        line {
                            x1: "12",
                            y1: "3",
                            x2: "12",
                            y2: "15",
                        }
                    }
                    span { class: "text-sm font-medium text-[var(--color-paper-secondary)] shrink-0",
                        "拖拽 / 粘贴 / 点击上传"
                    }
                    // 从素材库选择：阻止 label 默认 file 触发，打开选择 modal。
                    span {
                        class: "text-sm font-medium text-[var(--color-paper-tertiary)] hover:text-[var(--color-paper-primary)] transition-colors ml-auto shrink-0",
                        onclick: move |evt| {
                            evt.prevent_default();
                            evt.stop_propagation();
                            picker_visible.set(true);
                        },
                        "素材库"
                    }
                    // URL 文字链：阻止 label 的默认 file 触发，切换到 URL 输入模式。
                    span {
                        class: "text-sm font-medium text-[var(--color-paper-tertiary)] hover:text-[var(--color-paper-primary)] transition-colors shrink-0",
                        onclick: move |evt| {
                            evt.prevent_default();
                            evt.stop_propagation();
                            cover_url_mode.set(true);
                            cover_url_input.set(cover_image());
                        },
                        "输入链接"
                    }
                    // 隐藏的 file input，由 label 点击触发。
                    input {
                        r#type: "file",
                        accept: "image/jpeg,image/png,image/gif,image/webp",
                        class: "hidden",
                        onchange: move |evt| {
                            #[cfg(target_arch = "wasm32")]
                            {
                                if let Some(file) = evt.files().into_iter().next() {
                                    if let Some(web_file) = file.get_web_file() {
                                        spawn_cover_upload(web_file);
                                    }
                                }
                            }
                            // 注意：未重置 input.value，重复选择同一文件不会再次触发 onchange。
                            // 这是 file input 的通用行为，封面场景影响可忽略。
                        },
                    }
                }
            }
        }

        // —— URL 输入模式（内联展开，空态时叠加在容器外，避免与拖拽区争抢点击）——
        if cover_url_mode() && cover_image().is_empty() {
            div { class: "flex items-center gap-2 mt-2",
                FormInput {
                    r#type: "url",
                    placeholder: "粘贴图片链接...",
                    value: cover_url_input(),
                    class: INPUT_INLINE_CLASS,
                    oninput: move |v: String| cover_url_input.set(v),
                    onkeydown: move |evt: KeyboardEvent| {
                        if evt.key() == Key::Enter {
                            let v = cover_url_input().trim().to_string();
                            if !v.is_empty() {
                                form.write().cover_image = v;
                                cover_error.set(None);
                                cover_url_mode.set(false);
                            }
                        }
                    },
                }
                button {
                    class: "shrink-0 {BTN_PRIMARY_SM}",
                    onclick: move |_| {
                        let v = cover_url_input().trim().to_string();
                        if !v.is_empty() {
                            form.write().cover_image = v;
                            cover_error.set(None);
                            cover_url_mode.set(false);
                        }
                    },
                    "确认"
                }
                button {
                    class: "shrink-0 px-4 py-1.5 rounded-full text-sm font-medium text-[var(--color-paper-secondary)] hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                    onclick: move |_| {
                        cover_url_mode.set(false);
                        cover_url_input.set(String::new());
                    },
                    "取消"
                }
            }
        }

        // 封面上传失败提示：复用页面红色条风格。
        if let Some(err) = cover_error() {
            div { class: "flex items-center justify-between gap-3 px-4 py-2 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-2xl text-sm border border-red-100 dark:border-red-900/30 mt-2",
                span { "封面图: {err}" }
                button {
                    class: "{BTN_CLOSE_ICON}",
                    aria_label: "关闭提示",
                    onclick: move |_| cover_error.set(None),
                    "×"
                }
            }
        }

        // 素材选择 modal：选中回填封面 URL（上传新图成功同样回填）。
        AssetPickerModal {
            visible: picker_visible,
            cover_uploading,
            on_select: move |picks: Vec<AssetSelection>| {
                // 单选模式：载荷恰含一个元素。
                if let Some(first) = picks.into_iter().next() {
                    form.write().cover_image = first.url;
                    cover_error.set(None);
                }
            },
        }
    }
}
