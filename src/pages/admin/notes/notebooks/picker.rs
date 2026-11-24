//! 笔记本目录的笔记选择器。一次打开期间跨搜索、翻页保留勾选。

use crate::api::notes::{add_notebook_notes, notebook_candidates};
use crate::components::ui::{
    Button, ButtonSize, ButtonVariant, Checkbox, Icon, IconButton, IconName, LoadError, ModalShell,
    SegmentedControl, SegmentedItem, SegmentedKind, SegmentedSize, TablePager,
};
use crate::hooks::query::use_paginated;
use crate::models::note::{
    NotebookAddResult, NotebookCandidate, MAX_NOTE_BATCH_SIZE, NOTES_PER_PAGE,
};
use crate::utils::error::failure_message;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Missing,
    Selected,
}

fn can_join(note: &NotebookCandidate) -> bool {
    !note.joined && !note.at_limit
}

#[component]
pub(super) fn NotebookNotePicker(
    book_id: i32,
    book_title: Signal<String>,
    is_public: Signal<bool>,
    visible: RwSignal<bool>,
    busy: RwSignal<bool>,
    on_added: Callback<NotebookAddResult>,
) -> impl IntoView {
    let closing = RwSignal::new(false);
    let close = Callback::new(move |()| {
        closing.set(true);
        visible.set(false);
    });
    view! {
        <ModalShell
            visible
            closing
            title="添加笔记"
            overlay_padding="dialog-overlay-responsive"
            panel_class="asset-modal notebook-picker"
            dismiss_disabled=busy
        >
            <PickerContents book_id book_title is_public busy close on_added/>
        </ModalShell>
    }
}

