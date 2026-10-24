//! 笔记本管理（`/admin/notebooks`）。
//!
//! 左栏：使用中 / 已归档选项卡与笔记本列表；右栏：选中笔记本的名称、简介、公开开关、
//! 目录顺序（上移下移）与归档，或新建笔记本的表单。名称、简介、公开与顺序一起保存，
//! 只提交改动过的部分。数据只在浏览器鉴权后读取。样式见 styles/pages/admin-notes.css。
use crate::api::notes::*;
use crate::components::admin_layout::AdminPageHead;
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::forms::{FormInput, FormLabel, FormTextarea, ToggleSwitch};
use crate::components::skeletons::notes_admin_skeleton::NotebooksSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, ConfirmDialog, Icon, IconButton, IconName,
    LoadError,
};
use crate::models::note::*;
use crate::utils::error::failure_message;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{lazy_route, LazyRoute};

/// `/admin/notebooks`。数据全部在浏览器鉴权后读取，没有 SSR 数据。
pub struct NotebooksRoute;

#[lazy_route]
impl LazyRoute for NotebooksRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <AdminNotebooks/> }.into_any()
    }
}

/// 右栏正在编辑的对象。
#[derive(Clone, Copy, PartialEq)]
enum Selection {
    Book(i32),
    New,
}

/// 整页所处的阶段；只在阶段变化时重建整块内容，列表刷新不会重建右栏编辑器。
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Failed,
    Loading,
    Empty,
    Grid,
}

/// 新建前已有的笔记本之外多出来的那本；列表还没刷新时为 `None`。
fn created(items: &[Notebook], known: &[i32]) -> Option<i32> {
    items.iter().map(|b| b.id).find(|id| !known.contains(id))
}

