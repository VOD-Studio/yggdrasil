//! 图鉴中的浏览器实例。每个预览拥有独立 DOM 宿主，正式页面与这里复用同一 JS 库。
//!
//! 水合一致：SSR 与水合首帧输出同一棵视图（库加载中占位）；浏览器专属逻辑
//! 只出现在 Effect / 事件 / `on_cleanup` 里，句柄放在 `StoredValue::new_local`，
//! 卸载时 take 出来丢弃即触发 `Drop` → `destroy`。

use leptos::prelude::*;

use crate::bridges::library::{
    library_ready, use_browser_library, LibraryLoadError, LibraryResource,
};

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
pub(super) fn preview(slug: &str, detail: bool) -> Option<AnyView> {
    match slug {
        "tiptap-editor" | "code-mirror-editor" | "xterm-terminal" | "lightbox" => {
            Some(view! { <BrowserPreview slug=slug.to_string() detail/> }.into_any())
        }
        _ => None,
    }
}

/// 初始化失败提示条 + 重试按钮。
fn error_alert(error: RwSignal<Option<String>>, retry: RwSignal<u32>) -> impl IntoView {
    move || {
        error.get().map(|message| {
            view! {
                <div role="alert" class="showcase-browser-error">
                    {message}
                    <button
                        type="button"
                        on:click=move |_| retry.update(|value| *value = value.wrapping_add(1))
                    >
                        "重试"
                    </button>
                </div>
            }
        })
    }
}

/// 实例初始化中 / 库加载中的两条占位。
fn loading_placeholders(
    library: LibraryResource,
    ready: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    initializing: &'static str,
    loading: &'static str,
) -> impl IntoView {
    view! {
        {move || {
            (!ready.get() && error.read().is_none() && library_ready(library))
                .then(|| view! { <div class="showcase-browser-placeholder">{initializing}</div> })
        }}
        {move || {
            (!ready.get() && library.read_state().is_none())
                .then(|| view! { <div class="showcase-browser-placeholder">{loading}</div> })
        }}
    }
}

#[component]
fn BrowserPreview(slug: String, detail: bool) -> impl IntoView {
    let lightbox = slug == "lightbox";
    let visible = RwSignal::new(lightbox);
    let visibility_id = host_id(&slug, detail);

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{closure::Closure, JsCast};

        type Watch = (
            web_sys::IntersectionObserver,
            Closure<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>,
        );
        let watcher = StoredValue::new_local(None::<Watch>);
        let target_id = visibility_id.clone();
        Effect::new(move |_| {
            if let Some(Some((observer, _callback))) = watcher.try_update_value(Option::take) {
                observer.disconnect();
            }
            if visible.get() {
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
                        let _ = visible.try_set(true);
                    }
                },
            );
            if let Ok(observer) =
                web_sys::IntersectionObserver::new(callback.as_ref().unchecked_ref())
            {
                observer.observe(&target);
                watcher.set_value(Some((observer, callback)));
            } else {
                // 老浏览器没有 IntersectionObserver 时仍可打开预览。
                visible.set(true);
            }
        });
        on_cleanup(move || {
            if let Some(Some((observer, _callback))) = watcher.try_update_value(Option::take) {
                observer.disconnect();
            }
        });
    }

    let class = if detail {
        "showcase-browser-preview showcase-browser-preview--detail"
    } else {
        "showcase-browser-preview"
    };
    view! {
        <div class=class id=visibility_id>
            {move || {
                if visible.get() {
                    match slug.as_str() {
                        "tiptap-editor" => view! { <TiptapPreview detail/> }.into_any(),
                        "code-mirror-editor" => view! { <CodeMirrorPreview detail/> }.into_any(),
                        "xterm-terminal" => view! { <XtermPreview detail/> }.into_any(),
                        _ => view! { <LightboxPreview detail/> }.into_any(),
                    }
                } else {
                    view! {
                        <div class="showcase-browser-placeholder" role="status">
                            {if detail { "正在加载真实组件…" } else { "滚动到这里后加载真实组件…" }}
                        </div>
                    }
                    .into_any()
                }
            }}
        </div>
    }
}

