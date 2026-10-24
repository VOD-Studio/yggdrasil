//! 笔记管理（`/admin/notes`）。
//!
//! 标题行（管理笔记本、写笔记）→ 快速记录 Composer（正文与标签，存为私密随记）→
//! 类型选项卡、搜索、公开状态 / 知识库 / 笔记本筛选与回收站开关 → 笔记行（类型字、
//! 标题、公开版本、未发布修改、知识库版本、笔记本与标签、最近修改时间）与底栏分页。
//! 回收站里的行改为「恢复为草稿」。筛选条件按历史记录保存。私人数据只在浏览器
//! 鉴权后读取（Effect 内请求，SSR 不执行）。样式见 styles/pages/admin-notes.css。
use chrono::{DateTime, Datelike, FixedOffset};
use leptos::ev::{KeyboardEvent, SubmitEvent};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{lazy_route, LazyRoute};

use super::note_drafts::{clear_submitted, use_draft_protection};
use crate::api::notes::*;
use crate::components::admin_layout::AdminPageHead;
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::forms::{FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::skeletons::notes_admin_skeleton::NoteRowsSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Composer, ComposerInput, Icon, IconName,
    LoadError, SegmentedControl, SegmentedItem, StatusDot, TablePager,
};
use crate::models::note::*;
use crate::routes::Route;
use crate::utils::error::failure_message;
use crate::utils::time::to_local;

pub mod notebooks;

/// 列表接口每页固定 20 条。
const NOTES_PER_PAGE: i32 = 20;
/// 输入停顿多久后自动搜索（毫秒）。
const SEARCH_DEBOUNCE_MS: u32 = 300;

/// `/admin/notes`。数据全部在浏览器鉴权后读取，没有 SSR 数据。
pub struct AdminNotesRoute;

#[lazy_route]
impl LazyRoute for AdminNotesRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <AdminNotes/> }.into_any()
    }
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct NotesHistoryState {
    filter: NoteFilter,
    search_input: String,
}

