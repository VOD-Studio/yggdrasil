//! 文章编辑器页面。
//!
//! 提供新建文章与编辑文章两种模式，使用基于 Tiptap 的富文本编辑器。
//! 编辑器通过 [`crate::bridges::tiptap`] 的 wasm-bindgen 绑定在 WASM 前端初始化，
//! 并与 `window.TiptapEditor` 实例交互，实现 Markdown 内容回填、图片上传与组件卸载时的清理。

#![allow(unused_imports)]

// prelude 在 WASM 构建里直接使用（use_signal/Signal/Element 等）；
// server 构建里 #[component] 宏会重新导出这些符号导致 native 报 unused，故 allow。
use dioxus::prelude::*;

mod cover;
mod editor;
mod metadata;
mod save;
mod state;

use crate::bridges::library::{library_ready, LibraryLoadError};
use crate::components::assets::{AssetPickerModal, AssetSelection};
use crate::components::forms::FormSelect;
use crate::components::skeletons::write_skeleton::WriteSkeleton;
use crate::components::ui::{LoadingButton, BTN_CLOSE_ICON};
use crate::router::Route;
use dioxus::router::components::Link;
use editor::use_write_editor;
use metadata::MetadataPanel;

/// 新建文章页面组件。
///
/// 内部委托给 `write_editor`，以 `None` 表示新建模式。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn Write() -> Element {
    write_editor(None)
}

/// 编辑文章页面组件。
///
/// `id` 为要编辑的文章 ID，内部委托给 `write_editor` 加载现有数据。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn WriteEdit(id: i32) -> Element {
    write_editor(Some(id))
}

