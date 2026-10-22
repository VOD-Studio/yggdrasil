//! 写笔记与编辑笔记（`/admin/notes/new`、`/admin/notes/edit/:id`）。
//!
//! 与写文章同一套无干扰版式：页头（返回、保存状态、查看公开页面、预览、设置、保存）
//! 替代后台顶栏；类型分段、衬线标题与 Tiptap 正文居中；右侧设置抽屉是版本面板
//! （工作稿、公开版、AI 知识库）、笔记本、标签与历史版本。工作稿停止输入后自动保存，
//! 公开与知识库快照只由明确操作更新。样式沿用 styles/pages/admin-write.css，
//! 笔记专属部分在 admin-notes.css。
mod composer;
mod history;
mod metadata;
mod save;
mod state;

use super::note_drafts::use_draft_protection;
use super::notes::client_request;
use crate::api::notes::get_owned_note;
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::skeletons::write_skeleton::WriteSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconName, LoadError,
    SegmentedControl, SegmentedItem,
};
use crate::models::note::*;
use crate::router::Route;
use composer::NoteComposer;
use dioxus::prelude::*;
use metadata::MetadataPanel;
use save::persist;
use state::SaveStatus;

#[component]
pub fn NewNote() -> Element {
    rsx! { NoteEditor { initial: None } }
}

#[component]
pub fn EditNote(id: i32) -> Element {
    rsx! {
        for key in std::iter::once(id) {
            LoadNoteEditor { key: "{key}", id: key }
        }
    }
}

#[component]
fn LoadNoteEditor(id: i32) -> Element {
    let navigator = use_navigator();
    let mut response = use_resource(move || client_request(get_owned_note(id, None)));
    let data = response.read();
    match data.as_ref() {
        Some(Ok(note)) => rsx! {
            NoteEditor { initial: Some(note.clone()) }
        },
        Some(Err(e)) if e.to_string().contains("不存在") => rsx! {
            div { class: "write-missing",
                EmptyState {
                    icon: IconName::FileX,
                    title: "找不到这条笔记",
                    description: "它可能已被彻底删除。",
                    action: EmptyStateAction {
                        label: "返回笔记".to_string(),
                        onclick: Callback::new(move |_| {
                            navigator.push(Route::AdminNotes {});
                        }),
                    },
                }
            }
        },
        Some(Err(_)) => rsx! {
            div { class: "write-missing",
                LoadError { title: "笔记加载失败", on_retry: move |_| response.restart() }
            }
        },
        None => rsx! {
            WriteSkeleton {}
        },
    }
}

