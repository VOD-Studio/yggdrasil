use super::{save::restore_revision, state::EditorState};
use crate::api::notes::note_history;
use crate::pages::admin::notes::client_request;
use crate::utils::error::failure_message;
use crate::utils::time::to_local;
use dioxus::prelude::*;

/// 历史版本：每次保存成新版本后重新读取。恢复会把那一版写成新的工作稿。
#[component]
pub(super) fn HistoryPanel(state: EditorState, locked: bool) -> Element {
    // 只有换了笔记或版本号才重新读取；发布、收录不产生新版本。
    let key = use_memo(move || state.saved().map(|n| (n.id, n.version)));
    let current = key().map(|(_, version)| version);
    let revisions = use_resource(move || {
        let key = key();
        async move {
            match key {
                Some((id, _)) => client_request(note_history(id)).await.map(Some),
                None => Ok(None),
            }
        }
    });
    rsx! {
        section { class: "note-side-section", aria_labelledby: "note-history-h",
            div { class: "note-side-head",
                h2 { id: "note-history-h", "历史版本" }
                span { "恢复会生成新草稿" }
            }
            match revisions.read().as_ref() {
                Some(Ok(None)) => rsx! {
                    p { class: "note-side-hint", "保存后开始记录版本。" }
                },
                Some(Err(e)) => rsx! {
                    p { class: "note-side-hint", role: "alert", {failure_message("历史版本加载失败", e)} }
                },
                None => rsx! {
                    p { class: "note-side-hint", role: "status", "正在读取历史版本…" }
                },
                Some(Ok(Some(items))) => rsx! {
                    ol { class: "note-history",
                        for revision in items.iter().cloned() {
                            li { key: "{revision.version}",
                                span { class: "note-history-v", "v{revision.version}" }
                                span { class: "note-history-when",
                                    {to_local(revision.created_at).format("%m-%d %H:%M").to_string()}
                                    if !revision.title.is_empty() {
                                        " · {revision.title}"
                                    }
                                }
                                if Some(revision.version) == current {
                                    span { class: "note-history-current", "当前" }
                                } else {
                                    button {
                                        r#type: "button",
                                        aria_label: "恢复 v{revision.version}",
                                        disabled: locked,
                                        onclick: move |_| async move {
                                            restore_revision(state, revision.version).await;
                                        },
                                        "恢复"
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}