/// 组合编辑器、元信息面板和保存状态，支持新建与编辑模式。
/// 浏览器生命周期由 `editor` hook 管理，保存请求由 `save` 捕获表单快照。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn write_editor(post_id: Option<i32>) -> Element {
    let state = use_write_editor(post_id);
    let mut form = state.form;
    let editor_library = state.library;
    let loading = use_memo(move || state.phase.read().is_loading());
    let saving = use_memo(move || state.save.read().is_saving());
    let load_error = use_memo(move || state.phase.read().error().map(str::to_owned));
    let error = use_memo(move || state.save.read().error().map(str::to_owned));
    let upload_errors = state.upload_errors;
    let cover_uploading = state.cover_uploading;
    let body_picker_visible = state.body_picker_visible;
    let body_picker_uploading = state.body_picker_uploading;
    #[cfg(target_arch = "wasm32")]
    let editor = state.editor;
    let mut sidebar_open = use_signal(|| true);
    // Apply the viewport default after hydration so SSR and the first browser render
    // agree, while narrow screens keep the writing area usable on initial entry.
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let wide = web_sys::window()
            .and_then(|window| window.inner_width().ok())
            .and_then(|width| width.as_f64())
            .is_some_and(|width| width >= 1024.0);
        sidebar_open.set(wide);
    });
    let word_stats = use_memo(move || {
        let chars = form
            .read()
            .content
            .chars()
            .filter(|c| !c.is_whitespace())
            .count();
        (chars, (chars / 300).max(1))
    });
    let on_submit = move |_| save::submit(state, post_id);
    let is_edit = post_id.is_some();
    let mode_title = if is_edit {
        if let Some(id) = post_id {
            format!("编辑文章 #{}", id)
        } else {
            "编辑文章".to_string()
        }
    } else {
        "新建文章".to_string()
    };

    rsx! {
        // 根容器：flex 分区布局，包含顶部精致导航条 + 主体两栏写作区 + 底部操作栏。
        div { class: "animate-page-enter relative flex flex-col w-full min-h-0 flex-1 bg-[var(--color-paper-theme)]",
            if loading() && !matches!(&*editor_library.read(), Some(Err(_))) {
                div { class: "absolute inset-0 z-10 bg-[var(--color-paper-theme)] flex flex-col", WriteSkeleton {} }
            }

            // 顶部精致工具条：面包屑 / 写作模式 / 实时字数与阅读时长 / 侧栏沉浸式切换
            div { class: "flex-shrink-0 px-6 py-3 border-b border-[var(--color-paper-border)]/60 bg-[var(--color-paper-theme)] flex items-center justify-between gap-4 z-10 select-none",
                // 左侧：返回文章列表 + 模式指示 + 状态胶囊
                div { class: "flex items-center gap-3",
                    Link {
                        to: Route::Posts {},
                        class: "inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] hover:bg-[var(--color-paper-entry)] transition-all cursor-pointer",
                        svg {
                            class: "w-4 h-4",
                            xmlns: "http://www.w3.org/2000/svg",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M19 12H5M12 19l-7-7 7-7" }
                        }
                        "文章列表"
                    }
                    div { class: "w-px h-4 bg-[var(--color-paper-border)]/60" }
                    div { class: "flex items-center gap-2",
                        span { class: "text-sm font-semibold text-[var(--color-paper-primary)]",
                            "{mode_title}"
                        }
                        // 状态胶囊
                        if form.read().status.clone() == "published" {
                            span { class: "inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20",
                                "公开"
                            }
                        } else {
                            span { class: "inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20",
                                "草稿"
                            }
                        }
                    }
                }

                // 中间：实时字数与预计阅读时间
                div { class: "hidden md:flex items-center gap-3 text-xs text-[var(--color-paper-tertiary)] font-medium",
                    div { class: "flex items-center gap-1.5",
                        span { class: "w-1.5 h-1.5 rounded-full bg-[var(--color-paper-accent)]" }
                        span { "{word_stats().0} 字" }
                    }
                    span { "·" }
                    span { "预计阅读 {word_stats().1} 分钟" }
                }

                // 右侧：侧栏 / 专注写作模式切换按钮
                div { class: "flex items-center gap-2",
                    button {
                        class: "inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium border transition-all cursor-pointer",
                        class: if sidebar_open() {
                            "border-[var(--color-paper-border)] text-[var(--color-paper-primary)] bg-[var(--color-paper-entry)] hover:bg-[var(--color-paper-border)]/50"
                        } else {
                            "border-[var(--color-paper-accent)]/50 text-[var(--color-paper-accent)] bg-[var(--color-paper-accent)]/10 hover:bg-[var(--color-paper-accent)]/20"
                        },
                        onclick: move |_| sidebar_open.toggle(),
                        title: if sidebar_open() { "进入专注写作模式（收起设置）" } else { "展开文章属性面板" },
                        svg {
                            class: "w-4 h-4",
                            xmlns: "http://www.w3.org/2000/svg",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            rect { x: "3", y: "3", width: "18", height: "18", rx: "2", ry: "2" }
                            line { x1: "15", y1: "3", x2: "15", y2: "21" }
                        }
                        if sidebar_open() {
                            "收起设置"
                        } else {
                            "文章设置"
                        }
                    }
                }
            }

            // 两栏容器：flex-1 分配空间，自身不滚动 (min-h-0)，滚动职责下放给左右两栏。
            div { class: "flex-1 min-h-0 flex",
                // 左栏 (主写作区)：类 Notion/Ghost 极简文档流，居中优雅排版。
                div { "data-vt-scroll": "admin-write-content", class: "flex-1 min-w-0 min-h-0 overflow-y-auto px-6 sm:px-10 md:px-14 py-8 flex flex-col items-center",
                    div { class: "w-full max-w-4xl flex-1 flex flex-col",
                        // 标题输入
                        input {
                            class: "w-full text-3xl sm:text-4xl md:text-5xl font-extrabold bg-transparent text-[var(--color-paper-primary)] placeholder-[var(--color-paper-tertiary)]/60 focus:outline-none tracking-tight leading-tight transition-colors py-2 mb-4",
                            placeholder: "输入文章标题...",
                            value: form.read().title.clone(),
                            oninput: move |evt| form.write().title = evt.value(),
                        }

                        // 错误和成功提示
                        LibraryLoadError { library: editor_library }
                        if let Some(err) = load_error() {
                            div { class: "flex-shrink-0 px-4 py-2 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-2xl text-sm border border-red-100 dark:border-red-900/30 mb-3",
                                "{err}"
                                button { class: "ml-3 underline", onclick: move |_| state.retry(), "重试" }
                            }
                        }

                        if let Some(err) = error() {
                            div { class: "flex-shrink-0 px-4 py-2 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-2xl text-sm border border-red-100 dark:border-red-900/30 mb-3",
                                "{err}"
                            }
                        }

                        // 上传失败提示：多条堆叠，×关闭同时删除编辑器内失败占位符（避免孤儿）
                        for err in upload_errors().clone() {
                            div {
                                key: "{err.id}",
                                class: "flex-shrink-0 flex items-center justify-between gap-3 px-4 py-2 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-2xl text-sm border border-red-100 dark:border-red-900/30 mb-3",
                                span { "图片上传失败: {err.file_name} — {err.message}" }
                                button {
                                    class: "{BTN_CLOSE_ICON}",
                                    aria_label: "关闭提示",
                                    onclick: {
                                        let id = err.id.clone();
                                        let mut upload_errors = upload_errors;
                                        move |_| {
                                            #[cfg(target_arch = "wasm32")]
                                            if let Some(handle) = &*editor.read() {
                                                handle.instance().remove_upload_by_upload_id(&id);
                                            }
                                            upload_errors.write().retain(|e| e.id != id);
                                        }
                                    },
                                    "×"
                                }
                            }
                        }

                        // 编辑器区域：软卡片容器，随窗口自适应扩展。
                        div { class: "flex-1 min-h-[480px] flex flex-col mb-4",
                            div {
                                class: "relative group flex-1 min-h-0 w-full rounded-2xl overflow-hidden bg-[var(--color-paper-entry)]/40 border border-[var(--color-paper-border)]/70 focus-within:border-[var(--color-paper-accent)]/70 focus-within:ring-2 focus-within:ring-[var(--color-paper-accent)]/15 transition-all shadow-xs",
                                id: "tiptap-editor",
                            }
                        }
                    }
                } // 左栏闭合

                // 右栏 (侧边栏)：折叠/展开动画，分节卡片化，带图标与视觉预览
                if sidebar_open() {
                    MetadataPanel { form, sidebar_open, cover_uploading }
                } // 右栏闭合
            } // 两栏容器闭合

            // 底部操作栏 - flex 分区布局的贴底块：固定在底部，提供发布、草稿状态切换与返回
            div { class: "flex-shrink-0 px-6 py-3.5 flex items-center justify-between border-t border-[var(--color-paper-border)]/80 bg-[var(--color-paper-theme)] shadow-xs select-none",
                // 左侧取消返回
                Link {
                    to: Route::Posts {},
                    class: "inline-flex items-center gap-1.5 px-4 py-2 rounded-full text-sm font-medium text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                    "返回列表"
                }

                // 右侧操作群组
                div { class: "flex items-center gap-3",
                    // 文章状态下拉（胶囊触发器）
                    FormSelect {
                        trigger_class: Some(
                            "inline-flex w-auto cursor-pointer select-none text-left text-sm font-medium pl-4 pr-8 py-2 rounded-full border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] hover:bg-[var(--color-paper-theme)] focus:outline-none focus:border-paper-accent focus:ring-1 focus:ring-paper-accent/30 transition-colors duration-200",
                        ),
                        value: form.read().status.clone(),
                        options: vec![
                            ("draft".to_string(), "存为草稿"),
                            ("published".to_string(), "直接发布"),
                        ],
                        onchange: move |v| form.write().status = v,
                    }
                    div { class: "w-px h-5 bg-[var(--color-paper-border)]/60" }
                    LoadingButton {
                        label: if is_edit { "更新文章".to_string() } else { "发布文章".to_string() },
                        loading: saving(),
                        disabled: !state.phase.read().is_ready() || !library_ready(editor_library),
                        onclick: move |_| on_submit(()),
                    }
                }
            } // 底部操作栏闭合

            // 正文素材选择 modal（多选模式）
            AssetPickerModal {
                visible: body_picker_visible,
                cover_uploading: body_picker_uploading,
                title: "选择图片",
                multi: true,
                on_select: move |picks: Vec<AssetSelection>| {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(handle) = &*editor.read() {
                        if let Ok(json) = serde_json::to_string(&picks) {
                            handle.instance().insert_images_from_library(&json);
                        }
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = picks;
                },
            }
        }
    }
}