/// 「标签：Rust, 排错」→ 去空白、去空项；中英文逗号都算分隔。
pub(super) fn parse_tags(input: &str) -> Vec<String> {
    input
        .split([',', '，'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// 列表里的修改时间：当天只写时刻，今年写月日，更早写完整日期。
fn short_when(at: DateTime<FixedOffset>, now: DateTime<FixedOffset>) -> String {
    if at.date_naive() == now.date_naive() {
        at.format("%H:%M").to_string()
    } else if at.year() == now.year() {
        at.format("%m-%d").to_string()
    } else {
        at.format("%Y-%m-%d").to_string()
    }
}

fn kind_value(kind: Option<NoteKind>) -> String {
    kind.map_or("all", NoteKind::as_str).to_string()
}

/// 笔记管理页。
#[component]
fn AdminNotes() -> impl IntoView {
    let entry_id = crate::bridges::navigation::entry_id();
    let saved = crate::bridges::navigation::read_state::<NotesHistoryState>("admin-notes")
        .unwrap_or_default();
    let toast = use_toast();
    let filter = RwSignal::new(NoteFilter {
        page: saved.filter.page.max(1),
        ..saved.filter.clone()
    });
    let search_input = RwSignal::new(saved.search_input.clone());
    let search_seq = RwSignal::new(0_u64);
    let restoring = RwSignal::new(None::<i32>);
    // 列表与笔记本的镜像信号：刷新时保留上一份数据直到新结果到达。
    let response = RwSignal::new(None::<Result<NotePage, ()>>);
    let books = RwSignal::new(None::<Result<Vec<Notebook>, ()>>);
    let reload = RwSignal::new(0_u32);
    let generation = StoredValue::new(0_u64);

    Effect::new(move |_| {
        reload.track();
        let current = filter.get();
        let Some(id) = generation.try_update_value(|g| {
            *g += 1;
            *g
        }) else {
            return;
        };
        spawn_local(async move {
            let result = list_owned_notes(current).await.map_err(|_| ());
            if generation.try_get_value() == Some(id) {
                let _ = response.try_set(Some(result));
            }
        });
    });
    Effect::new(move |_| {
        spawn_local(async move {
            let result = owned_notebooks().await.map_err(|_| ());
            let _ = books.try_set(Some(result));
        });
    });

    Effect::new(move |_| {
        crate::bridges::navigation::write_state(
            &entry_id,
            "admin-notes",
            &NotesHistoryState {
                filter: filter.get(),
                search_input: search_input.get(),
            },
        );
    });

    let submit_search = move || {
        let q = search_input.get_untracked().trim().to_string();
        if q != filter.with_untracked(|f| f.query.clone()) {
            filter.update(|f| {
                f.query = q;
                f.page = 1;
            });
        }
    };
    let schedule_search = move || {
        search_seq.update(|s| *s += 1);
        let id = search_seq.get_untracked();
        spawn_local(async move {
            crate::utils::time::sleep_ms(SEARCH_DEBOUNCE_MS).await;
            if search_seq.try_get_untracked() == Some(id) {
                submit_search();
            }
        });
    };
    let restore = Callback::new(move |note: Note| {
        restoring.set(Some(note.id));
        spawn_local(async move {
            match act_on_note(note.id, note.version, NoteAction::Restore).await {
                Ok(_) => {
                    toast.success(format!("已把「{}」恢复为草稿", note.display_title()));
                    let _ = reload.try_update(|n| *n = n.wrapping_add(1));
                }
                Err(e) => toast.error(failure_message("恢复失败", &e)),
            }
            let _ = restoring.try_set(None);
        });
    });

    let trash = Memo::new(move |_| filter.with(|f| f.trash));
    let filtering = Memo::new(move |_| {
        filter.with(|f| {
            !f.query.is_empty()
                || f.published.is_some()
                || f.knowledge.is_some()
                || f.notebook_id.is_some()
        })
    });
    let book_list = Memo::new(move |_| match books.get() {
        Some(Ok(items)) => items,
        _ => Vec::new(),
    });
    let book_options = Memo::new(move |_| {
        let mut options: Vec<(Option<i32>, String)> = vec![(None, "笔记本".to_string())];
        options.extend(
            book_list
                .get()
                .iter()
                .map(|b| (Some(b.id), b.title.clone())),
        );
        options
    });
    let retry = Callback::new(move |()| {
        reload.update(|n| *n = n.wrapping_add(1));
    });

    view! {
        <div class="admin-notes">
            <AdminPageHead
                title="笔记"
                actions=|| {
                    view! {
                        <a
                            class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                            href=Route::AdminNotebooks.to_string()
                        >
                            "管理笔记本"
                        </a>
                        <a
                            class=format!(
                                "{} admin-hide-mobile",
                                button_class(ButtonVariant::Primary, ButtonSize::Md),
                            )
                            href=Route::NewNote.to_string()
                        >
                            <Icon name=IconName::Plus size=16/>
                            "写笔记"
                        </a>
                    }
                }
            >
                "捕捉此刻的想法，也为长久的知识留一个位置。"
            </AdminPageHead>

            <QuickNote on_saved=retry/>

            <div class="admin-toolbar admin-notes-toolbar">
                {move || {
                    if trash.get() {
                        view! {
                            <p class="admin-notes-trash-title">
                                <Icon name=IconName::Trash size=15/>
                                "回收站"
                                <span>"恢复后为私密草稿，公开与收录需要重新确认。"</span>
                            </p>
                        }
                            .into_any()
                    } else {
                        view! {
                            <SegmentedControl
                                aria_label="笔记类型"
                                items=vec![
                                    SegmentedItem::new("all".to_string(), "全部"),
                                    SegmentedItem::new("moment".to_string(), "随记"),
                                    SegmentedItem::new("topic".to_string(), "主题笔记"),
                                ]
                                value=Signal::derive(move || kind_value(filter.with(|f| f.kind)))
                                on_change=Callback::new(move |value: String| {
                                    filter
                                        .update(|f| {
                                            f.kind = match value.as_str() {
                                                "moment" => Some(NoteKind::Moment),
                                                "topic" => Some(NoteKind::Topic),
                                                _ => None,
                                            };
                                            f.page = 1;
                                        })
                                })
                            />
                        }
                            .into_any()
                    }
                }}
                <div class="admin-toolbar-filters">
                    <label class="field field-sm admin-filter-search">
                        <Icon name=IconName::Search size=14/>
                        <span class="sr-only">"搜索标题、正文或标签"</span>
                        <input
                            type="search"
                            placeholder="搜索标题、正文或标签"
                            value=search_input.get_untracked()
                            prop:value=move || search_input.get()
                            on:input=move |e| {
                                search_input.set(event_target_value(&e));
                                schedule_search();
                            }
                            on:keydown=move |e: KeyboardEvent| {
                                if e.key() == "Enter" && !e.is_composing() {
                                    search_seq.update(|s| *s += 1);
                                    submit_search();
                                } else if e.key() == "Escape" && !search_input.get_untracked().is_empty() {
                                    e.prevent_default();
                                    search_input.set(String::new());
                                    search_seq.update(|s| *s += 1);
                                    submit_search();
                                }
                            }
                        />
                    </label>
                    {move || {
                        (!trash.get())
                            .then(|| {
                                view! {
                                    <FormSelect
                                        id="admin-notes-published"
                                        value=Signal::derive(move || filter.with(|f| f.published))
                                        options=vec![
                                            (None, "公开状态"),
                                            (Some(true), "已公开"),
                                            (Some(false), "私密"),
                                        ]
                                        trigger_class=FORM_SELECT_COMPACT_CLASS
                                        aria_label="按公开状态筛选"
                                        onchange=Callback::new(move |value: Option<bool>| {
                                            filter
                                                .update(|f| {
                                                    f.published = value;
                                                    f.page = 1;
                                                })
                                        })
                                    />
                                    <FormSelect
                                        id="admin-notes-knowledge"
                                        value=Signal::derive(move || filter.with(|f| f.knowledge))
                                        options=vec![
                                            (None, "AI 知识库"),
                                            (Some(true), "已收录"),
                                            (Some(false), "未收录"),
                                        ]
                                        trigger_class=FORM_SELECT_COMPACT_CLASS
                                        aria_label="按知识库收录筛选"
                                        onchange=Callback::new(move |value: Option<bool>| {
                                            filter
                                                .update(|f| {
                                                    f.knowledge = value;
                                                    f.page = 1;
                                                })
                                        })
                                    />
                                    {move || {
                                        (book_options.with(|options| options.len()) > 1)
                                            .then(|| {
                                                view! {
                                                    <FormSelect
                                                        id="admin-notes-notebook"
                                                        value=Signal::derive(move || {
                                                            filter.with(|f| f.notebook_id)
                                                        })
                                                        options=book_options
                                                        trigger_class=FORM_SELECT_COMPACT_CLASS
                                                        aria_label="按笔记本筛选"
                                                        onchange=Callback::new(move |value: Option<i32>| {
                                                            filter
                                                                .update(|f| {
                                                                    f.notebook_id = value;
                                                                    f.page = 1;
                                                                })
                                                        })
                                                    />
                                                }
                                            })
                                    }}
                                }
                            })
                    }}
                    <button
                        class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                        type="button"
                        aria-pressed=move || trash.get().to_string()
                        on:click=move |_| {
                            filter
                                .update(|f| {
                                    f.trash = !f.trash;
                                    f.page = 1;
                                })
                        }
                    >
                        {move || {
                            if trash.get() {
                                view! {
                                    <Icon name=IconName::ArrowLeft size=14/>
                                    "返回笔记"
                                }
                                    .into_any()
                            } else {
                                "回收站".into_any()
                            }
                        }}
                    </button>
                </div>
            </div>

            {move || match response.get() {
                None => view! { <NoteRowsSkeleton/> }.into_any(),
                Some(Err(())) => {
                    view! { <LoadError title="笔记列表加载失败" on_retry=retry/> }.into_any()
                }
                Some(Ok(page)) if page.notes.is_empty() => {
                    view! {
                        <div class="admin-panel">
                            {move || {
                                if filtering.get() {
                                    view! {
                                        <EmptyState
                                            icon=IconName::SearchX
                                            title="没有符合条件的笔记"
                                            description="换个关键词，或清除筛选再试。"
                                            action=EmptyStateAction::new(
                                                "清除筛选",
                                                move |_| {
                                                    search_input.set(String::new());
                                                    filter
                                                        .update(|f| {
                                                            *f = NoteFilter {
                                                                kind: f.kind,
                                                                trash: f.trash,
                                                                ..Default::default()
                                                            };
                                                        });
                                                },
                                            )
                                        />
                                    }
                                        .into_any()
                                } else if trash.get() {
                                    view! {
                                        <EmptyState
                                            icon=IconName::Trash
                                            title="回收站是空的"
                                            description="移入回收站的笔记会出现在这里，可以随时恢复。"
                                        />
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <EmptyState
                                            icon=IconName::NotebookPen
                                            title="还没有笔记"
                                            description="从上面的快速记录开始，或者写一篇主题笔记。"
                                            action=EmptyStateAction::new(
                                                "写笔记",
                                                move |_| {
                                                    crate::navigation::push(&Route::NewNote.to_string());
                                                },
                                            )
                                        />
                                    }
                                        .into_any()
                                }
                            }}
                        </div>
                    }
                        .into_any()
                }
                Some(Ok(page)) => {
                    let now = crate::utils::time::local_now();
                    let book_list = book_list.get();
                    let total = page.total;
                    let rows = page
                        .notes
                        .into_iter()
                        .map(|note| {
                            let id = note.id;
                            let when = short_when(to_local(note.updated_at), now);
                            let names = note
                                .notebook_ids
                                .iter()
                                .filter_map(|id| book_list.iter().find(|b| b.id == *id))
                                .map(|b| b.title.clone())
                                .collect::<Vec<_>>();
                            view! {
                                <NoteRow
                                    note=note
                                    when=when
                                    books=names
                                    restoring=Signal::derive(move || restoring.get() == Some(id))
                                    on_restore=restore
                                />
                            }
                        })
                        .collect_view();
                    view! {
                        <div class="admin-panel admin-notes-list">
                            <ul>{rows}</ul>
                            <TablePager
                                page=Signal::derive(move || filter.with(|f| f.page.max(1)))
                                total=total
                                per_page=NOTES_PER_PAGE
                                unit="条"
                                on_change=Callback::new(move |page: i32| {
                                    filter.update(|f| f.page = page)
                                })
                            />
                        </div>
                    }
                        .into_any()
                }
            }}

            <a class="admin-fab" href=Route::NewNote.to_string() aria-label="写笔记">
                <Icon name=IconName::Plus size=18/>
                "写笔记"
            </a>
        </div>
    }
}

/// 快速记录：正文与标签存为私密随记。正文未变时进入编辑器，保存期间继续输入则留在本页。
#[component]
fn QuickNote(on_saved: Callback<()>) -> impl IntoView {
    let toast = use_toast();
    let quick = RwSignal::new(String::new());
    let tags = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let last_saved = RwSignal::new(None::<i32>);
    let (recovery, cache_failed) = use_draft_protection("quick".into(), quick, move || {
        !quick.get().is_empty() || busy.get()
    });
    let recovering = Memo::new(move |_| recovery.with(Option::is_some));
    let empty = Memo::new(move |_| quick.with(|q| q.trim().is_empty()));

    let save = move || {
        if busy.get_untracked() || recovering.get_untracked() || empty.get_untracked() {
            return;
        }
        busy.set(true);
        let content_md = quick.get_untracked();
        let tag_list = parse_tags(&tags.get_untracked());
        spawn_local(async move {
            let result = save_note(NoteDraft {
                content_md: content_md.clone(),
                tags: tag_list,
                ..Default::default()
            })
            .await;
            if busy.try_set(false).is_some() {
                // 组件已卸载，丢弃迟到的结果。
                return;
            }
            match result {
                Ok(note) => {
                    let _ = tags.try_set(String::new());
                    let Some(unchanged) =
                        quick.try_update(|current| clear_submitted(current, &content_md))
                    else {
                        return;
                    };
                    if unchanged {
                        crate::navigation::push(&Route::EditNote { id: note.id }.to_string());
                    } else {
                        let _ = last_saved.try_set(Some(note.id));
                        on_saved.run(());
                    }
                }
                Err(e) => toast.error(failure_message("保存失败", &e)),
            }
        });
    };

    view! {
        {move || {
            recovery
                .get()
                .map(|local| {
                    view! {
                        <div class="admin-notes-banner" role="alert">
                            <Icon name=IconName::RotateCcwClock size=16/>
                            <p>"这个标签页有尚未保存的快速记录。"</p>
                            <button
                                class=button_class(ButtonVariant::Secondary, ButtonSize::Sm)
                                type="button"
                                on:click=move |_| {
                                    quick.set(local.clone());
                                    recovery.set(None);
                                }
                            >
                                "恢复记录"
                            </button>
                            <button
                                class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                type="button"
                                on:click=move |_| recovery.set(None)
                            >
                                "放弃"
                            </button>
                        </div>
                    }
                })
        }}
        <form on:submit=move |ev: SubmitEvent| {
            ev.prevent_default();
            save();
        }>
            <Composer stacked=true class="admin-notes-quick">
                <ComposerInput
                    multiline=true
                    aria_label="快速记录"
                    disabled=recovering
                    placeholder="有什么想记下的？一句话、一段代码，或一个刚发现的链接。"
                    value=quick
                    oninput=Callback::new(move |value: String| quick.set(value))
                    on_submit=Callback::new(move |()| save())
                />
                <div class="admin-notes-quick-foot">
                    <input
                        class="admin-notes-quick-tags"
                        aria-label="随记标签"
                        placeholder="标签：Rust, 排错, 读书"
                        disabled=move || recovering.get()
                        prop:value=move || tags.get()
                        on:input=move |ev| tags.set(event_target_value(&ev))
                    />
                    <span class="admin-notes-quick-hint">"先存为私密草稿，稍后整理。"</span>
                    <button
                        class=button_class(ButtonVariant::Primary, ButtonSize::Sm)
                        type="submit"
                        disabled=move || busy.get() || recovering.get() || empty.get()
                    >
                        {move || if busy.get() { "保存中…" } else { "记下" }}
                    </button>
                </div>
            </Composer>
        </form>
        {move || {
            cache_failed
                .get()
                .then(|| {
                    view! {
                        <p class="admin-notes-note" role="alert">
                            "浏览器无法保存本地恢复副本，请先保存到服务端再离开。"
                        </p>
                    }
                })
        }}
        {move || {
            last_saved
                .get()
                .map(|id| {
                    view! {
                        <p class="admin-notes-note" role="status">
                            "上一条已保存，继续输入的内容已保留。"
                            <a href=Route::EditNote { id }.to_string()>"查看已保存记录"</a>
                        </p>
                    }
                })
        }}
    }
}

/// 笔记的一行。回收站里的行不进入编辑器，右侧给出「恢复为草稿」。
#[component]
fn NoteRow(
    note: Note,
    when: String,
    books: Vec<String>,
    restoring: Signal<bool>,
    on_restore: Callback<Note>,
) -> impl IntoView {
    let glyph = match note.kind {
        NoteKind::Topic => "题",
        NoteKind::Moment => "记",
    };
    let title = note.display_title();
    let ahead = note.published_version.is_some_and(|v| note.version > v);
    let tags = note
        .tags
        .iter()
        .map(|t| format!("#{t}"))
        .collect::<Vec<_>>()
        .join(" ");
    let published = match note.published_version {
        Some(v) => view! { <StatusDot label=format!("公开 v{v}") tone="clay"/> }.into_any(),
        None => view! { <StatusDot label="私密" class="admin-notes-private"/> }.into_any(),
    };
    let knowledge = note
        .knowledge_version
        .map(|v| view! { <span class="admin-notes-pill">{format!("知识库 v{v}")}</span> });
    let books = books
        .into_iter()
        .map(|book| view! { <span>{format!("《{book}》")}</span> })
        .collect_view();
    let meta = view! {
        <span class="admin-notes-meta">
            {published} {ahead.then(|| view! { <span class="admin-notes-ahead">"有未发布修改"</span> })}
            {knowledge} {books} {(!tags.is_empty()).then(|| view! { <span>{tags}</span> })}
        </span>
    };
    if let Some(deleted_at) = note.deleted_at {
        let deleted = to_local(deleted_at).format("%Y-%m-%d").to_string();
        return view! {
            <li class="admin-notes-row" aria-busy=move || restoring.get().then_some("true")>
                <span class="admin-notes-glyph" aria-hidden="true">
                    {glyph}
                </span>
                <span class="admin-notes-main">
                    <span class="admin-notes-title">{title}</span>
                    {meta}
                </span>
                <span class="admin-notes-side">
                    <time>{format!("删除于 {deleted}")}</time>
                    <button
                        class=button_class(ButtonVariant::Secondary, ButtonSize::Sm)
                        type="button"
                        disabled=move || restoring.get()
                        on:click=move |_| on_restore.run(note.clone())
                    >
                        "恢复为草稿"
                    </button>
                </span>
            </li>
        }
        .into_any();
    }
    view! {
        <li>
            <a class="admin-notes-row" href=Route::EditNote { id: note.id }.to_string()>
                <span class="admin-notes-glyph" aria-hidden="true">
                    {glyph}
                </span>
                <span class="admin-notes-main">
                    <span class="admin-notes-title">{title}</span>
                    {meta}
                </span>
                <span class="admin-notes-side">
                    <time>{when}</time>
                    <span class="admin-notes-edit" aria-hidden="true">
                        "编辑"
                        <Icon name=IconName::ArrowRight size=13/>
                    </span>
                </span>
            </a>
        </li>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(8 * 3600)
            .unwrap()
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .unwrap()
    }

    #[test]
    fn short_when_shortens_recent_dates() {
        let now = at(2026, 10, 21, 16, 0);
        assert_eq!(short_when(at(2026, 10, 21, 9, 5), now), "09:05");
        assert_eq!(short_when(at(2026, 10, 12, 9, 5), now), "10-12");
        assert_eq!(short_when(at(2025, 12, 31, 9, 5), now), "2025-12-31");
    }

    #[test]
    fn parse_tags_accepts_both_commas_and_drops_blanks() {
        assert_eq!(parse_tags(" Rust，排错, ,读书 "), ["Rust", "排错", "读书"]);
        assert!(parse_tags("  ").is_empty());
    }
}