#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
fn NoteEditor(initial: Option<Note>) -> Element {
    let toast = use_toast();
    let mut state = state::use_editor_state(initial, toast);
    let editor_epoch = state.editor_epoch;
    let slot = state
        .draft()
        .id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "new".into());
    let (mut recovery, cache_failed) = use_draft_protection(slot, state.draft, move || {
        state.dirty() || state.busy() || (state.uploading)()
    });
    let mut preview = use_signal(|| false);
    let mut drawer_open = use_signal(|| true);
    // 首屏与 SSR 一致，水合后窄屏默认收起设置抽屉。
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let wide = web_sys::window()
            .and_then(|w| w.inner_width().ok())
            .and_then(|w| w.as_f64())
            .is_some_and(|w| w >= 1024.0);
        drawer_open.set(wide);
    });
    save::use_autosave(state, recovery);

    let toggle_preview = move |_| async move {
        if preview() || persist(state, None).await {
            preview.toggle();
        }
    };
    let save_now = move || {
        spawn(async move {
            persist(state, None).await;
        });
    };
    let error = state.error();
    let saved = state.saved();
    let draft = state.draft();
    let deleted = saved.as_ref().is_some_and(|n| n.deleted_at.is_some());
    let recovering = recovery().is_some();
    let status = SaveStatus::of(
        state.busy(),
        !error.is_empty(),
        state.dirty(),
        recovering,
        saved.as_ref().map(|n| n.version),
    );
    let public_url = saved
        .as_ref()
        .filter(|n| n.published_version.is_some())
        .map(|n| format!("/notes/{}", n.slug));
    let locked = state.busy() || deleted || recovering;

    rsx! {
        div {
            class: "write-page note-editor",
            onkeydown: move |e| {
                let modifiers = e.modifiers();
                if (modifiers.meta() || modifiers.ctrl()) && e.key() == Key::Character("s".into()) {
                    e.prevent_default();
                    if !locked {
                        save_now();
                    }
                }
            },
            header { class: "write-head",
                div { class: "write-head-left",
                    Link {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                        to: Route::AdminNotes {},
                        Icon { name: IconName::ArrowLeft, size: 16 }
                        span { class: "write-hide-narrow", "笔记管理" }
                        span { class: "sr-only", "返回" }
                    }
                }
                div { class: "write-head-actions",
                    if status == SaveStatus::Failed {
                        button {
                            class: "write-status",
                            r#type: "button",
                            "data-tone": status.tone(),
                            "data-state": status.key(),
                            title: "{error}",
                            onclick: move |_| save_now(),
                            span { class: "write-status-dot", aria_hidden: "true" }
                            span { role: "status", "{status.label()}" }
                        }
                    } else {
                        span {
                            class: "write-status",
                            "data-tone": status.tone(),
                            "data-state": status.key(),
                            "data-pulse": (status == SaveStatus::Saving).then_some("true"),
                            span { class: "write-status-dot", aria_hidden: "true" }
                            span { role: "status", aria_live: "polite", "{status.label()}" }
                        }
                    }
                    if let Some(url) = public_url {
                        a {
                            class: "{button_class(ButtonVariant::Ghost, ButtonSize::Md)} write-hide-narrow",
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener",
                            "查看公开页面"
                            Icon { name: IconName::ArrowUpRight, size: 14 }
                        }
                    }
                    button {
                        class: "{button_class(ButtonVariant::Ghost, ButtonSize::Md)} write-hide-narrow",
                        r#type: "button",
                        disabled: state.busy() || saved.is_none() || recovering,
                        aria_pressed: "{preview()}",
                        onclick: toggle_preview,
                        if preview() { "返回编辑" } else { "预览" }
                    }
                    button {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                        r#type: "button",
                        aria_pressed: "{drawer_open()}",
                        aria_controls: "note-drawer",
                        // 窄屏只剩图标，名称由 aria-label 补出。
                        aria_label: "设置",
                        onclick: move |_| drawer_open.toggle(),
                        Icon { name: IconName::PanelRight, size: 15 }
                        span { class: "write-hide-narrow", "设置" }
                    }
                    button {
                        class: button_class(ButtonVariant::Secondary, ButtonSize::Md),
                        r#type: "button",
                        disabled: locked,
                        title: "⌘S",
                        onclick: move |_| save_now(),
                        "保存草稿"
                    }
                }
            }

            if let Some(local) = recovery() {
                div { class: "write-banner", role: "alert",
                    Icon { name: IconName::RotateCcwClock, size: 18 }
                    div {
                        p { class: "write-banner-title", "这个标签页留有未保存的内容" }
                        p {
                            "恢复只修改工作稿，不会发布；若服务端已有新版本，请先核对内容。选择之前编辑器暂停输入。"
                        }
                        div { class: "write-banner-actions",
                            button {
                                class: button_class(ButtonVariant::Primary, ButtonSize::Sm),
                                r#type: "button",
                                onclick: move |_| {
                                    state.restore_local(local.clone());
                                    recovery.set(None);
                                },
                                "恢复未保存内容"
                            }
                            button {
                                class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                                r#type: "button",
                                onclick: move |_| recovery.set(None),
                                "使用服务端版本"
                            }
                        }
                    }
                }
            }

            div { class: "write-body",
                main { class: "write-main", "data-vt-scroll": "admin-note-content",
                    div { class: "write-doc",
                        if !error.is_empty() {
                            p { class: "write-notice", role: "alert", span { "{error}" } }
                        }
                        if cache_failed() {
                            p { class: "write-notice", role: "alert",
                                "浏览器无法保存本地恢复副本，请先保存到服务端再离开。"
                            }
                        }
                        if deleted {
                            div { class: "note-trashed", role: "status",
                                Icon { name: IconName::Trash, size: 16 }
                                p { "这条笔记在回收站中。恢复后可继续编辑，公开与知识库收录需要重新确认。" }
                                button {
                                    class: button_class(ButtonVariant::Primary, ButtonSize::Sm),
                                    r#type: "button",
                                    disabled: state.busy(),
                                    onclick: move |_| async move {
                                        persist(state, Some(NoteAction::Restore)).await;
                                    },
                                    "恢复笔记"
                                }
                            }
                        }
                        if preview() {
                            // 预览复用公开渲染路径：完整 Note 转为 PublicNote（编辑器的工作稿
                            // 版本号保留展示），显示字段不变，Markdown 不会进入公开渲染组件。
                            if let Some(note) = saved.clone() {
                                crate::pages::notes::NoteDocument {
                                    key: "{note.version}",
                                    note: crate::models::public_note::PublicNote::from(note),
                                    preview: true,
                                }
                            }
                        } else {
                            SegmentedControl {
                                class: "note-kind-tabs",
                                aria_label: "笔记类型",
                                items: vec![
                                    SegmentedItem::new("moment".to_string(), "随记"),
                                    SegmentedItem::new("topic".to_string(), "主题笔记"),
                                ],
                                value: draft.kind.as_str().to_string(),
                                on_change: move |value: String| {
                                    if !locked {
                                        state.draft.with_mut(|d| {
                                            d.kind = if value == "topic" { NoteKind::Topic } else { NoteKind::Moment };
                                        });
                                    }
                                },
                            }
                            label { class: "sr-only", r#for: "note-title", "笔记标题" }
                            textarea {
                                id: "note-title",
                                class: "write-title note-title",
                                rows: "1",
                                maxlength: 200,
                                readonly: recovering || deleted,
                                placeholder: if draft.kind == NoteKind::Moment { "标题可选，先记下来…" } else { "给这篇笔记一个标题" },
                                value: "{draft.title}",
                                oninput: move |ev| state.draft.with_mut(|d| d.title = ev.value().replace(['\n', '\r'], "")),
                                onkeydown: move |e| {
                                    if e.key() == Key::Enter && !e.data().is_composing() {
                                        e.prevent_default();
                                        composer::focus_editor();
                                    }
                                },
                            }
                            div {
                                class: "write-editor-wrap",
                                "inert": (recovering || deleted).then_some("true"),
                                for epoch in std::iter::once(editor_epoch()) {
                                    NoteComposer {
                                        key: "{epoch}",
                                        initial: draft.content_md.clone(),
                                        on_change: move |md| state.draft.with_mut(|d| d.content_md = md),
                                        uploading: state.uploading,
                                    }
                                }
                            }
                        }
                    }
                }
                if drawer_open() {
                    div { id: "note-drawer", class: "write-drawer-slot",
                        MetadataPanel { state, open: drawer_open, locked }
                    }
                }
            }
        }
    }
}