#[component]
fn TiptapPreview(detail: bool) -> impl IntoView {
    let container_id = host_id("tiptap-host", detail);
    let error = RwSignal::new(None::<String>);
    let ready = RwSignal::new(false);
    let changed = RwSignal::new(false);
    let retry = RwSignal::new(0_u32);
    let library = use_browser_library("tiptap", || true);

    #[cfg(target_arch = "wasm32")]
    {
        use crate::bridges::tiptap::{self, EditorHandle};
        use wasm_bindgen::{closure::Closure, JsValue};

        let handle: StoredValue<Option<EditorHandle>, LocalStorage> = StoredValue::new_local(None);
        let mount_id = container_id.clone();
        Effect::new(move |_| {
            retry.track();
            if handle.with_value(|handle| handle.is_some()) || !library_ready(library) {
                return;
            }
            let on_update = Closure::new(move |_markdown: String| {
                let _ = changed.try_set(true);
            });
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
                    let _ = instance.set_markdown(TIPTAP_SAMPLE);
                    handle.set_value(Some(EditorHandle::new(
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
                Err(_) => error.set(Some("编辑器初始化失败".to_string())),
            }
        });
        on_cleanup(move || {
            let _ = handle.try_update_value(Option::take);
        });
    }

    view! {
        <div class="showcase-browser-toolbar">
            <span>"Tiptap · Markdown"</span>
            {detail.then(|| view! { <small>"本地演示，内容不会保存"</small> })}
        </div>
        <div class="showcase-browser-host showcase-browser-host--tiptap" id=container_id></div>
        {loading_placeholders(library, ready, error, "编辑器正在初始化…", "编辑器库加载中…")}
        <LibraryLoadError library/>
        {error_alert(error, retry)}
        {move || {
            (detail && changed.get())
                .then(|| {
                    view! {
                        <p class="showcase-browser-note">
                            "内容已在本地修改；可用编辑器右上角按钮切换 Markdown 源码。"
                        </p>
                    }
                })
        }}
    }
}

#[component]
fn CodeMirrorPreview(detail: bool) -> impl IntoView {
    let container_id = host_id("code-mirror-host", detail);
    let language = RwSignal::new("python".to_string());
    let error = RwSignal::new(None::<String>);
    let ready = RwSignal::new(false);
    let shortcut_note = RwSignal::new(false);
    let retry = RwSignal::new(0_u32);
    let library = use_browser_library("codemirror", || true);

    #[cfg(target_arch = "wasm32")]
    let handle: StoredValue<Option<crate::bridges::codemirror::EditorHandle>, LocalStorage> =
        StoredValue::new_local(None);

    #[cfg(target_arch = "wasm32")]
    {
        use crate::bridges::codemirror::{self, EditorHandle};
        use crate::theme::{use_resolved_theme, ResolvedTheme};
        use wasm_bindgen::closure::Closure;

        let resolved = use_resolved_theme();
        let mount_id = container_id.clone();
        Effect::new(move |_| {
            retry.track();
            if handle.with_value(|handle| handle.is_some()) || !library_ready(library) {
                return;
            }
            let on_change = Closure::new(|_value: String| {});
            let on_ready = Closure::new(|| {});
            let on_shortcut = Closure::new(move || {
                let _ = shortcut_note.try_set(true);
            });
            let current_language = language.get_untracked();
            let opts = codemirror::EditorOptions::new();
            opts.set_language(&current_language);
            opts.set_theme(if resolved.get_untracked() == ResolvedTheme::Dark {
                "dark"
            } else {
                "light"
            });
            opts.set_value(code_sample(&current_language));
            opts.set_on_change(&on_change);
            opts.set_on_ready(&on_ready);
            opts.set_on_run_shortcut(&on_shortcut);
            match codemirror::get_module().create(&mount_id, &opts) {
                Ok(Some(instance)) => {
                    handle.set_value(Some(EditorHandle::new(
                        instance,
                        on_change,
                        on_ready,
                        on_shortcut,
                    )));
                    ready.set(true);
                    error.set(None);
                }
                Ok(None) => error.set(Some("代码编辑器宿主未就绪".to_string())),
                Err(_) => error.set(Some("代码编辑器初始化失败".to_string())),
            }
        });
        Effect::new(move |_| {
            let theme = if resolved.get() == ResolvedTheme::Dark {
                "dark"
            } else {
                "light"
            };
            handle.with_value(|editor| {
                if let Some(editor) = editor {
                    editor.instance().set_theme(theme);
                }
            });
        });
        on_cleanup(move || {
            let _ = handle.try_update_value(Option::take);
        });
    }

    let on_language_change = move |event: leptos::ev::Event| {
        let next = event_target_value(&event);
        #[cfg(target_arch = "wasm32")]
        handle.with_value(|editor| {
            if let Some(editor) = editor {
                editor.instance().set_language(&next);
                editor.instance().set_value(code_sample(&next));
            }
        });
        language.set(next);
    };

    view! {
        <div class="showcase-browser-toolbar">
            <span>{move || format!("CodeMirror · {}", language.get())}</span>
            {detail
                .then(|| {
                    view! {
                        <label>
                            "语言 "
                            <select
                                prop:value=move || language.get()
                                on:change=on_language_change
                            >
                                <option value="python">"Python"</option>
                                <option value="rust">"Rust"</option>
                                <option value="sql">"SQL"</option>
                            </select>
                        </label>
                    }
                })}
        </div>
        <div class="showcase-browser-host showcase-browser-host--code" id=container_id></div>
        {loading_placeholders(
            library,
            ready,
            error,
            "代码编辑器正在初始化…",
            "代码编辑器库加载中…",
        )}
        <LibraryLoadError library/>
        {error_alert(error, retry)}
        {detail
            .then(|| {
                view! {
                    <p class="showcase-browser-note">
                        "可编辑、撤销并切换语法高亮；代码只保存在当前预览。"
                    </p>
                    {move || {
                        shortcut_note
                            .get()
                            .then(|| {
                                view! {
                                    <p role="status" class="showcase-browser-note">
                                        "Ctrl/Cmd+Enter 在图鉴中不会执行代码。"
                                    </p>
                                }
                            })
                    }}
                }
            })}
    }
}

#[component]
fn XtermPreview(detail: bool) -> impl IntoView {
    let container_id = host_id("xterm-host", detail);
    let error = RwSignal::new(None::<String>);
    let ready = RwSignal::new(false);
    let status = RwSignal::new("输出示例已就绪".to_string());
    let retry = RwSignal::new(0_u32);
    let library = use_browser_library("xterm", || true);

    #[cfg(target_arch = "wasm32")]
    let handle: StoredValue<Option<crate::bridges::xterm::TerminalHandle>, LocalStorage> =
        StoredValue::new_local(None);
    // 播放序号：任何新的播放/暂停/清屏/卸载都会使旧的播放序列失效。
    #[cfg(target_arch = "wasm32")]
    let generation = StoredValue::new(0_u64);

    #[cfg(target_arch = "wasm32")]
    {
        use crate::bridges::xterm::{self, TerminalHandle};
        use crate::theme::{use_resolved_theme, ResolvedTheme};
        use wasm_bindgen::closure::Closure;

        let resolved = use_resolved_theme();
        let mount_id = container_id.clone();
        Effect::new(move |_| {
            retry.track();
            if handle.with_value(|handle| handle.is_some()) || !library_ready(library) {
                return;
            }
            let on_ready = Closure::new(|| {});
            let opts = xterm::XtermOptions::new();
            opts.set_theme(if resolved.get_untracked() == ResolvedTheme::Dark {
                "dark"
            } else {
                "light"
            });
            opts.set_font_size(if detail { 13 } else { 11 });
            opts.set_on_ready(&on_ready);
            match xterm::get_module().create(&mount_id, &opts) {
                Ok(Some(instance)) => {
                    instance.write_all(TERMINAL_STDOUT, TERMINAL_STDERR);
                    handle.set_value(Some(TerminalHandle::new(instance, on_ready)));
                    ready.set(true);
                    error.set(None);
                }
                Ok(None) => error.set(Some("终端宿主未就绪".to_string())),
                Err(_) => error.set(Some("终端初始化失败".to_string())),
            }
        });
        Effect::new(move |_| {
            let theme = if resolved.get() == ResolvedTheme::Dark {
                "dark"
            } else {
                "light"
            };
            handle.with_value(|terminal| {
                if let Some(terminal) = terminal {
                    terminal.instance().set_theme(theme);
                }
            });
        });
        on_cleanup(move || {
            let _ = generation.try_update_value(|value| *value = value.wrapping_add(1));
            let _ = handle.try_update_value(Option::take);
        });
    }

    let on_play = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let Some(ticket) = generation.try_update_value(|value| {
                *value = value.wrapping_add(1);
                *value
            }) else {
                return;
            };
            handle.with_value(|terminal| {
                if let Some(terminal) = terminal {
                    terminal.instance().clear();
                }
            });
            status.set("播放中".to_string());
            leptos::task::spawn_local(async move {
                for (stderr, text) in [
                    (false, "\x1b[32m✓\x1b[0m 正在整理文章样例…\n"),
                    (false, "已读取 3 个本地条目\n"),
                    (true, "提示：这只是固定输出示例。\n"),
                    (
                        false,
                        "这是一段较长的输出，用于观察终端在窄屏下如何换行和滚动：Yggdrasil showcase terminal preview output line.\n",
                    ),
                ] {
                    if generation.try_get_value() != Some(ticket) {
                        return;
                    }
                    let _ = handle.try_with_value(|terminal| {
                        if let Some(terminal) = terminal {
                            if stderr {
                                terminal.instance().write_stderr(text);
                            } else {
                                terminal.instance().write_stdout(text);
                            }
                        }
                    });
                    crate::utils::time::sleep_ms(330).await;
                }
                if generation.try_get_value() == Some(ticket) {
                    let _ = status.try_set("播放完成，可重放".to_string());
                }
            });
        }
    };
    let on_pause = move |_| {
        #[cfg(target_arch = "wasm32")]
        generation.update_value(|value| *value = value.wrapping_add(1));
        status.set("已暂停".to_string());
    };
    let on_clear = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            generation.update_value(|value| *value = value.wrapping_add(1));
            handle.with_value(|terminal| {
                if let Some(terminal) = terminal {
                    terminal.instance().clear();
                }
            });
        }
        status.set("已清屏".to_string());
    };

    view! {
        <div class="showcase-browser-toolbar">
            <span>"Xterm · 输出示例"</span>
            {detail
                .then(|| {
                    view! {
                        <div class="showcase-browser-actions">
                            <button
                                type="button"
                                disabled=move || !ready.get()
                                on:click=on_play
                            >
                                "播放 / 重放"
                            </button>
                            <button
                                type="button"
                                disabled=move || !ready.get()
                                on:click=on_pause
                            >
                                "暂停"
                            </button>
                            <button
                                type="button"
                                disabled=move || !ready.get()
                                on:click=on_clear
                            >
                                "清屏"
                            </button>
                        </div>
                    }
                })}
        </div>
        <div class="showcase-browser-host showcase-browser-host--xterm" id=container_id></div>
        {loading_placeholders(library, ready, error, "终端正在初始化…", "终端库加载中…")}
        <LibraryLoadError library/>
        {error_alert(error, retry)}
        {detail
            .then(|| {
                view! {
                    <p class="showcase-browser-note" role="status">
                        {move || format!("{}。固定数据，与编辑的代码无关。", status.get())}
                    </p>
                }
            })}
    }
}