#[component]
fn AdminNotebooks() -> impl IntoView {
    // 列表镜像信号：刷新时保留上一份数据直到新结果到达。
    let books = RwSignal::new(None::<Result<Vec<Notebook>, ()>>);
    let reload = RwSignal::new(0_u32);
    let generation = StoredValue::new(0_u64);
    let archived = RwSignal::new(false);
    let selection = RwSignal::new(None::<Selection>);
    // 新建时记下已有的笔记本；列表刷新出新的那本后选中它。
    let known = RwSignal::new(None::<Vec<i32>>);

    Effect::new(move |_| {
        reload.track();
        let Some(id) = generation.try_update_value(|g| {
            *g += 1;
            *g
        }) else {
            return;
        };
        spawn_local(async move {
            let result = owned_notebooks().await.map_err(|_| ());
            if generation.try_get_value() == Some(id) {
                let _ = books.try_set(Some(result));
            }
        });
    });
    let restart = Callback::new(move |()| reload.update(|n| *n = n.wrapping_add(1)));

    // 首次加载、切换选项卡或选中项消失时，选中当前选项卡的第一本。
    Effect::new(move |_| {
        let Some(Ok(items)) = books.get() else {
            return;
        };
        if let Some(before) = known.get() {
            // 还是刷新前的列表时保持新建表单。
            if let Some(id) = created(&items, &before) {
                known.set(None);
                archived.set(false);
                selection.set(Some(Selection::Book(id)));
            }
            return;
        }
        let archived = archived.get();
        let shown: Vec<i32> = items
            .iter()
            .filter(|b| b.archived_at.is_some() == archived)
            .map(|b| b.id)
            .collect();
        let keep = match selection.get_untracked() {
            Some(Selection::New) => true,
            Some(Selection::Book(id)) => shown.contains(&id),
            None => false,
        };
        if !keep {
            selection.set(shown.first().copied().map(Selection::Book));
        }
    });

    let current = Memo::new(move |_| selection.get());
    let active_count = Memo::new(move |_| match books.get() {
        Some(Ok(items)) => items.iter().filter(|b| b.archived_at.is_none()).count(),
        _ => 0,
    });
    let archived_count = Memo::new(move |_| match books.get() {
        Some(Ok(items)) => items.len() - active_count.get(),
        _ => 0,
    });
    let phase = Memo::new(move |_| match books.get() {
        None => Phase::Loading,
        Some(Err(())) => Phase::Failed,
        Some(Ok(items)) if items.is_empty() && current.get() != Some(Selection::New) => {
            Phase::Empty
        }
        Some(Ok(_)) => Phase::Grid,
    });
    let shown_books = Memo::new(move |_| match books.get() {
        Some(Ok(items)) => {
            let archived = archived.get();
            items
                .into_iter()
                .filter(|b| b.archived_at.is_some() == archived)
                .collect::<Vec<_>>()
        }
        _ => Vec::new(),
    });

    view! {
        <div class="admin-notebooks">
            <AdminPageHead
                title="笔记本"
                actions=move || {
                    view! {
                        <button
                            class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                            type="button"
                            on:click=move |_| selection.set(Some(Selection::New))
                        >
                            <Icon name=IconName::Plus size=16/>
                            "新建笔记本"
                        </button>
                    }
                }
            >
                "把相关的记录放在一起，按你的思路排列。"
            </AdminPageHead>
            {move || match phase.get() {
                Phase::Failed => {
                    view! { <LoadError title="笔记本加载失败" on_retry=restart/> }.into_any()
                }
                Phase::Loading => view! { <NotebooksSkeleton/> }.into_any(),
                Phase::Empty => {
                    view! {
                        <div class="admin-panel">
                            <EmptyState
                                icon=IconName::BookOpen
                                title="还没有笔记本"
                                description="把一组主题笔记放进笔记本，按你的顺序排列；公开后访客可以从第一篇读起。"
                                action=EmptyStateAction::new(
                                    "新建笔记本",
                                    move |_| selection.set(Some(Selection::New)),
                                )
                            />
                        </div>
                    }
                        .into_any()
                }
                Phase::Grid => {
                    view! {
                        <div class="admin-notebooks-grid">
                            <section class="admin-panel admin-notebooks-list" aria-label="笔记本列表">
                                <div class="admin-notebooks-tabs" role="tablist" aria-label="笔记本状态">
                                    {[(false, "使用中"), (true, "已归档")]
                                        .into_iter()
                                        .map(|(value, label)| {
                                            view! {
                                                <button
                                                    type="button"
                                                    role="tab"
                                                    aria-selected=move || (archived.get() == value).to_string()
                                                    on:click=move |_| {
                                                        archived.set(value);
                                                        if selection.get_untracked() == Some(Selection::New) {
                                                            selection.set(None);
                                                        }
                                                    }
                                                >
                                                    {label}
                                                    <span>
                                                        {move || {
                                                            if value {
                                                                archived_count.get()
                                                            } else {
                                                                active_count.get()
                                                            }
                                                        }}
                                                    </span>
                                                </button>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                                <ul>
                                    <For
                                        each=move || shown_books.get()
                                        key=|book| (book.id, book.note_count, book.is_public, book.archived_at.is_some(), book.title.clone())
                                        children=move |book| {
                                            let id = book.id;
                                            let status = if book.archived_at.is_some() {
                                                "已归档"
                                            } else if book.is_public {
                                                "公开"
                                            } else {
                                                "私密"
                                            };
                                            let public_dot = book.is_public && book.archived_at.is_none();
                                            view! {
                                                <li>
                                                    <button
                                                        class="admin-notebook-item"
                                                        type="button"
                                                        aria-pressed=move || {
                                                            (current.get() == Some(Selection::Book(id))).to_string()
                                                        }
                                                        on:click=move |_| selection.set(Some(Selection::Book(id)))
                                                    >
                                                        <span class="admin-notebook-item-text">
                                                            <span class="admin-notebook-item-title">{book.title}</span>
                                                            <span class="admin-notebook-item-meta">
                                                                {format!("{} 篇 · ", book.note_count)} {status}
                                                            </span>
                                                        </span>
                                                        {public_dot
                                                            .then(|| {
                                                                view! { <span class="admin-notebook-item-dot" title="公开"></span> }
                                                            })}
                                                    </button>
                                                </li>
                                            }
                                        }
                                    />
                                </ul>
                                {move || {
                                    shown_books
                                        .with(|shown| shown.is_empty())
                                        .then(|| {
                                            view! {
                                                <p class="admin-notebooks-empty">
                                                    {if archived.get() {
                                                        "没有已归档的笔记本。"
                                                    } else {
                                                        "没有使用中的笔记本。"
                                                    }}
                                                </p>
                                            }
                                        })
                                }}
                            </section>
                            {move || match current.get() {
                                Some(Selection::New) => {
                                    view! {
                                        <NotebookEditor
                                            book=Signal::stored(None)
                                            on_saved=Callback::new(move |()| {
                                                let ids = books
                                                    .with_untracked(|b| match b {
                                                        Some(Ok(items)) => items.iter().map(|b| b.id).collect(),
                                                        _ => Vec::new(),
                                                    });
                                                known.set(Some(ids));
                                                restart.run(());
                                            })
                                            on_cancel=Callback::new(move |()| selection.set(None))
                                        />
                                    }
                                        .into_any()
                                }
                                Some(Selection::Book(id)) => {
                                    let book = Memo::new(move |_| {
                                        books
                                            .with(|b| match b {
                                                Some(Ok(items)) => items.iter().find(|b| b.id == id).cloned(),
                                                _ => None,
                                            })
                                    });
                                    let exists = Memo::new(move |_| book.with(Option::is_some));
                                    view! {
                                        {move || {
                                            if exists.get() {
                                                view! {
                                                    <NotebookEditor
                                                        book=Signal::from(book)
                                                        on_saved=restart
                                                        on_cancel=Callback::new(|()| {})
                                                    />
                                                }
                                                    .into_any()
                                            } else {
                                                blank_editor().into_any()
                                            }
                                        }}
                                    }
                                        .into_any()
                                }
                                None => blank_editor().into_any(),
                            }}
                        </div>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

fn blank_editor() -> impl IntoView {
    view! {
        <div class="admin-panel admin-notebook-editor admin-notebook-blank">
            <p>"选择左侧的笔记本进行编辑。"</p>
        </div>
    }
}

/// 右栏：名称、简介、公开、目录顺序与归档。`book` 为 `None` 时是新建表单；
/// 保存后列表刷新，`book` 随之更新，「已保存」状态由它推出。
#[component]
fn NotebookEditor(
    book: Signal<Option<Notebook>>,
    on_saved: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let toast = use_toast();
    let id = book.with_untracked(|b| b.as_ref().map(|b| b.id));
    let is_archived =
        Memo::new(move |_| book.with(|b| b.as_ref().is_some_and(|b| b.archived_at.is_some())));
    let base_title =
        Memo::new(move |_| book.with(|b| b.as_ref().map(|b| b.title.clone()).unwrap_or_default()));
    let base_description = Memo::new(move |_| {
        book.with(|b| {
            b.as_ref()
                .map(|b| b.description.clone())
                .unwrap_or_default()
        })
    });
    let base_public = Memo::new(move |_| book.with(|b| b.as_ref().is_some_and(|b| b.is_public)));
    let title = RwSignal::new(base_title.get_untracked());
    let description = RwSignal::new(base_description.get_untracked());
    let public = RwSignal::new(base_public.get_untracked());
    let busy = RwSignal::new(false);
    let confirm_archive = RwSignal::new(false);

    // 目录：服务端顺序与当前编排。刷新时保留上一份数据直到新结果到达。
    let order = RwSignal::new(None::<Result<(), ()>>);
    let order_reload = RwSignal::new(0_u32);
    let order_generation = StoredValue::new(0_u64);
    let baseline = RwSignal::new(Vec::<NotebookEntry>::new());
    let rows = RwSignal::new(Vec::<NotebookEntry>::new());
    let moved = RwSignal::new(None::<i32>);
    if let Some(book_id) = id {
        Effect::new(move |_| {
            order_reload.track();
            let Some(token) = order_generation.try_update_value(|g| {
                *g += 1;
                *g
            }) else {
                return;
            };
            spawn_local(async move {
                let result = notebook_order(book_id).await;
                if order_generation.try_get_value() != Some(token) {
                    return;
                }
                match result {
                    Ok(items) => {
                        let _ = baseline.try_set(items.clone());
                        let _ = rows.try_set(items);
                        let _ = order.try_set(Some(Ok(())));
                    }
                    Err(_) => {
                        let _ = order.try_set(Some(Err(())));
                    }
                }
            });
        });
    }
    let retry_order = Callback::new(move |()| order_reload.update(|n| *n = n.wrapping_add(1)));

    let meta_dirty = Memo::new(move |_| {
        title.get() != base_title.get()
            || description.get() != base_description.get()
            || public.get() != base_public.get()
    });
    let order_dirty = Memo::new(move |_| {
        rows.with(|r| baseline.with(|b| r.iter().map(|r| r.id).ne(b.iter().map(|r| r.id))))
    });
    let dirty = Memo::new(move |_| meta_dirty.get() || order_dirty.get());
    let can_save = Memo::new(move |_| {
        !busy.get() && !title.with(|t| t.trim().is_empty()) && (dirty.get() || id.is_none())
    });

    let save = move || {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let ids: Vec<i32> = rows.with_untracked(|r| r.iter().map(|r| r.id).collect());
        let meta_dirty = meta_dirty.get_untracked();
        let order_dirty = order_dirty.get_untracked();
        let input = NotebookInput {
            id,
            title: title.get_untracked(),
            description: description.get_untracked(),
            is_public: public.get_untracked(),
        };
        spawn_local(async move {
            let mut result: Result<(), ServerFnError> = Ok(());
            if meta_dirty || id.is_none() {
                result = save_notebook(input).await;
            }
            if let (Ok(()), Some(book_id), true) = (&result, id, order_dirty) {
                result = reorder_notebook(book_id, ids).await;
                if result.is_ok() {
                    if let Some(saved) = rows.try_get_untracked() {
                        let _ = baseline.try_set(saved);
                    }
                    let _ = moved.try_set(None);
                }
            }
            match result {
                Ok(()) => {
                    toast.success(if id.is_some() {
                        "笔记本已保存"
                    } else {
                        "笔记本已创建"
                    });
                    on_saved.try_run(());
                }
                Err(e) => {
                    toast.error(failure_message("保存失败", &e));
                    let _ = order_reload.try_update(|n| *n = n.wrapping_add(1));
                }
            }
            let _ = busy.try_set(false);
        });
    };
    let set_archived = move |archive: bool| {
        let Some(book_id) = id else {
            return;
        };
        busy.set(true);
        spawn_local(async move {
            match archive_notebook(book_id, archive).await {
                Ok(()) => {
                    toast.success(if archive {
                        "已归档，公开目录不再显示"
                    } else {
                        "已恢复，目录保持私密"
                    });
                    on_saved.try_run(());
                }
                Err(e) => toast.error(failure_message("操作失败", &e)),
            }
            let _ = busy.try_set(false);
        });
    };
    let shift = move |index: usize, up: bool| {
        let to = if up { index - 1 } else { index + 1 };
        rows.update(|r| r.swap(index, to));
        moved.set(rows.with_untracked(|r| r.get(to).map(|r| r.id)));
    };

    let heading = if id.is_some() {
        "编辑笔记本"
    } else {
        "新建笔记本"
    };
    let count = Memo::new(move |_| rows.with(Vec::len));

    view! {
        <section class="admin-panel admin-notebook-editor" aria-labelledby="notebook-editor-h">
            <div class="admin-notebook-fields">
                <h2 id="notebook-editor-h">{heading}</h2>
                <div>
                    <FormLabel label="名称" html_for="notebook-title"/>
                    <FormInput
                        id="notebook-title"
                        maxlength=100
                        required=true
                        value=title
                        oninput=Callback::new(move |v: String| title.set(v))
                    />
                </div>
                <div>
                    <FormLabel label="简介" html_for="notebook-description"/>
                    <FormTextarea
                        id="notebook-description"
                        rows=2
                        maxlength=1000
                        placeholder="一两句话说明这本笔记写什么"
                        value=description
                        oninput=Callback::new(move |v: String| description.set(v))
                    />
                </div>
                <div class="admin-notebook-switch">
                    <ToggleSwitch
                        checked=public
                        disabled=is_archived
                        aria_label="公开展示这个笔记本"
                        ontoggle=Callback::new(move |()| public.update(|p| *p = !*p))
                    />
                    <div>
                        <p>"公开展示这个笔记本"</p>
                        <p class="admin-notebook-hint">
                            {move || {
                                if is_archived.get() {
                                    "已归档的笔记本须先恢复，才能公开。"
                                } else {
                                    "访客只能看到其中已发布的笔记。"
                                }
                            }}
                        </p>
                    </div>
                </div>
            </div>

            {id
                .is_some()
                .then(|| {
                    view! {
                        <div class="admin-notebook-order">
                            <div class="admin-notebook-order-head">
                                <h3>"目录顺序"</h3>
                                <span>"上移或下移调整阅读顺序，保存后生效"</span>
                            </div>
                            {move || match order.get() {
                                None => {
                                    view! {
                                        <p class="admin-notebook-hint" role="status">
                                            "正在读取目录…"
                                        </p>
                                    }
                                        .into_any()
                                }
                                Some(Err(())) => {
                                    view! { <LoadError title="目录加载失败" on_retry=retry_order/> }
                                        .into_any()
                                }
                                Some(Ok(())) if count.get() == 0 => {
                                    view! {
                                        <p class="admin-notebook-hint">
                                            "这个笔记本还没有笔记。在笔记编辑器的「设置」里把笔记收进来。"
                                        </p>
                                    }
                                        .into_any()
                                }
                                Some(Ok(())) => {
                                    view! {
                                        <ol class="admin-notebook-rows">
                                            <For
                                                each=move || rows.get()
                                                key=|entry| entry.id
                                                children=move |entry| {
                                                    let entry_id = entry.id;
                                                    let index = Memo::new(move |_| {
                                                        rows.with(|r| r.iter().position(|e| e.id == entry_id))
                                                            .unwrap_or(0)
                                                    });
                                                    view! {
                                                        <li data-moved=move || {
                                                            (moved.get() == Some(entry_id)).then_some("true")
                                                        }>
                                                            <span class="admin-notebook-row-n">
                                                                {move || format!("{:02}", index.get() + 1)}
                                                            </span>
                                                            <span class="admin-notebook-row-text">
                                                                <span class="admin-notebook-row-title">
                                                                    {entry.title.clone()}
                                                                </span>
                                                                <span class="admin-notebook-row-state">
                                                                    {entry.state_label()}
                                                                </span>
                                                            </span>
                                                            <IconButton
                                                                icon=IconName::ChevronUp
                                                                label=format!("上移「{}」", entry.title)
                                                                size=ButtonSize::Sm
                                                                disabled=Signal::derive(move || {
                                                                    index.get() == 0 || busy.get()
                                                                })
                                                                onclick=move |_| shift(index.get_untracked(), true)
                                                            />
                                                            <IconButton
                                                                icon=IconName::ChevronDown
                                                                label=format!("下移「{}」", entry.title)
                                                                size=ButtonSize::Sm
                                                                disabled=Signal::derive(move || {
                                                                    index.get() + 1 == count.get() || busy.get()
                                                                })
                                                                onclick=move |_| shift(index.get_untracked(), false)
                                                            />
                                                        </li>
                                                    }
                                                }
                                            />
                                        </ol>
                                    }
                                        .into_any()
                                }
                            }}
                        </div>
                    }
                })}

            <div class="admin-notebook-foot">
                {if id.is_some() {
                    view! {
                        <div class="admin-notebook-archive">
                            {move || {
                                if is_archived.get() {
                                    view! {
                                        <button
                                            class="admin-notebook-link"
                                            type="button"
                                            disabled=move || busy.get()
                                            on:click=move |_| set_archived(false)
                                        >
                                            "恢复这个笔记本"
                                        </button>
                                        <p>"恢复后目录保持私密，需要时再打开公开。"</p>
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <button
                                            class="admin-notebook-link"
                                            type="button"
                                            disabled=move || busy.get()
                                            on:click=move |_| confirm_archive.set(true)
                                        >
                                            "归档这个笔记本"
                                        </button>
                                        <p>
                                            "归档会隐藏公开目录，不删除笔记，也不撤回笔记自身的发布或 AI 授权。"
                                        </p>
                                    }
                                        .into_any()
                                }
                            }}
                        </div>
                    }
                        .into_any()
                } else {
                    view! {
                        <button
                            class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                            type="button"
                            on:click=move |_| on_cancel.run(())
                        >
                            "取消"
                        </button>
                    }
                        .into_any()
                }}
                <button
                    class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                    type="button"
                    disabled=move || !can_save.get()
                    on:click=move |_| save()
                >
                    {move || {
                        if busy.get() {
                            "保存中…"
                        } else if id.is_none() {
                            "创建笔记本"
                        } else if dirty.get() {
                            "保存"
                        } else {
                            "已保存"
                        }
                    }}
                </button>
            </div>
        </section>
        <ConfirmDialog
            visible=confirm_archive
            title="归档这个笔记本？"
            description="公开目录会隐藏，笔记本身不受影响：已发布的笔记仍然公开，AI 授权也保留。之后可以在「已归档」里恢复。"
            confirm_label="归档"
            icon=IconName::Archive
            on_confirm=Callback::new(move |()| set_archived(true))
        />
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_finds_the_notebook_missing_before_save() {
        let book = |id| Notebook {
            id,
            ..Default::default()
        };
        assert_eq!(created(&[book(3), book(9), book(4)], &[3, 4]), Some(9));
        // 列表尚未刷新。
        assert_eq!(created(&[book(3), book(4)], &[3, 4]), None);
    }
}
