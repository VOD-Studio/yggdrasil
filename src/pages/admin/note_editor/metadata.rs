use super::{history::HistoryPanel, save::persist, state::EditorState};
use crate::api::notes::owned_notebooks;
use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, ConfirmDialog, Icon, IconButton, IconName, StatusDot,
};
use crate::models::note::*;
use crate::pages::admin::notes::{client_request, parse_tags};
use crate::router::Route;
use dioxus::prelude::*;

/// 落后几个版本的说明；`None` 表示与工作稿一致。
fn behind(work: i32, snapshot: i32) -> Option<i32> {
    (work > snapshot).then_some(work - snapshot)
}

/// 设置抽屉：版本面板、笔记本、标签、历史版本与移入回收站。
#[component]
pub(super) fn MetadataPanel(mut state: EditorState, open: Signal<bool>, locked: bool) -> Element {
    let books = use_resource(|| client_request(owned_notebooks()));
    let mut tags = state.tags;
    let mut confirm_trash = use_signal(|| false);
    let navigator = navigator();
    let saved = state.saved();
    let draft = state.draft();
    let dirty = state.dirty();
    let work = saved.as_ref().map(|n| n.version);
    let published = saved.as_ref().and_then(|n| n.published_version);
    let public_tone = if published.is_some() { "clay" } else { "muted" };
    let knowledge = saved.as_ref().and_then(|n| n.knowledge_version);
    // 有未保存修改时，操作会先保存出新版本，所以一致也允许点。
    let publish_same = !dirty && work.is_some() && published == work;
    let knowledge_same = !dirty && work.is_some() && knowledge == work;
    let act = move |action: NoteAction| async move {
        persist(state, Some(action)).await;
    };

    rsx! {
        aside { class: "write-drawer note-drawer", aria_label: "笔记设置",
            div { class: "write-drawer-head",
                h2 { "设置" }
                IconButton {
                    icon: IconName::X,
                    label: "关闭设置",
                    size: ButtonSize::Sm,
                    onclick: move |_| open.set(false),
                }
            }

            section { class: "note-side-section", aria_labelledby: "note-versions-h",
                h2 { id: "note-versions-h", class: "note-side-title", "版本" }
                ol { class: "note-versions",
                    li {
                        div { class: "note-version-head",
                            span { "工作稿" }
                            if let Some(v) = work {
                                span { class: "note-version-num", "v{v}" }
                            }
                        }
                        p { "只有你能看到。停止输入约 1.2 秒后自动保存。" }
                    }
                    li { "data-kind": "public",
                        div { class: "note-version-head",
                            StatusDot { label: "公开版", tone: public_tone }
                            if let Some(v) = published {
                                span { class: "note-version-num", "v{v}" }
                            }
                        }
                        match (published, work) {
                            (None, _) => rsx! {
                                p { "仅自己可见。发布后访客可以在笔记页读到这一版。" }
                            },
                            (Some(p), Some(w)) if behind(w, p).is_some() => rsx! {
                                p { class: "note-version-ahead",
                                    "工作稿比公开版新 {w - p} 个版本，修改不会自动发布。"
                                }
                            },
                            _ => rsx! {
                                p { "与工作稿一致。" }
                            },
                        }
                        div { class: "note-version-actions",
                            button {
                                class: button_class(ButtonVariant::Primary, ButtonSize::Sm),
                                r#type: "button",
                                disabled: locked || publish_same,
                                onclick: move |_| act(NoteAction::Publish),
                                if published.is_some() { "更新公开版" } else { "发布到前台" }
                            }
                            if published.is_some() {
                                button {
                                    class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                                    r#type: "button",
                                    disabled: locked,
                                    onclick: move |_| act(NoteAction::Unpublish),
                                    "撤回"
                                }
                            }
                        }
                    }
                    li { "data-kind": "knowledge",
                        div { class: "note-version-head",
                            span { "AI 知识库" }
                            if let Some(v) = knowledge {
                                span { class: "note-version-num", "v{v}" }
                            } else {
                                span { class: "note-version-num", "未收录" }
                            }
                        }
                        p { "收录后，获得授权的 AI 可以读取这一版内容，无需公开发布。" }
                        if let (Some(k), Some(w)) = (knowledge, work) {
                            if let Some(n) = behind(w, k) {
                                p { class: "note-version-ahead", "已收录的是 {n} 个版本之前的内容。" }
                            }
                        }
                        div { class: "note-version-actions",
                            button {
                                class: button_class(ButtonVariant::Secondary, ButtonSize::Sm),
                                r#type: "button",
                                disabled: locked || knowledge_same,
                                onclick: move |_| act(NoteAction::IncludeKnowledge),
                                "收录当前版本"
                            }
                            if knowledge.is_some() {
                                button {
                                    class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                                    r#type: "button",
                                    disabled: locked,
                                    onclick: move |_| act(NoteAction::ExcludeKnowledge),
                                    "移出"
                                }
                            }
                        }
                    }
                }
            }

            section { class: "note-side-section", aria_labelledby: "note-books-h",
                h2 { id: "note-books-h", class: "note-side-title", "笔记本" }
                match books.read().as_ref() {
                    Some(Ok(items)) if items.is_empty() => rsx! {
                        p { class: "note-side-hint",
                            "还没有笔记本。"
                            Link { to: Route::AdminNotebooks {}, "新建一个" }
                        }
                    },
                    Some(Ok(items)) => rsx! {
                        div { class: "note-books", role: "group", aria_label: "收录到笔记本",
                            for book in items.iter().filter(|b| b.archived_at.is_none() || draft.notebook_ids.contains(&b.id)).cloned() {
                                button {
                                    key: "{book.id}",
                                    class: "note-book",
                                    r#type: "button",
                                    aria_pressed: "{draft.notebook_ids.contains(&book.id)}",
                                    disabled: locked,
                                    onclick: move |_| state.draft.with_mut(|d| {
                                        if let Some(at) = d.notebook_ids.iter().position(|id| *id == book.id) {
                                            d.notebook_ids.remove(at);
                                        } else {
                                            d.notebook_ids.push(book.id);
                                            d.notebook_ids.sort_unstable();
                                        }
                                    }),
                                    if draft.notebook_ids.contains(&book.id) {
                                        Icon { name: IconName::Check, size: 12 }
                                    }
                                    "{book.title}"
                                    if book.archived_at.is_some() {
                                        span { class: "note-book-archived", "已归档" }
                                    }
                                }
                            }
                        }
                    },
                    Some(Err(_)) => rsx! {
                        p { class: "note-side-hint", role: "alert", "笔记本加载失败。" }
                    },
                    None => rsx! {
                        p { class: "note-side-hint", role: "status", "正在读取笔记本…" }
                    },
                }
            }

            section { class: "note-side-section",
                label { class: "note-side-title", r#for: "note-tags", "标签" }
                input {
                    id: "note-tags",
                    class: "field",
                    r#type: "text",
                    placeholder: "Rust, 排错, 读书",
                    disabled: locked,
                    value: "{tags}",
                    oninput: move |ev| {
                        let value = ev.value();
                        state.draft.with_mut(|d| d.tags = parse_tags(&value));
                        tags.set(value);
                    },
                }
            }

            HistoryPanel { state, locked }

            if saved.as_ref().is_some_and(|n| n.deleted_at.is_none()) {
                div { class: "write-drawer-foot note-drawer-foot",
                    div {
                        button {
                            class: "write-trash",
                            r#type: "button",
                            disabled: locked,
                            onclick: move |_| confirm_trash.set(true),
                            "移入回收站"
                        }
                        p { "会撤回公开版并移出知识库，之后可以恢复。" }
                    }
                }
                ConfirmDialog {
                    visible: confirm_trash,
                    title: "把这条笔记移入回收站？",
                    description: "公开版会撤回，AI 知识库也不再收录。之后可以在回收站恢复为私密草稿。",
                    confirm_label: "移入回收站",
                    on_confirm: move |_| {
                        spawn(async move {
                            if persist(state, Some(NoteAction::Trash)).await {
                                navigator.push(Route::AdminNotes {});
                            }
                        });
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::behind;

    #[test]
    fn behind_counts_versions_only_when_snapshot_is_older() {
        assert_eq!(behind(12, 9), Some(3));
        assert_eq!(behind(9, 9), None);
    }
}
