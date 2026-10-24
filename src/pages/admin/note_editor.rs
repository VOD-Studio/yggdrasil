//! 写笔记与编辑笔记（`/admin/notes/new`、`/admin/notes/edit/:id`）。
//!
//! 与写文章同一套无干扰版式：页头（返回、保存状态、查看公开页面、预览、设置、保存）
//! 替代后台顶栏；类型分段、衬线标题与 Tiptap 正文居中；右侧设置抽屉是版本面板
//! （工作稿、公开版、AI 知识库）、笔记本、标签与历史版本。工作稿停止输入后自动保存，
//! 公开与知识库快照只由明确操作更新。样式沿用 styles/pages/admin-write.css，
//! 笔记专属部分在 admin-notes.css。数据只在浏览器鉴权后读取。
mod composer;
mod history;
mod metadata;
mod save;
mod state;

use super::note_drafts::use_draft_protection;
use crate::api::notes::get_owned_note;
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::skeletons::write_skeleton::WriteSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconName, LoadError,
    SegmentedControl, SegmentedItem,
};
use crate::models::note::*;
use crate::routes::Route;
use crate::utils::error::is_not_found;
use composer::NoteComposer;
use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;
use leptos_router::{lazy_route, LazyRoute};
use metadata::MetadataPanel;
use save::persist;
use state::SaveStatus;

/// `/admin/notes/new`。
pub struct NewNoteRoute;

#[lazy_route]
impl LazyRoute for NewNoteRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <NoteEditor initial=None/> }.into_any()
    }
}

/// `/admin/notes/edit/:id`。
pub struct EditNoteRoute {
    id: Memo<i32>,
}

#[lazy_route]
impl LazyRoute for EditNoteRoute {
    fn data() -> Self {
        let params = use_params_map();
        Self {
            id: Memo::new(move |_| {
                params
                    .read()
                    .get("id")
                    .and_then(|id| id.parse().ok())
                    .unwrap_or_default()
            }),
        }
    }

    fn view(this: Self) -> AnyView {
        let id = this.id;
        // 换一条笔记就整体重建编辑器，不带走上一条的状态。
        view! {
            {move || {
                let id = id.get();
                view! { <LoadNoteEditor id=id/> }
            }}
        }
        .into_any()
    }
}

/// 加载失败的两种形态：笔记不存在，或其他错误（可重试）。
#[derive(Clone, Copy, PartialEq)]
enum LoadFailure {
    Missing,
    Failed,
}

#[component]
fn LoadNoteEditor(id: i32) -> impl IntoView {
    // 编辑器位于路由的 `<Suspense>` 内，笔记也会重试加载：用 Effect + 镜像信号，不用 Resource。
    let response = RwSignal::new(None::<Result<Note, LoadFailure>>);
    let reload = RwSignal::new(0_u32);
    let generation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        reload.track();
        let Some(token) = generation.try_update_value(|g| {
            *g += 1;
            *g
        }) else {
            return;
        };
        spawn_local(async move {
            let result = get_owned_note(id, None).await.map_err(|e| {
                if is_not_found(&e) {
                    LoadFailure::Missing
                } else {
                    LoadFailure::Failed
                }
            });
            if generation.try_get_value() == Some(token) {
                let _ = response.try_set(Some(result));
            }
        });
    });

    move || match response.get() {
        Some(Ok(note)) => view! { <NoteEditor initial=Some(note)/> }.into_any(),
        Some(Err(LoadFailure::Missing)) => view! {
            <div class="write-missing">
                <EmptyState
                    icon=IconName::FileX
                    title="找不到这条笔记"
                    description="它可能已被彻底删除。"
                    action=EmptyStateAction::new(
                        "返回笔记",
                        move |_| {
                            crate::navigation::push(&Route::AdminNotes.to_string());
                        },
                    )
                />
            </div>
        }
        .into_any(),
        Some(Err(LoadFailure::Failed)) => view! {
            <div class="write-missing">
                <LoadError
                    title="笔记加载失败"
                    on_retry=Callback::new(move |()| reload.update(|n| *n = n.wrapping_add(1)))
                />
            </div>
        }
        .into_any(),
        None => view! { <WriteSkeleton/> }.into_any(),
    }
}