#[component]
fn PickerContents(
    book_id: i32,
    book_title: Signal<String>,
    is_public: Signal<bool>,
    busy: RwSignal<bool>,
    close: Callback<()>,
    on_added: Callback<NotebookAddResult>,
) -> impl IntoView {
    let filter = RwSignal::new(Filter::All);
    let search = RwSignal::new(String::new());
    let query = RwSignal::new(String::new());
    let search_generation = StoredValue::new(0_u64);
    let page = RwSignal::new(1_i32);
    let retry = RwSignal::new(0_u32);
    let selected = RwSignal::new(Vec::<NotebookCandidate>::new());
    let add_error = RwSignal::new(None::<String>);

    let data = use_paginated(
        move || page.get(),
        NOTES_PER_PAGE,
        move |page, _| {
            retry.track();
            let filter = filter.get();
            let query = query.get();
            async move {
                if filter == Filter::Selected {
                    return Ok((Vec::new(), 0));
                }
                notebook_candidates(book_id, query, page, filter == Filter::Missing)
                    .await
                    .map(|response| (response.notes, response.total))
                    .map_err(|e| failure_message("笔记加载失败", &e))
            }
        },
    );
    let searching = Memo::new(move |_| search.with(|v| query.with(|q| v.trim() != q)));
    let loading = Memo::new(move |_| {
        filter.get() != Filter::Selected && (data.loading.get() || searching.get())
    });
    let count = Memo::new(move |_| selected.with(Vec::len));
    let rows = Memo::new(move |_| {
        if filter.get() == Filter::Selected {
            selected.get()
        } else {
            data.items.get()
        }
    });
    let page_counts = Memo::new(move |_| {
        let mut available = 0;
        let mut checked = 0;
        rows.with(|notes| {
            selected.with(|picks| {
                for note in notes.iter().filter(|note| can_join(note)) {
                    available += 1;
                    if picks.iter().any(|pick| pick.id == note.id) {
                        checked += 1;
                    }
                }
            })
        });
        (available, checked)
    });

    let select = Callback::new(move |(note, checked): (NotebookCandidate, bool)| {
        if busy.get_untracked() || loading.get_untracked() {
            return;
        }
        selected.update(|picks| {
            if !checked {
                picks.retain(|pick| pick.id != note.id);
            } else if can_join(&note)
                && picks.len() < MAX_NOTE_BATCH_SIZE
                && !picks.iter().any(|pick| pick.id == note.id)
            {
                picks.push(note);
            }
        });
        add_error.set(None);
    });
    let select_page = move |checked| {
        let (available, already_selected) = page_counts.get_untracked();
        let can_select_page =
            available - already_selected <= MAX_NOTE_BATCH_SIZE - count.get_untracked();
        for note in rows.get_untracked().into_iter().filter(can_join) {
            // 混合状态下若剩余名额不足，清除本页勾选也仍可用。
            select.run((note, checked && can_select_page));
        }
    };

    let submit = move |_| {
        if busy.get_untracked() || count.get_untracked() == 0 {
            return;
        }
        add_error.set(None);
        busy.set(true);
        let note_ids = selected.with_untracked(|picks| picks.iter().map(|note| note.id).collect());
        spawn_local(async move {
            let result = add_notebook_notes(book_id, note_ids).await;
            if busy.try_set(false).is_some() {
                return;
            }
            match result {
                Ok(result) => {
                    close.try_run(());
                    on_added.try_run(result);
                }
                Err(e) => {
                    let _ = add_error.try_set(Some(failure_message("加入失败", &e)));
                }
            }
        });
    };
    let update_search = move |event| {
        let value = event_target_value(&event);
        search.set(value.clone());
        let Some(token) = search_generation.try_update_value(|token| {
            *token += 1;
            *token
        }) else {
            return;
        };
        spawn_local(async move {
            crate::utils::time::sleep_ms(300).await;
            if search_generation.try_get_value() == Some(token)
                && query.with_untracked(|q| *q != value.trim())
            {
                query.set(value.trim().to_string());
                page.set(1);
            }
        });
    };

    view! {
        <div class="asset-modal-head">
            <div>
                <h2>"添加笔记"</h2>
                <p class="admin-notebook-hint">{move || format!("添加到「{}」", book_title.get())}</p>
            </div>
            <IconButton icon=IconName::X label="关闭" size=ButtonSize::Sm disabled=busy onclick=move |_| close.run(())/>
        </div>
        <div class="asset-modal-bar notebook-picker-tools">
            <SegmentedControl
                items=vec![
                    SegmentedItem::new(Filter::All, "全部"),
                    SegmentedItem::new(Filter::Missing, "未加入"),
                    SegmentedItem::new(Filter::Selected, "已选"),
                ]
                value=filter
                on_change=move |next| {
                    if !busy.get_untracked() {
                        page.set(1);
                        filter.set(next);
                    }
                }
                aria_label="筛选笔记"
                kind=SegmentedKind::Radio
                size=SegmentedSize::Sm
                class="notebook-picker-filter"
            />
            <Show when=move || filter.get() != Filter::Selected>
                <label class="field field-sm admin-filter-search asset-modal-search">
                    <Icon name=IconName::Search size=14/>
                    <span class="sr-only">"搜索标题、正文或标签"</span>
                    <input
                        type="search"
                        maxlength="200"
                        placeholder="搜索标题、正文或标签"
                        prop:value=move || search.get()
                        disabled=move || busy.get()
                        on:input=update_search
                    />
                </label>
            </Show>
        </div>
        <div class="asset-modal-body" aria-busy=move || loading.get().to_string()>
            {move || {
                if loading.get() {
                    return view! { <p class="asset-modal-empty" role="status">"正在读取笔记…"</p> }.into_any();
                }
                if filter.get() != Filter::Selected && data.error.with(Option::is_some) {
                    return view! {
                        <LoadError
                            title="笔记加载失败"
                            on_retry=Callback::new(move |()| retry.update(|n| *n = n.wrapping_add(1)))
                        />
                    }.into_any();
                }
                view! {
                    <Show when=move || rows.with(Vec::is_empty)>
                        <p class="asset-modal-empty" role="status">
                            {move || match filter.get() {
                                Filter::Selected => "还没有勾选笔记。",
                                Filter::Missing if query.with(String::is_empty) => "没有未加入的笔记。",
                                Filter::All if query.with(String::is_empty) => "还没有笔记。",
                                _ => "没有符合条件的笔记。",
                            }}
                        </p>
                    </Show>
                    <ul class="notebook-picker-list" aria-label="笔记">
                        <For
                            each=move || rows.get()
                            key=|note| (note.id, note.title.clone(), note.kind.as_str(), note.published, note.joined, note.at_limit)
                            children=move |note| view! {
                                <PickerRow note selected count busy on_select=select/>
                            }
                        />
                    </ul>
                }.into_any()
            }}
        </div>
        <Show when=move || {
            filter.get() != Filter::Selected && data.error.with(Option::is_none) && data.total.get() > 0
        }>
            <div class="asset-modal-pager notebook-picker-pager">
                <label>
                    <Checkbox
                        checked=move || page_counts.with(|&(available, checked)| available > 0 && available == checked)
                        indeterminate=move || page_counts.with(|&(available, checked)| checked > 0 && checked < available)
                        disabled=move || busy.get() || loading.get() || page_counts.with(|&(available, checked)| {
                            available == 0 || (checked == 0 && available > MAX_NOTE_BATCH_SIZE - count.get())
                        })
                        onchange=select_page
                    />
                    "全选本页可加入的笔记"
                </label>
                <TablePager
                    page total=data.total per_page=NOTES_PER_PAGE unit="篇"
                    on_change=move |next| {
                        if !loading.get_untracked() && !busy.get_untracked() {
                            page.set(next);
                        }
                    }
                />
            </div>
        </Show>
        {move || add_error.get().map(|message| view! {
            <p class="notebook-picker-error" role="alert">{message}</p>
        })}
        <div class="asset-modal-foot">
            <div class="notebook-picker-selection">
                <p role="status">
                    {move || format!("已选 {} 篇", count.get())}
                    {move || {
                        let (available, checked) = page_counts.get();
                        (count.get() == MAX_NOTE_BATCH_SIZE
                            || available - checked > MAX_NOTE_BATCH_SIZE - count.get())
                            .then(|| format!("，本次最多 {MAX_NOTE_BATCH_SIZE} 篇"))
                    }}
                </p>
                <Show when=move || is_public.get()>
                    <p class="admin-notebook-hint">"访客可在目录中看到已发布的笔记。"</p>
                </Show>
            </div>
            <div class="asset-modal-foot-actions">
                <Button variant=ButtonVariant::Ghost disabled=busy onclick=move |_| close.run(())>"取消"</Button>
                <Button variant=ButtonVariant::Primary loading=busy disabled=move || count.get() == 0 onclick=submit>
                    "加入"
                </Button>
            </div>
        </div>
    }
}

