use super::{save::restore_revision, state::EditorState};
use crate::api::notes::note_history;
use crate::models::note::NoteRevision;
use crate::utils::error::failure_message;
use crate::utils::time::{hm, md, to_local};
use leptos::prelude::*;
use leptos::task::spawn_local;

/// 历史版本：每次保存成新版本后重新读取。恢复会把那一版写成新的工作稿。
///
/// 位于路由的 `<Suspense>` 内且会重新读取，因此不用 `Resource`：Effect + 世代号写镜像信号，
/// 刷新期间保留上一份列表。错误已转成中文提示文本。
#[component]
pub(super) fn HistoryPanel(
    state: EditorState,
    #[prop(into)] locked: Signal<bool>,
) -> impl IntoView {
    // 只有换了笔记或版本号才重新读取；发布、收录不产生新版本。
    let key = Memo::new(move |_| state.saved.with(|n| n.as_ref().map(|n| (n.id, n.version))));
    let current = Memo::new(move |_| key.get().map(|(_, version)| version));
    let revisions = RwSignal::new(None::<Result<Option<Vec<NoteRevision>>, String>>);
    let generation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        let key = key.get();
        let Some(token) = generation.try_update_value(|g| {
            *g += 1;
            *g
        }) else {
            return;
        };
        let Some((id, _)) = key else {
            revisions.set(Some(Ok(None)));
            return;
        };
        spawn_local(async move {
            let result = note_history(id)
                .await
                .map(Some)
                .map_err(|e| failure_message("历史版本加载失败", &e));
            if generation.try_get_value() == Some(token) {
                let _ = revisions.try_set(Some(result));
            }
        });
    });

    view! {
        <section class="note-side-section" aria-labelledby="note-history-h">
            <div class="note-side-head">
                <h2 id="note-history-h">"历史版本"</h2>
                <span>"恢复会生成新草稿"</span>
            </div>
            {move || match revisions.get() {
                Some(Ok(None)) => {
                    view! { <p class="note-side-hint">"保存后开始记录版本。"</p> }.into_any()
                }
                Some(Err(message)) => {
                    view! {
                        <p class="note-side-hint" role="alert">
                            {message}
                        </p>
                    }
                        .into_any()
                }
                None => {
                    view! {
                        <p class="note-side-hint" role="status">
                            "正在读取历史版本…"
                        </p>
                    }
                        .into_any()
                }
                Some(Ok(Some(items))) => {
                    view! {
                        <ol class="note-history">
                            {items
                                .into_iter()
                                .map(|revision| {
                                    let version = revision.version;
                                    let t = to_local(revision.created_at);
                                    let when = format!("{} {}", md(&t, '-'), hm(&t));
                                    let title = (!revision.title.is_empty())
                                        .then(|| format!(" · {}", revision.title));
                                    view! {
                                        <li>
                                            <span class="note-history-v">{format!("v{version}")}</span>
                                            <span class="note-history-when">{when} {title}</span>
                                            {move || {
                                                if current.get() == Some(version) {
                                                    view! { <span class="note-history-current">"当前"</span> }
                                                        .into_any()
                                                } else {
                                                    view! {
                                                        <button
                                                            type="button"
                                                            aria-label=format!("恢复 v{version}")
                                                            disabled=move || locked.get()
                                                            on:click=move |_| {
                                                                spawn_local(async move {
                                                                    restore_revision(state, version).await;
                                                                });
                                                            }
                                                        >
                                                            "恢复"
                                                        </button>
                                                    }
                                                        .into_any()
                                                }
                                            }}
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ol>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}