#[component]
fn LightboxPreview(detail: bool) -> impl IntoView {
    let owner = host_id("lightbox-gallery", detail);

    #[cfg(target_arch = "wasm32")]
    {
        let init_owner = owner.clone();
        Effect::new(move |_| {
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
        on_cleanup(move || {
            if let Some(window) = web_sys::window() {
                crate::utils::js::invoke_optional_global(
                    &window,
                    "__closeLightboxFor",
                    &[cleanup_owner.into()],
                );
            }
        });
    }

    view! {
        <div
            class="showcase-browser-lightbox-grid"
            id=owner.clone()
            data-showcase-lightbox-owner=owner
        >
            <img
                src="/images/samples/clay-circle.webp"
                alt="暖白纸面上的一轮陶土色圆"
                loading="lazy"
            />
            <img
                src="/images/samples/hairline-grid.webp"
                alt="细线网格上连起的几个点"
                loading="lazy"
            />
            <img
                src="/images/samples/ink-dusk.webp"
                alt="深色背景上层层展开的陶土色弧线"
                loading="lazy"
            />
        </div>
        {detail
            .then(|| {
                view! {
                    <p class="showcase-browser-note">
                        "点击图片打开灯箱，可切换、缩放、旋转，并用 Esc 关闭。"
                    </p>
                }
            })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptos::reactive::owner::Owner;

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

    #[test]
    fn unknown_slug_has_no_preview() {
        Owner::new().with(|| assert!(preview("alert-box", false).is_none()));
    }

    #[test]
    fn lightbox_renders_owner_markers_on_server() {
        Owner::new().with(|| {
            let html = view! { <LightboxPreview detail=true/> }.to_html();
            assert!(
                html.contains(
                    r#"data-showcase-lightbox-owner="showcase-browser-lightbox-gallery-detail""#
                ),
                "{html}"
            );
            assert!(html.contains("点击图片打开灯箱"), "{html}");
        });
    }
}
