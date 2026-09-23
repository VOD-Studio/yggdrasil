//! 图鉴中的浏览器实例。每个预览拥有独立 DOM 宿主，正式页面与这里复用同一 JS 库。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::bridges::library::{library_ready, use_browser_library, LibraryLoadError};

#[cfg(target_arch = "wasm32")]
const TIPTAP_SAMPLE: &str = "# 一页正在生长的笔记\n\n选中这句话，可以试试加粗或链接。\n\n- 编辑正文\n- 切换 Markdown 源码\n\n> 图鉴里的修改只留在本页。";
#[cfg(target_arch = "wasm32")]
const TERMINAL_STDOUT: &str = "\x1b[32m✓\x1b[0m 正在整理文章样例…\n已读取 3 个本地条目\n这是一段较长的输出，用于观察终端在窄屏下如何换行和滚动：Yggdrasil showcase terminal preview output line.\n";
#[cfg(target_arch = "wasm32")]
const TERMINAL_STDERR: &str = "提示：这只是固定输出示例。\n";

fn host_id(slug: &str, detail: bool) -> String {
    format!(
        "showcase-browser-{slug}-{}",
        if detail { "detail" } else { "card" }
    )
}

#[cfg(any(target_arch = "wasm32", test))]
fn code_sample(language: &str) -> &'static str {
    match language {
        "rust" => "fn main() {\n    println!(\"让想法生根\");\n}\n",
        "sql" => "SELECT title, published_at\nFROM posts\nORDER BY published_at DESC\nLIMIT 3;\n",
        _ => "def greet(name):\n    return f'你好，{name}'\n\nprint(greet('Yggdrasil'))\n",
    }
}

/// 按稳定 slug 分派；卡片上的重型库只在首次进入视口后加载。
pub(super) fn preview(slug: &str, detail: bool) -> Option<Element> {
    match slug {
        "tiptap-editor" | "code-mirror-editor" | "xterm-terminal" | "lightbox" => {
            Some(rsx! { BrowserPreview { slug: slug.to_string(), detail } })
        }
        _ => None,
    }
}

#[component]
fn BrowserPreview(slug: String, detail: bool) -> Element {
    let lightbox = slug == "lightbox";
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut visible = use_signal(move || lightbox);
    let visibility_id = host_id(&slug, detail);

    #[cfg(target_arch = "wasm32")]
    {
        use std::{cell::RefCell, rc::Rc};
        use wasm_bindgen::{closure::Closure, JsCast};

        type Watch = (
            web_sys::IntersectionObserver,
            Closure<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>,
        );
        let watcher = use_hook(|| Rc::new(RefCell::new(None::<Watch>)));
        let watcher_for_effect = watcher.clone();
        let target_id = visibility_id.clone();
        use_effect(move || {
            if let Some((observer, _callback)) = watcher_for_effect.borrow_mut().take() {
                observer.disconnect();
            }
            if visible() {
                return;
            }
            if detail {
                visible.set(true);
                return;
            }
            let Some(target) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(&target_id))
            else {
                return;
            };
            let callback = Closure::new(
                move |entries: js_sys::Array, observer: web_sys::IntersectionObserver| {
                    if entries.iter().any(|value| {
                        value
                            .unchecked_into::<web_sys::IntersectionObserverEntry>()
                            .is_intersecting()
                    }) {
                        observer.disconnect();
                        visible.set(true);
                    }
                },
            );
            if let Ok(observer) =
                web_sys::IntersectionObserver::new(callback.as_ref().unchecked_ref())
            {
                observer.observe(&target);
                watcher_for_effect
                    .borrow_mut()
                    .replace((observer, callback));
            } else {
                // 老浏览器没有 IntersectionObserver 时仍可打开预览。
                visible.set(true);
            }
        });
        use_drop(move || {
            if let Some((observer, _callback)) = watcher.borrow_mut().take() {
                observer.disconnect();
            }
        });
    }

    rsx! {
        div {
            class: if detail { "showcase-browser-preview showcase-browser-preview--detail" } else { "showcase-browser-preview" },
            id: visibility_id,
            if visible() {
                match slug.as_str() {
                    "tiptap-editor" => rsx! { TiptapPreview { detail } },
                    "code-mirror-editor" => rsx! { CodeMirrorPreview { detail } },
                    "xterm-terminal" => rsx! { XtermPreview { detail } },
                    _ => rsx! { LightboxPreview { detail } },
                }
            } else {
                div { class: "showcase-browser-placeholder", role: "status",
                    if detail { "正在加载真实组件…" } else { "滚动到这里后加载真实组件…" }
                }
            }
        }
    }
}