#[component]
fn NoteEditor(initial: Option<Note>) -> impl IntoView {
    let toast = use_toast();
    let state = state::use_editor_state(initial, toast);
    let slot = state
        .draft
        .with_untracked(|d| d.id)
        .map(|id| id.to_string())
        .unwrap_or_else(|| "new".into());
    let (recovery, cache_failed) = use_draft_protection(slot, state.draft, move || {
        state.dirty() || state.busy() || state.uploading.get()
    });
    let preview = RwSignal::new(false);
    let drawer_open = RwSignal::new(true);
    // 只有点「设置」打开时才播滑入动画；首屏、换页与编辑器挂载时直接出现。
    let drawer_animate = RwSignal::new(false);
    // 首屏与 SSR 一致，水合后窄屏默认收起设置抽屉。
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let wide = web_sys::window()
                .and_then(|w| w.inner_width().ok())
                .and_then(|w| w.as_f64())
                .is_some_and(|w| w >= 1024.0);
            drawer_open.set(wide);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = drawer_open;
    });
    save::use_autosave(state, recovery);

    let save_now = move || {
        spawn_local(async move {
            persist(state, None).await;
        });
    };
    let toggle_preview = move |_| {
        spawn_local(async move {
            if preview.try_get_untracked() == Some(true) || persist(state, None).await {
                let _ = preview.try_update(|p| *p = !*p);
            }
        });
    };

    let error = Memo::new(move |_| state.error());
    let deleted = Memo::new(move |_| {
        state
            .saved
            .with(|n| n.as_ref().is_some_and(|n| n.deleted_at.is_some()))
    });
    let recovering = Memo::new(move |_| recovery.with(Option::is_some));
    let status = Memo::new(move |_| {
        SaveStatus::of(
            state.busy(),
            !error.get().is_empty(),
            state.dirty(),
            recovering.get(),
            state.saved.with(|n| n.as_ref().map(|n| n.version)),
        )
    });
    let failed = Memo::new(move |_| status.get() == SaveStatus::Failed);
    let public_url = Memo::new(move |_| {
        state.saved.with(|n| {
            n.as_ref()
                .filter(|n| n.published_version.is_some())
                .map(|n| format!("/notes/{}", n.slug))
        })
    });
    let locked = Memo::new(move |_| state.busy() || deleted.get() || recovering.get());
    let kind = Memo::new(move |_| state.draft.with(|d| d.kind));

    view! {
        <div
            class="write-page note-editor"
            on:keydown=move |e: KeyboardEvent| {
                if (e.meta_key() || e.ctrl_key()) && e.key() == "s" {
                    e.prevent_default();
                    if !locked.get_untracked() {
                        save_now();
                    }
                }
            }
        >
            <header class="write-head">
                <div class="write-head-left">
                    <a
                        class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                        href=Route::AdminNotes.to_string()
                    >
                        <Icon name=IconName::ArrowLeft size=16/>
                        <span class="write-hide-narrow">"笔记管理"</span>
                        <span class="sr-only">"返回"</span>
                    </a>
                </div>
                <div class="write-head-actions">
                    {move || {
                        if failed.get() {
                            view! {
                                <button
                                    class="write-status"
                                    type="button"
                                    data-tone=move || status.get().tone()
                                    data-state=move || status.get().key()
                                    title=move || error.get()
                                    on:click=move |_| save_now()
                                >
                                    <span class="write-status-dot" aria-hidden="true"></span>
                                    <span role="status">{move || status.get().label()}</span>
                                </button>
                            }
                                .into_any()
                        } else {
                            view! {
                                <span
                                    class="write-status"
                                    data-tone=move || status.get().tone()
                                    data-state=move || status.get().key()
                                    data-pulse=move || (status.get() == SaveStatus::Saving).then_some("true")
                                >
                                    <span class="write-status-dot" aria-hidden="true"></span>
                                    <span role="status" aria-live="polite">
                                        {move || status.get().label()}
                                    </span>
                                </span>
                            }
                                .into_any()
                        }
                    }}
                    {move || {
                        public_url
                            .get()
                            .map(|url| {
                                view! {
                                    <a
                                        class=format!(
                                            "{} write-hide-narrow",
                                            button_class(ButtonVariant::Ghost, ButtonSize::Md),
                                        )
                                        href=url
                                        target="_blank"
                                        rel="noopener"
                                    >
                                        "查看公开页面"
                                        <Icon name=IconName::ArrowUpRight size=14/>
                                    </a>
                                }
                            })
                    }}
                    <button
                        class=format!(
                            "{} write-hide-narrow",
                            button_class(ButtonVariant::Ghost, ButtonSize::Md),
                        )
                        type="button"
                        disabled=move || {
                            state.busy() || state.saved.with(Option::is_none) || recovering.get()
                        }
                        aria-pressed=move || preview.get().to_string()
                        on:click=toggle_preview
                    >
                        {move || if preview.get() { "返回编辑" } else { "预览" }}
                    </button>
                    <button
                        class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                        type="button"
                        aria-pressed=move || drawer_open.get().to_string()
                        aria-controls="note-drawer"
                        // 窄屏只剩图标，名称由 aria-label 补出。
                        aria-label="设置"
                        on:click=move |_| {
                            drawer_animate.set(true);
                            drawer_open.update(|open| *open = !*open);
                        }
                    >
                        <Icon name=IconName::PanelRight size=15/>
                        <span class="write-hide-narrow">"设置"</span>
                    </button>
                    <button
                        class=button_class(ButtonVariant::Secondary, ButtonSize::Md)
                        type="button"
                        disabled=move || locked.get()
                        title="⌘S"
                        on:click=move |_| save_now()
                    >
                        "保存草稿"
                    </button>
                </div>
            </header>

            {move || {
                recovery
                    .get()
                    .map(|local| {
                        view! {
                            <div class="write-banner" role="alert">
                                <Icon name=IconName::RotateCcwClock size=18/>
                                <div>
                                    <p class="write-banner-title">"这个标签页留有未保存的内容"</p>
                                    <p>
                                        "恢复只修改工作稿，不会发布；若服务端已有新版本，请先核对内容。选择之前编辑器暂停输入。"
                                    </p>
                                    <div class="write-banner-actions">
                                        <button
                                            class=button_class(ButtonVariant::Primary, ButtonSize::Sm)
                                            type="button"
                                            on:click=move |_| {
                                                state.restore_local(local.clone());
                                                recovery.set(None);
                                            }
                                        >
                                            "恢复未保存内容"
                                        </button>
                                        <button
                                            class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                            type="button"
                                            on:click=move |_| recovery.set(None)
                                        >
                                            "使用服务端版本"
                                        </button>
                                    </div>
                                </div>
                            </div>
                        }
                    })
            }}

            <div class="write-body">
                <main class="write-main" data-vt-scroll="admin-note-content">
                    <div class="write-doc">
                        {move || {
                            let message = error.get();
                            (!message.is_empty())
                                .then(|| {
                                    view! {
                                        <p class="write-notice" role="alert">
                                            <span>{message}</span>
                                        </p>
                                    }
                                })
                        }}
                        {move || {
                            cache_failed
                                .get()
                                .then(|| {
                                    view! {
                                        <p class="write-notice" role="alert">
                                            "浏览器无法保存本地恢复副本，请先保存到服务端再离开。"
                                        </p>
                                    }
                                })
                        }}
                        {move || {
                            deleted
                                .get()
                                .then(|| {
                                    view! {
                                        <div class="note-trashed" role="status">
                                            <Icon name=IconName::Trash size=16/>
                                            <p>
                                                "这条笔记在回收站中。恢复后可继续编辑，公开与知识库收录需要重新确认。"
                                            </p>
                                            <button
                                                class=button_class(ButtonVariant::Primary, ButtonSize::Sm)
                                                type="button"
                                                disabled=move || state.busy()
                                                on:click=move |_| {
                                                    spawn_local(async move {
                                                        persist(state, Some(NoteAction::Restore)).await;
                                                    });
                                                }
                                            >
                                                "恢复笔记"
                                            </button>
                                        </div>
                                    }
                                })
                        }}
                        {move || {
                            if preview.get() {
                                // 预览复用公开渲染路径：完整 Note 转为 PublicNote（编辑器的工作稿
                                // 版本号保留展示），显示字段不变，Markdown 不会进入公开渲染组件。
                                view! {
                                    {move || {
                                        state
                                            .saved
                                            .get()
                                            .map(|note| {
                                                view! {
                                                    <crate::pages::notes::NoteDocument
                                                        note=crate::models::public_note::PublicNote::from(note)
                                                        preview=true
                                                    />
                                                }
                                            })
                                    }}
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <SegmentedControl
                                        class="note-kind-tabs"
                                        aria_label="笔记类型"
                                        items=vec![
                                            SegmentedItem::new("moment".to_string(), "随记"),
                                            SegmentedItem::new("topic".to_string(), "主题笔记"),
                                        ]
                                        value=Signal::derive(move || kind.get().as_str().to_string())
                                        on_change=Callback::new(move |value: String| {
                                            if !locked.get_untracked() {
                                                state
                                                    .draft
                                                    .update(|d| {
                                                        d.kind = if value == "topic" {
                                                            NoteKind::Topic
                                                        } else {
                                                            NoteKind::Moment
                                                        };
                                                    });
                                            }
                                        })
                                    />
                                    <label class="sr-only" for="note-title">
                                        "笔记标题"
                                    </label>
                                    <textarea
                                        id="note-title"
                                        class="write-title note-title"
                                        rows="1"
                                        maxlength="200"
                                        readonly=move || recovering.get() || deleted.get()
                                        placeholder=move || {
                                            if kind.get() == NoteKind::Moment {
                                                "标题可选，先记下来…"
                                            } else {
                                                "给这篇笔记一个标题"
                                            }
                                        }
                                        prop:value=move || state.draft.with(|d| d.title.clone())
                                        on:input=move |ev| {
                                            let value = event_target_value(&ev).replace(['\n', '\r'], "");
                                            state.draft.update(|d| d.title = value);
                                        }
                                        on:keydown=move |e: KeyboardEvent| {
                                            if e.key() == "Enter" && !e.is_composing() {
                                                e.prevent_default();
                                                composer::focus_editor();
                                            }
                                        }
                                    >
                                        {state.draft.with_untracked(|d| d.title.clone())}
                                    </textarea>
                                    <div
                                        class="write-editor-wrap"
                                        inert=move || (recovering.get() || deleted.get()).then_some("true")
                                    >
                                        {move || {
                                            let _ = state.editor_epoch.get();
                                            let initial = state.draft.with_untracked(|d| d.content_md.clone());
                                            view! {
                                                <NoteComposer
                                                    initial=initial
                                                    on_change=Callback::new(move |md: String| {
                                                        state.draft.update(|d| d.content_md = md)
                                                    })
                                                    uploading=state.uploading
                                                />
                                            }
                                        }}
                                    </div>
                                }
                                    .into_any()
                            }
                        }}
                    </div>
                </main>
                {move || {
                    drawer_open
                        .get()
                        .then(|| {
                            view! {
                                <div
                                    id="note-drawer"
                                    class="write-drawer-slot"
                                    data-enter=move || drawer_animate.get().then_some("true")
                                >
                                    <MetadataPanel state open=drawer_open locked/>
                                </div>
                            }
                        })
                }}
            </div>
        </div>
    }
}
