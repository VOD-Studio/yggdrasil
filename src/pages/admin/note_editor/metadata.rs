use super::{history::HistoryPanel, save::persist, state::EditorState};
use crate::api::notes::owned_notebooks;
use crate::components::forms::FormInput;
use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, ConfirmDialog, Icon, IconButton, IconName, StatusDot,
};
use crate::models::note::*;
use crate::pages::admin::notes::parse_tags;
use crate::routes::Route;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// 落后几个版本的说明；`None` 表示与工作稿一致。
fn behind(work: i32, snapshot: i32) -> Option<i32> {
    (work > snapshot).then_some(work - snapshot)
}

/// 设置抽屉：版本面板、笔记本、标签、历史版本与移入回收站。
#[component]
pub(super) fn MetadataPanel(
    state: EditorState,
    open: RwSignal<bool>,
    #[prop(into)] locked: Signal<bool>,
) -> impl IntoView {
    // 位于路由的 `<Suspense>` 内：不用 Resource，Effect 取一次写镜像信号。
    let books = RwSignal::new(None::<Result<Vec<Notebook>, ()>>);
    Effect::new(move |_| {
        spawn_local(async move {
            let result = owned_notebooks().await.map_err(|_| ());
            let _ = books.try_set(Some(result));
        });
    });
    let tags = state.tags;
    let confirm_trash = RwSignal::new(false);

    let dirty = Memo::new(move |_| state.dirty());
    let work = Memo::new(move |_| state.saved.with(|n| n.as_ref().map(|n| n.version)));
    let published = Memo::new(move |_| {
        state
            .saved
            .with(|n| n.as_ref().and_then(|n| n.published_version))
    });
    let knowledge = Memo::new(move |_| {
        state
            .saved
            .with(|n| n.as_ref().and_then(|n| n.knowledge_version))
    });
    let public_tone = move || {
        if published.get().is_some() {
            "clay"
        } else {
            "muted"
        }
    };
    // 有未保存修改时，操作会先保存出新版本，所以一致也允许点。
    let publish_same =
        Memo::new(move |_| !dirty.get() && work.get().is_some() && published.get() == work.get());
    let knowledge_same =
        Memo::new(move |_| !dirty.get() && work.get().is_some() && knowledge.get() == work.get());
    let selected_ids = Memo::new(move |_| state.draft.with(|d| d.notebook_ids.clone()));
    let can_trash = Memo::new(move |_| {
        state
            .saved
            .with(|n| n.as_ref().is_some_and(|n| n.deleted_at.is_none()))
    });
    let act = move |action: NoteAction| {
        spawn_local(async move {
            persist(state, Some(action)).await;
        });
    };

    view! {
        <aside class="write-drawer note-drawer" aria-label="笔记设置">
            <div class="write-drawer-head">
                <h2>"设置"</h2>
                <IconButton
                    icon=IconName::X
                    label="关闭设置"
                    size=ButtonSize::Sm
                    onclick=move |_| open.set(false)
                />
            </div>

            <section class="note-side-section" aria-labelledby="note-versions-h">
                <h2 id="note-versions-h" class="note-side-title">
                    "版本"
                </h2>
                <ol class="note-versions">
                    <li>
                        <div class="note-version-head">
                            <span>"工作稿"</span>
                            {move || {
                                work
                                    .get()
                                    .map(|v| {
                                        view! { <span class="note-version-num">{format!("v{v}")}</span> }
                                    })
                            }}
                        </div>
                        <p>"只有你能看到。停止输入约 1.2 秒后自动保存。"</p>
                    </li>
                    <li data-kind="public">
                        <div class="note-version-head">
                            <StatusDot label="公开版" tone=Signal::derive(move || public_tone().to_string())/>
                            {move || {
                                published
                                    .get()
                                    .map(|v| {
                                        view! { <span class="note-version-num">{format!("v{v}")}</span> }
                                    })
                            }}
                        </div>
                        {move || match (published.get(), work.get()) {
                            (None, _) => {
                                view! { <p>"仅自己可见。发布后访客可以在笔记页读到这一版。"</p> }
                                    .into_any()
                            }
                            (Some(p), Some(w)) if behind(w, p).is_some() => {
                                view! {
                                    <p class="note-version-ahead">
                                        {format!("工作稿比公开版新 {} 个版本，修改不会自动发布。", w - p)}
                                    </p>
                                }
                                    .into_any()
                            }
                            _ => view! { <p>"与工作稿一致。"</p> }.into_any(),
                        }}
                        <div class="note-version-actions">
                            <button
                                class=button_class(ButtonVariant::Primary, ButtonSize::Sm)
                                type="button"
                                disabled=move || locked.get() || publish_same.get()
                                on:click=move |_| act(NoteAction::Publish)
                            >
                                {move || if published.get().is_some() { "更新公开版" } else { "发布到前台" }}
                            </button>
                            {move || {
                                published
                                    .get()
                                    .is_some()
                                    .then(|| {
                                        view! {
                                            <button
                                                class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                                type="button"
                                                disabled=move || locked.get()
                                                on:click=move |_| act(NoteAction::Unpublish)
                                            >
                                                "撤回"
                                            </button>
                                        }
                                    })
                            }}
                        </div>
                    </li>
                    <li data-kind="knowledge">
                        <div class="note-version-head">
                            <span>"AI 知识库"</span>
                            {move || match knowledge.get() {
                                Some(v) => {
                                    view! { <span class="note-version-num">{format!("v{v}")}</span> }
                                        .into_any()
                                }
                                None => {
                                    view! { <span class="note-version-num">"未收录"</span> }.into_any()
                                }
                            }}
                        </div>
                        <p>"收录后，获得授权的 AI 可以读取这一版内容，无需公开发布。"</p>
                        {move || {
                            knowledge
                                .get()
                                .zip(work.get())
                                .and_then(|(k, w)| behind(w, k))
                                .map(|n| {
                                    view! {
                                        <p class="note-version-ahead">
                                            {format!("已收录的是 {n} 个版本之前的内容。")}
                                        </p>
                                    }
                                })
                        }}
                        <div class="note-version-actions">
                            <button
                                class=button_class(ButtonVariant::Secondary, ButtonSize::Sm)
                                type="button"
                                disabled=move || locked.get() || knowledge_same.get()
                                on:click=move |_| act(NoteAction::IncludeKnowledge)
                            >
                                "收录当前版本"
                            </button>
                            {move || {
                                knowledge
                                    .get()
                                    .is_some()
                                    .then(|| {
                                        view! {
                                            <button
                                                class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                                type="button"
                                                disabled=move || locked.get()
                                                on:click=move |_| act(NoteAction::ExcludeKnowledge)
                                            >
                                                "移出"
                                            </button>
                                        }
                                    })
                            }}
                        </div>
                    </li>
                </ol>
            </section>

            <section class="note-side-section" aria-labelledby="note-books-h">
                <h2 id="note-books-h" class="note-side-title">
                    "笔记本"
                </h2>
                {move || match books.get() {
                    Some(Ok(items)) if items.is_empty() => {
                        view! {
                            <p class="note-side-hint">
                                "还没有笔记本。"
                                <a href=Route::AdminNotebooks.to_string()>"新建一个"</a>
                            </p>
                        }
                            .into_any()
                    }
                    Some(Ok(items)) => {
                        // 已归档的笔记本只在当前笔记已收进去时显示。
                        view! {
                            <div class="note-books" role="group" aria-label="收录到笔记本">
                                <For
                                    each=move || {
                                        let ids = selected_ids.get();
                                        items
                                            .iter()
                                            .filter(|b| b.archived_at.is_none() || ids.contains(&b.id))
                                            .cloned()
                                            .collect::<Vec<_>>()
                                    }
                                    key=|book| (book.id, book.title.clone(), book.archived_at.is_some())
                                    children=move |book| {
                                        let id = book.id;
                                        let selected = Memo::new(move |_| {
                                            selected_ids.with(|ids| ids.contains(&id))
                                        });
                                        view! {
                                            <button
                                                class="note-book"
                                                type="button"
                                                aria-pressed=move || selected.get().to_string()
                                                disabled=move || locked.get()
                                                on:click=move |_| {
                                                    state
                                                        .draft
                                                        .update(|d| {
                                                            if let Some(at) = d.notebook_ids.iter().position(|b| *b == id) {
                                                                d.notebook_ids.remove(at);
                                                            } else {
                                                                d.notebook_ids.push(id);
                                                                d.notebook_ids.sort_unstable();
                                                            }
                                                        })
                                                }
                                            >
                                                {move || {
                                                    selected
                                                        .get()
                                                        .then(|| view! { <Icon name=IconName::Check size=12/> })
                                                }}
                                                {book.title.clone()}
                                                {book
                                                    .archived_at
                                                    .is_some()
                                                    .then(|| view! { <span class="note-book-archived">"已归档"</span> })}
                                            </button>
                                        }
                                    }
                                />
                            </div>
                        }
                            .into_any()
                    }
                    Some(Err(())) => {
                        view! {
                            <p class="note-side-hint" role="alert">
                                "笔记本加载失败。"
                            </p>
                        }
                            .into_any()
                    }
                    None => {
                        view! {
                            <p class="note-side-hint" role="status">
                                "正在读取笔记本…"
                            </p>
                        }
                            .into_any()
                    }
                }}
            </section>

            <section class="note-side-section">
                <label class="note-side-title" for="note-tags">
                    "标签"
                </label>
                <FormInput
                    id="note-tags"
                    placeholder="Rust, 排错, 读书"
                    disabled=locked
                    value=tags
                    oninput=Callback::new(move |value: String| {
                        state.draft.update(|d| d.tags = parse_tags(&value));
                        tags.set(value);
                    })
                />
            </section>

            <HistoryPanel state locked/>

            {move || {
                can_trash
                    .get()
                    .then(|| {
                        view! {
                            <div class="write-drawer-foot note-drawer-foot">
                                <div>
                                    <button
                                        class="write-trash"
                                        type="button"
                                        disabled=move || locked.get()
                                        on:click=move |_| confirm_trash.set(true)
                                    >
                                        "移入回收站"
                                    </button>
                                    <p>"会撤回公开版并移出知识库，之后可以恢复。"</p>
                                </div>
                            </div>
                            <ConfirmDialog
                                visible=confirm_trash
                                title="把这条笔记移入回收站？"
                                description="公开版会撤回，AI 知识库也不再收录。之后可以在回收站恢复为私密草稿。"
                                confirm_label="移入回收站"
                                on_confirm=Callback::new(move |()| {
                                    spawn_local(async move {
                                        if persist(state, Some(NoteAction::Trash)).await {
                                            crate::navigation::push(&Route::AdminNotes.to_string());
                                        }
                                    });
                                })
                            />
                        }
                    })
            }}
        </aside>
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