#[component]
fn PickerRow(
    note: NotebookCandidate,
    selected: RwSignal<Vec<NotebookCandidate>>,
    count: Memo<usize>,
    busy: RwSignal<bool>,
    on_select: Callback<(NotebookCandidate, bool)>,
) -> impl IntoView {
    let id = note.id;
    let checked = Memo::new(move |_| selected.with(|picks| picks.iter().any(|note| note.id == id)));
    let available = can_join(&note);
    let status = if note.joined {
        "已加入"
    } else {
        "已达收录上限"
    };
    let checkbox_label = format!("选择笔记「{}」", note.title);
    let detail = format!(
        "{} · {}",
        note.kind.label(),
        if note.published {
            "已发布"
        } else {
            "私密"
        }
    );
    let title = note.title.clone();
    let on_change = move |value| on_select.run((note.clone(), value));
    let copy = view! {
        <span class="notebook-picker-row-copy">
            <span class="notebook-picker-row-title">{title}</span>
            <span class="admin-notebook-hint">{detail}</span>
        </span>
    };
    view! {
        <li>
            {if available {
                view! {
                    <label class="notebook-picker-row" class:is-selected=move || checked.get()>
                    <Checkbox
                        checked
                        disabled=move || busy.get() || (!checked.get() && count.get() == MAX_NOTE_BATCH_SIZE)
                        label=checkbox_label
                        onchange=on_change
                    />
                        {copy}
                    </label>
                }.into_any()
            } else {
                view! {
                    <div class="notebook-picker-row">
                        {copy}
                        <span class="admin-notebook-hint">{status}</span>
                    </div>
                }.into_any()
            }}
        </li>
    }
}