#[component]
fn TiptapPreview(detail: bool) -> Element {
    let container_id = host_id("tiptap-host", detail);
    #[cfg(target_arch = "wasm32")]
    let mut error = use_signal(|| None::<String>);
    #[cfg(target_arch = "wasm32")]
    let mut ready = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    let mut changed = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    let mut retry = use_signal(|| 0_u32);

    #[cfg(target_arch = "wasm32")]
    {
        use crate::bridges::tiptap::{self, EditorHandle};
        use wasm_bindgen::{closure::Closure, JsValue};

        let library = use_browser_library("tiptap", || true);
        let mut handle: Signal<Option<EditorHandle>> = use_signal(|| None);
        let mount_id = container_id.clone();
        use_effect(move || {
            let _attempt = retry();
            if handle.read().is_some() || !library_ready(library) {
                return;
            }
            let on_update = Closure::new(move |_markdown: String| changed.set(true));
            let on_ready = Closure::new(|| {});
            // 未注入上传和素材库回调：相关命令不会进入正式业务路径。
            let on_upload = Closure::new(|_file: web_sys::File| {
                js_sys::Promise::reject(&JsValue::from_str("图鉴不上传图片"))
            });
            let on_upload_event = Closure::new(|_event: tiptap::UploadEventJs| {});
            let on_run = Closure::new(|_opts: tiptap::wasm::RunCodeOptsJs| {
                js_sys::Promise::resolve(&JsValue::from_str("输出示例：图鉴不会执行代码。"))
            });
            let on_pick = Closure::new(|| {});
            let opts = tiptap::EditorOptions::new();
            opts.set_placeholder("在这里写一小段内容…");
            opts.set_on_update(&on_update);
            opts.set_on_ready(&on_ready);
            opts.set_on_run_code(&on_run);
            match tiptap::get_module().create(&mount_id, &opts) {
                Ok(Some(instance)) => {
                    instance.set_markdown(TIPTAP_SAMPLE);
                    handle.set(Some(EditorHandle::new(
                        instance,
                        on_update,
                        on_upload,
                        on_ready,
                        on_upload_event,
                        on_run,
                        on_pick,
                    )));
                    ready.set(true);
                    error.set(None);
                }
                Ok(None) => error.set(Some("编辑器宿主未就绪".to_string())),
                Err(reason) => error.set(Some(format!("编辑器初始化失败：{reason:?}"))),
            }
        });
        use_drop(move || handle.set(None));

        return rsx! {
            div { class: "showcase-browser-toolbar",
                span { "Tiptap · Markdown" }
                if detail { small { "本地演示，内容不会保存" } }
            }
            div { class: "showcase-browser-host showcase-browser-host--tiptap", id: container_id }
            if !ready() && error().is_none() && library_ready(library) { div { class: "showcase-browser-placeholder", "编辑器正在初始化…" } }
            if !ready() && library.read().is_none() { div { class: "showcase-browser-placeholder", "编辑器库加载中…" } }
            LibraryLoadError { library }
            if let Some(message) = error() {
                div { role: "alert", class: "showcase-browser-error", "{message}"
                    button { r#type: "button", onclick: move |_| retry += 1, "重试" }
                }
            }
            if detail && changed() { p { class: "showcase-browser-note", "内容已在本地修改；可用编辑器右上角按钮切换 Markdown 源码。" } }
        };
    }

    #[cfg(not(target_arch = "wasm32"))]
    rsx! {
        div { class: "showcase-browser-toolbar",
            span { "Tiptap · Markdown" }
            if detail { small { "本地演示，内容不会保存" } }
        }
        div { class: "showcase-browser-host showcase-browser-host--tiptap", id: container_id }
        div { class: "showcase-browser-placeholder", "编辑器库加载中…" }
    }
}

#[component]
fn CodeMirrorPreview(detail: bool) -> Element {
    let container_id = host_id("code-mirror-host", detail);
    #[cfg(target_arch = "wasm32")]
    let mut language = use_signal(|| "python".to_string());
    #[cfg(target_arch = "wasm32")]
    let mut error = use_signal(|| None::<String>);
    #[cfg(target_arch = "wasm32")]
    let mut ready = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    let mut shortcut_note = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    let mut retry = use_signal(|| 0_u32);

    #[cfg(target_arch = "wasm32")]
    {
        use crate::bridges::codemirror::{self, EditorHandle};
        use crate::theme::{use_resolved_theme, ResolvedTheme};
        use wasm_bindgen::closure::Closure;

        let library = use_browser_library("codemirror", || true);
        let resolved = use_resolved_theme();
        let mut handle: Signal<Option<EditorHandle>> = use_signal(|| None);
        let mount_id = container_id.clone();
        use_effect(move || {
            let _attempt = retry();
            if handle.read().is_some() || !library_ready(library) {
                return;
            }
            let on_change = Closure::new(|_value: String| {});
            let on_ready = Closure::new(|| {});
            let on_shortcut = Closure::new(move || shortcut_note.set(true));
            let opts = codemirror::EditorOptions::new();
            opts.set_language(&language());
            opts.set_theme(if resolved() == ResolvedTheme::Dark {
                "dark"
            } else {
                "light"
            });
            opts.set_value(code_sample(&language()));
            opts.set_on_change(&on_change);
            opts.set_on_ready(&on_ready);
            opts.set_on_run_shortcut(&on_shortcut);
            match codemirror::get_module().create(&mount_id, &opts) {
                Ok(Some(instance)) => {
                    handle.set(Some(EditorHandle::new(
                        instance,
                        on_change,
                        on_ready,
                        on_shortcut,
                    )));
                    ready.set(true);
                    error.set(None);
                }
                Ok(None) => error.set(Some("代码编辑器宿主未就绪".to_string())),
                Err(reason) => error.set(Some(format!("代码编辑器初始化失败：{reason:?}"))),
            }
        });
        use_effect(move || {
            let theme = if resolved() == ResolvedTheme::Dark {
                "dark"
            } else {
                "light"
            };
            if let Some(editor) = handle.read().as_ref() {
                editor.instance().set_theme(theme);
            }
        });
        use_drop(move || handle.set(None));

        return rsx! {
            div { class: "showcase-browser-toolbar",
                span { "CodeMirror · {language()}" }
                if detail {
                    label { "语言 "
                        select { value: language(), onchange: move |event| {
                            let next = event.value();
                            language.set(next.clone());
                            if let Some(editor) = handle.read().as_ref() {
                                editor.instance().set_language(&next);
                                editor.instance().set_value(code_sample(&next));
                            }
                        },
                            option { value: "python", "Python" }
                            option { value: "rust", "Rust" }
                            option { value: "sql", "SQL" }
                        }
                    }
                }
            }
            div { class: "showcase-browser-host showcase-browser-host--code", id: container_id }
            if !ready() && error().is_none() && library_ready(library) { div { class: "showcase-browser-placeholder", "代码编辑器正在初始化…" } }
            if !ready() && library.read().is_none() { div { class: "showcase-browser-placeholder", "代码编辑器库加载中…" } }
            LibraryLoadError { library }
            if let Some(message) = error() {
                div { role: "alert", class: "showcase-browser-error", "{message}"
                    button { r#type: "button", onclick: move |_| retry += 1, "重试" }
                }
            }
            if detail {
                p { class: "showcase-browser-note", "可编辑、撤销并切换语法高亮；代码只保存在当前预览。" }
                if shortcut_note() { p { role: "status", class: "showcase-browser-note", "Ctrl/Cmd+Enter 在图鉴中不会执行代码。" } }
            }
        };
    }

    #[cfg(not(target_arch = "wasm32"))]
    rsx! {
        div { class: "showcase-browser-toolbar",
            span { "CodeMirror · python" }
            if detail {
                label { "语言 "
                    select { value: "python",
                        option { value: "python", "Python" }
                        option { value: "rust", "Rust" }
                        option { value: "sql", "SQL" }
                    }
                }
            }
        }
        div { class: "showcase-browser-host showcase-browser-host--code", id: container_id }
        div { class: "showcase-browser-placeholder", "代码编辑器库加载中…" }
        if detail { p { class: "showcase-browser-note", "可编辑、撤销并切换语法高亮；代码只保存在当前预览。" } }
    }
}

#[component]
fn XtermPreview(detail: bool) -> Element {
    let container_id = host_id("xterm-host", detail);
    #[cfg(target_arch = "wasm32")]
    let mut error = use_signal(|| None::<String>);
    #[cfg(target_arch = "wasm32")]
    let mut ready = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    let mut status = use_signal(|| "输出示例已就绪".to_string());
    #[cfg(target_arch = "wasm32")]
    let mut retry = use_signal(|| 0_u32);

    #[cfg(target_arch = "wasm32")]
    {
        use crate::bridges::xterm::{self, TerminalHandle};
        use crate::theme::{use_resolved_theme, ResolvedTheme};
        use std::{cell::Cell, rc::Rc};
        use wasm_bindgen::closure::Closure;

        let library = use_browser_library("xterm", || true);
        let resolved = use_resolved_theme();
        let mut handle: Signal<Option<TerminalHandle>> = use_signal(|| None);
        let generation = use_hook(|| Rc::new(Cell::new(0_u64)));
        let mount_id = container_id.clone();
        use_effect(move || {
            let _attempt = retry();
            if handle.read().is_some() || !library_ready(library) {
                return;
            }
            let on_ready = Closure::new(|| {});
            let opts = xterm::XtermOptions::new();
            opts.set_theme(if resolved() == ResolvedTheme::Dark {
                "dark"
            } else {
                "light"
            });
            opts.set_font_size(if detail { 13 } else { 11 });
            opts.set_on_ready(&on_ready);
            match xterm::get_module().create(&mount_id, &opts) {
                Ok(Some(instance)) => {
                    instance.write_all(TERMINAL_STDOUT, TERMINAL_STDERR);
                    handle.set(Some(TerminalHandle::new(instance, on_ready)));
                    ready.set(true);
                    error.set(None);
                }
                Ok(None) => error.set(Some("终端宿主未就绪".to_string())),
                Err(reason) => error.set(Some(format!("终端初始化失败：{reason:?}"))),
            }
        });
        use_effect(move || {
            let theme = if resolved() == ResolvedTheme::Dark {
                "dark"
            } else {
                "light"
            };
            if let Some(terminal) = handle.read().as_ref() {
                terminal.instance().set_theme(theme);
            }
        });
        let drop_generation = generation.clone();
        use_drop(move || {
            drop_generation.set(drop_generation.get().wrapping_add(1));
            handle.set(None);
        });

        let play_generation = generation.clone();
        let pause_generation = generation.clone();
        let clear_generation = generation.clone();
        return rsx! {
            div { class: "showcase-browser-toolbar",
                span { "Xterm · 输出示例" }
                if detail {
                    div { class: "showcase-browser-actions",
                        button { r#type: "button", disabled: !ready(), onclick: move |_| {
                            let ticket = play_generation.get().wrapping_add(1);
                            play_generation.set(ticket);
                            if let Some(terminal) = handle.read().as_ref() {
                                terminal.instance().clear();
                            }
                            status.set("播放中".to_string());
                            let sequence = play_generation.clone();
                            spawn(async move {
                                for (stderr, text) in [
                                    (false, "\x1b[32m✓\x1b[0m 正在整理文章样例…\n"),
                                    (false, "已读取 3 个本地条目\n"),
                                    (true, "提示：这只是固定输出示例。\n"),
                                    (false, "这是一段较长的输出，用于观察终端在窄屏下如何换行和滚动：Yggdrasil showcase terminal preview output line.\n"),
                                ] {
                                    if sequence.get() != ticket { return; }
                                    if let Some(terminal) = handle.read().as_ref() {
                                        if stderr { terminal.instance().write_stderr(text); }
                                        else { terminal.instance().write_stdout(text); }
                                    }
                                    crate::utils::time::sleep_ms(330).await;
                                }
                                if sequence.get() == ticket { status.set("播放完成，可重放".to_string()); }
                            });
                        }, "播放 / 重放" }
                        button { r#type: "button", disabled: !ready(), onclick: move |_| {
                            pause_generation.set(pause_generation.get().wrapping_add(1));
                            status.set("已暂停".to_string());
                        }, "暂停" }
                        button { r#type: "button", disabled: !ready(), onclick: move |_| {
                            clear_generation.set(clear_generation.get().wrapping_add(1));
                            if let Some(terminal) = handle.read().as_ref() { terminal.instance().clear(); }
                            status.set("已清屏".to_string());
                        }, "清屏" }
                    }
                }
            }
            div { class: "showcase-browser-host showcase-browser-host--xterm", id: container_id }
            if !ready() && error().is_none() && library_ready(library) { div { class: "showcase-browser-placeholder", "终端正在初始化…" } }
            if !ready() && library.read().is_none() { div { class: "showcase-browser-placeholder", "终端库加载中…" } }
            LibraryLoadError { library }
            if let Some(message) = error() {
                div { role: "alert", class: "showcase-browser-error", "{message}"
                    button { r#type: "button", onclick: move |_| retry += 1, "重试" }
                }
            }
            if detail { p { class: "showcase-browser-note", role: "status", "{status()}。固定数据，与编辑的代码无关。" } }
        };
    }

    #[cfg(not(target_arch = "wasm32"))]
    rsx! {
        div { class: "showcase-browser-toolbar",
            span { "Xterm · 输出示例" }
            if detail {
                div { class: "showcase-browser-actions",
                    button { r#type: "button", disabled: true, "播放 / 重放" }
                    button { r#type: "button", disabled: true, "暂停" }
                    button { r#type: "button", disabled: true, "清屏" }
                }
            }
        }
        div { class: "showcase-browser-host showcase-browser-host--xterm", id: container_id }
        div { class: "showcase-browser-placeholder", "终端库加载中…" }
        if detail { p { class: "showcase-browser-note", role: "status", "输出示例已就绪。固定数据，与编辑的代码无关。" } }
    }
}

#[component]
fn LightboxPreview(detail: bool) -> Element {
    let owner = host_id("lightbox-gallery", detail);
    #[cfg(target_arch = "wasm32")]
    {
        let init_owner = owner.clone();
        use_effect(move || {
            if !detail {
                return;
            }
            if let Some(window) = web_sys::window() {
                crate::utils::js::invoke_optional_global(
                    &window,
                    "__initLightbox",
                    &[format!("#{init_owner}").into()],
                );
            }
        });
        let cleanup_owner = owner.clone();
        use_drop(move || {
            if let Some(window) = web_sys::window() {
                crate::utils::js::invoke_optional_global(
                    &window,
                    "__closeLightboxFor",
                    &[cleanup_owner.into()],
                );
            }
        });
    }
    rsx! {
        div { class: "showcase-browser-lightbox-grid", id: owner.clone(), "data-showcase-lightbox-owner": owner,
            img { src: "/images/xiaotiaoxiaogou_01.webp", alt: "小狗望向镜头", loading: "lazy" }
            img { src: "/images/xiantiaoxiaogou_02.webp", alt: "小狗的第二张照片", loading: "lazy" }
            img { src: "/images/xiantiaoxiaogou_03.webp", alt: "小狗的第三张照片", loading: "lazy" }
        }
        if detail { p { class: "showcase-browser-note", "点击图片打开灯箱，可切换、缩放、旋转，并用 Esc 关闭。" } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_card_and_detail_hosts() {
        for slug in [
            "tiptap-host",
            "code-mirror-host",
            "xterm-host",
            "lightbox-gallery",
        ] {
            assert_ne!(host_id(slug, false), host_id(slug, true));
        }
    }

    #[test]
    fn language_samples_are_real_documents() {
        assert!(code_sample("python").contains("def greet"));
        assert!(code_sample("rust").contains("fn main"));
        assert!(code_sample("sql").contains("SELECT"));
    }
}
