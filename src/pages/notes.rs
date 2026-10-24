//! 笔记：列表 `/notes`、笔记本目录 `/notes/book/:id` 与阅读页 `/notes/:slug?book=`。
//!
//! 列表与阅读页共用 880px 容器内的「200px 左栏 + 内容」布局：列表页左栏是筛选，
//! 阅读页左栏是所在笔记本的目录，进入阅读时布局不跳。
//!
//! 数据加载：三个路由在 `LazyRoute::data()` 里建 `Resource` 取 SSR 数据，`view()` 在
//! `Suspense` 内消费。会刷新的数据（筛选切换、加载更多、重试）一律走
//! `Effect` / 事件处理里的 `spawn_local` 写镜像信号，不再触碰 `Resource`，
//! 否则外层 `Suspense` 会把整页退回骨架屏。

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::hooks::{use_params_map, use_query_map};
use leptos_router::{lazy_route, LazyRoute};

use crate::api::notes::{get_note, get_note_context, list_notebooks, list_notes, note_stats};
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::post::post_content::PostContent;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::notes_route_skeleton::{
    NoteReaderSkeleton, NotebookSkeleton, NotesPageSkeleton,
};
use crate::components::ui::{
    use_sheet_close, BottomSheet, Button, ButtonVariant, Icon, IconName, LoadError,
};
use crate::models::note::*;
use crate::models::public_note::PublicNote;
use crate::pages::not_found::{NotFound, PageLoadFailed};
use crate::routes::Route;
use crate::utils::error::is_not_found;
use crate::utils::time::sleep_ms;

/// 列表一页的条数，与服务端 `store::list` 一致。
const PAGE_SIZE: i64 = 20;
/// 筛选框输入停顿多久后查询。
const FILTER_DEBOUNCE_MS: u32 = 300;
/// 左栏常用标签数。
const SIDE_TAGS: usize = 8;

/// 笔记类型筛选：全部、随记、主题笔记。
const KINDS: [(Option<NoteKind>, &str); 3] = [
    (None, "全部"),
    (Some(NoteKind::Moment), "随记"),
    (Some(NoteKind::Topic), "主题笔记"),
];

fn note_time(note: &Note) -> DateTime<Utc> {
    note.updated_at
}

/// 阅读页地址；有笔记本上下文时带上 `?book=`。
fn note_route(slug: &str, book: Option<i32>) -> Route {
    Route::NoteDetail {
        slug: slug.to_string(),
        book: book.map(|id| id.to_string()).unwrap_or_default(),
    }
}

/// 按天分组的笔记流。列表已按时间降序；相邻同一天的笔记归为一组。
#[derive(Clone, Debug, PartialEq)]
struct DayGroup {
    date: NaiveDate,
    notes: Vec<Note>,
}

impl DayGroup {
    /// 分组的渲染键：日期加上每条笔记的 `(id, version)`，内容变化时才重建该组。
    fn key(&self) -> (NaiveDate, Vec<(i32, i32)>) {
        (
            self.date,
            self.notes.iter().map(|n| (n.id, n.version)).collect(),
        )
    }
}

fn group_by_day(notes: &[Note]) -> Vec<DayGroup> {
    let mut groups: Vec<DayGroup> = Vec::new();
    for note in notes {
        let date = note_time(note).date_naive();
        match groups.last_mut() {
            Some(group) if group.date == date => group.notes.push(note.clone()),
            _ => groups.push(DayGroup {
                date,
                notes: vec![note.clone()],
            }),
        }
    }
    groups
}

/// 组标题旁的相对日期：今天、昨天、一周内的星期几；跨年时写年份。
fn relative_day(date: NaiveDate, today: NaiveDate) -> String {
    const WEEKDAYS: [&str; 7] = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
    match (today - date).num_days() {
        0 => "今天".to_string(),
        1 => "昨天".to_string(),
        2..=6 => WEEKDAYS[date.weekday().num_days_from_monday() as usize].to_string(),
        _ if date.year() != today.year() => date.year().to_string(),
        _ => String::new(),
    }
}

/// 今天的日期只在浏览器挂载后取得：服务端渲染不写相对日期，避免水合不一致与缓存过期。
fn use_today() -> RwSignal<Option<NaiveDate>> {
    let today = RwSignal::new(None::<NaiveDate>);
    Effect::new(move |_| {
        let now = DateTime::<Utc>::from_timestamp_millis(crate::utils::time::now_millis());
        today.set(now.map(|t| t.date_naive()));
    });
    today
}

fn notebook_title(books: &[Notebook], note: &Note) -> Option<(i32, String)> {
    note.notebook_ids
        .iter()
        .find_map(|id| books.iter().find(|b| b.id == *id))
        .map(|b| (b.id, b.title.clone()))
}

/// 列表页首屏随 SSR 预取的筛选：第一页、无筛选条件。
fn first_filter() -> NoteFilter {
    NoteFilter {
        page: 1,
        ..Default::default()
    }
}

// ======================= 列表页 =======================

/// 笔记列表路由，对应 `/notes`。
pub struct NotesRoute {
    books: Resource<Result<Vec<Notebook>, ServerFnError>>,
    stats: Resource<Result<NoteStats, ServerFnError>>,
    first: Resource<Result<NotePage, ServerFnError>>,
}

#[lazy_route]
impl LazyRoute for NotesRoute {
    fn data() -> Self {
        Self {
            books: Resource::new(|| (), |_| list_notebooks()),
            stats: Resource::new(|| (), |_| note_stats()),
            first: Resource::new(|| (), |_| list_notes(first_filter())),
        }
    }

    fn view(this: Self) -> AnyView {
        let NotesRoute {
            books,
            stats,
            first,
        } = this;
        view! {
            <Title text="笔记 · Yggdrasil"/>
            <div class="notes">
                <Suspense fallback=|| {
                    view! {
                        <DelayedSkeleton>
                            <NotesPageSkeleton/>
                        </DelayedSkeleton>
                    }
                }>
                    {move || Suspend::new(async move {
                        // 目录与概况只影响左栏，失败时按空处理；列表失败由列表自己提示重试。
                        let books = books.await.unwrap_or_default();
                        let stats = stats.await.ok();
                        let first = first.await;
                        view! { <NotesBody books stats first/> }
                    })}
                </Suspense>
            </div>
        }
        .into_any()
    }
}

/// 当前显示的笔记列表及其筛选；与请求中的筛选不同时表示新结果仍在加载。
#[derive(Clone)]
struct ShownNotes {
    filter: NoteFilter,
    result: Result<NotePage, ServerFnError>,
}

/// 首屏随 SSR 预取挂载；之后的筛选切换在后台请求，结果返回前继续显示当前列表。
/// 不能重新触发 `Resource`：外层 `Suspense` 会再次挂起回骨架屏，页面高度坍缩并带动
/// 滚动位置跳动。回到首屏筛选直接复用预取结果。
///
/// 返回当前显示的列表、重试回调与「重试中」标志。
fn use_note_page(
    filter: RwSignal<NoteFilter>,
    first: Result<NotePage, ServerFnError>,
) -> (RwSignal<ShownNotes>, Callback<()>, RwSignal<bool>) {
    let first_filter = first_filter();
    let shown = RwSignal::new(ShownNotes {
        filter: first_filter.clone(),
        result: first.clone(),
    });
    let retry = RwSignal::new(0_u32);
    let retrying = RwSignal::new(false);
    // 世代号：任何一次筛选变化或重试都会作废更早发出的请求。
    let generation = StoredValue::new(0_u64);

    Effect::new(move |previous: Option<u32>| {
        let wanted = filter.get();
        let nonce = retry.get();
        let retried = previous.is_some_and(|p| p != nonce);
        let id = generation.try_update_value(|g| {
            *g += 1;
            *g
        });
        let Some(id) = id else { return nonce };

        if !retried {
            if shown.with_untracked(|s| s.filter == wanted) {
                return nonce;
            }
            // 回到挂载时的筛选：预取结果可用就直接复用。
            if wanted == first_filter && first.is_ok() {
                shown.set(ShownNotes {
                    filter: first_filter.clone(),
                    result: first.clone(),
                });
                return nonce;
            }
        }
        if retried {
            retrying.set(true);
        }
        spawn_local(async move {
            let result = list_notes(wanted.clone()).await;
            // 之后又有新的筛选或重试：这份结果作废。
            if generation.try_get_value() != Some(id) {
                return;
            }
            let _ = shown.try_set(ShownNotes {
                filter: wanted,
                result,
            });
            let _ = retrying.try_set(false);
        });
        nonce
    });

    let on_retry = Callback::new(move |()| retry.update(|n| *n += 1));
    (shown, on_retry, retrying)
}

/// 「更早的记录」追加的页：属于哪个筛选、已加载到第几页。筛选变化后自然作废。
#[derive(Clone, Default, PartialEq)]
struct OlderPages {
    filter: NoteFilter,
    last_page: i32,
    notes: Vec<Note>,
}

#[component]
fn NotesBody(
    books: Vec<Notebook>,
    stats: Option<NoteStats>,
    first: Result<NotePage, ServerFnError>,
) -> impl IntoView {
    let entry_id = crate::bridges::navigation::entry_id();
    // 首帧与服务端一致用默认筛选；历史记录里保存的筛选在挂载后恢复。
    let filter = RwSignal::new(first_filter());
    // 恢复完成前不写回，避免用默认值覆盖要恢复的记录。
    let restored = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(mut saved) = crate::bridges::navigation::read_state::<NoteFilter>("notes") {
            saved.page = 1;
            if filter.with_untracked(|f| *f != saved) {
                filter.set(saved);
            }
        }
        restored.set(true);
    });
    Effect::new(move |_| {
        if restored.get() {
            crate::bridges::navigation::write_state(&entry_id, "notes", &filter.get());
        }
    });
    let (shown, on_retry, retrying) = use_note_page(filter, first);

    let total_text = stats
        .as_ref()
        .map(|stats| format!("{} 条 · {} 个笔记本", stats.total(), books.len()));

    view! {
        <header class="notes-header page-enter">
            <div>
                <h1 class="notes-title">"笔记"</h1>
                <p class="notes-intro">"随手记下的发现，和反复琢磨后成形的主题。"</p>
            </div>
            {total_text.map(|text| view! { <p class="notes-count tabular-nums">{text}</p> })}
        </header>
        <div class="notes-layout">
            <NotesSide filter books=books.clone() stats/>
            <NoteStream filter shown books on_retry retrying/>
        </div>
    }
}

/// 左栏：筛选框、类型、笔记本与常用标签。窄屏只留筛选框与横排类型。
#[component]
fn NotesSide(
    filter: RwSignal<NoteFilter>,
    books: Vec<Notebook>,
    stats: Option<NoteStats>,
) -> impl IntoView {
    let query = RwSignal::new(filter.get_untracked().query);
    let composing = RwSignal::new(false);
    let request = RwSignal::new(0_u64);
    // 筛选被外部改写（恢复历史状态、清除筛选）时，输入框跟着变。
    Effect::new(move |_| {
        let current = filter.with(|f| f.query.clone());
        if query.with_untracked(|q| q.trim() != current) {
            query.set(current);
        }
    });
    let apply_query = move || {
        request.update(|r| *r += 1);
        let id = request.get_untracked();
        if composing.get_untracked() {
            return;
        }
        spawn_local(async move {
            sleep_ms(FILTER_DEBOUNCE_MS).await;
            if request.try_get_untracked() != Some(id) {
                return;
            }
            let q = query.get_untracked().trim().to_string();
            if filter.with_untracked(|f| f.query != q) {
                filter.update(|f| f.query = q);
            }
        });
    };

    let count = |kind: Option<NoteKind>| {
        stats.as_ref().map(|s| match kind {
            None => s.total(),
            Some(NoteKind::Moment) => s.moments,
            Some(NoteKind::Topic) => s.topics,
        })
    };
    let kinds = KINDS
        .into_iter()
        .map(|(kind, label)| {
            let count = count(kind);
            view! {
                <button
                    type="button"
                    class="notes-side-item"
                    aria-pressed=move || filter.with(|f| f.kind == kind).to_string()
                    on:click=move |_| {
                        if filter.with_untracked(|f| f.kind != kind) {
                            filter.update(|f| f.kind = kind);
                        }
                    }
                >
                    <span>{label}</span>
                    {count
                        .map(|n| {
                            view! { <span class="notes-side-count tabular-nums">{n}</span> }
                        })}
                </button>
            }
        })
        .collect_view();
    let tags = stats
        .as_ref()
        .map(|s| s.tags.iter().take(SIDE_TAGS).cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    let books_group = (!books.is_empty()).then(|| {
        view! {
            <nav class="notes-side-group notes-side-extra" aria-label="笔记本">
                <p class="notes-side-label">"笔记本"</p>
                {books
                    .into_iter()
                    .map(|book| {
                        view! {
                            <a
                                class="notes-side-item"
                                href=Route::NotebookDetail { id: book.id }.to_string()
                            >
                                <Icon name=IconName::BookOpen size=15 class="notes-side-icon"/>
                                <span class="notes-side-text">{book.title}</span>
                                <span class="notes-side-count tabular-nums">{book.note_count}</span>
                            </a>
                        }
                    })
                    .collect_view()}
            </nav>
        }
    });
    let tags_group = (!tags.is_empty()).then(|| {
        view! {
            <div class="notes-side-group notes-side-extra">
                <p class="notes-side-label" id="notes-tags-label">
                    "标签"
                </p>
                <div class="notes-side-tags" role="group" aria-labelledby="notes-tags-label">
                    {tags
                        .into_iter()
                        .map(|(tag, n)| {
                            let pressed_tag = tag.clone();
                            let click_tag = tag.clone();
                            view! {
                                <button
                                    type="button"
                                    class="chip"
                                    aria-pressed=move || {
                                        filter.with(|f| f.tag == pressed_tag).to_string()
                                    }
                                    title=format!("{n} 条")
                                    on:click=move |_| {
                                        let next = if filter.with_untracked(|f| f.tag == click_tag) {
                                            String::new()
                                        } else {
                                            click_tag.clone()
                                        };
                                        filter.update(|f| f.tag = next);
                                    }
                                >
                                    {tag}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
        }
    });

    view! {
        <aside class="notes-side page-enter" style="--enter-index: 1" aria-label="笔记筛选">
            <form
                class="notes-filter"
                role="search"
                on:submit=move |event| {
                    event.prevent_default();
                    request.update(|r| *r += 1);
                    let q = query.get_untracked().trim().to_string();
                    filter.update(|f| f.query = q);
                }
            >
                <Icon name=IconName::Search size=14/>
                <label class="sr-only" for="notes-filter-input">
                    "筛选笔记"
                </label>
                <input
                    id="notes-filter-input"
                    type="search"
                    placeholder="筛选笔记"
                    autocomplete="off"
                    maxlength="200"
                    value=query.get_untracked()
                    prop:value=move || query.get()
                    on:input=move |event| {
                        query.set(event_target_value(&event));
                        apply_query();
                    }
                    on:compositionstart=move |_| composing.set(true)
                    on:compositionend=move |_| {
                        composing.set(false);
                        apply_query();
                    }
                />
            </form>
            <nav class="notes-side-group" aria-label="笔记类型">
                <p class="notes-side-label">"类型"</p>
                <div class="notes-kinds">{kinds}</div>
            </nav>
            {books_group}
            {tags_group}
        </aside>
    }
}

/// 按天分组的笔记流，底部「更早的记录」按页追加。
#[component]
fn NoteStream(
    filter: RwSignal<NoteFilter>,
    shown: RwSignal<ShownNotes>,
    books: Vec<Notebook>,
    on_retry: Callback<()>,
    retrying: RwSignal<bool>,
) -> impl IntoView {
    let today = use_today();
    let older = RwSignal::new(OlderPages::default());
    let loading_more = RwSignal::new(false);
    let more_failed = RwSignal::new(false);
    let loading = move || shown.with(|s| s.filter != filter.get());
    let books = std::sync::Arc::new(books);

    let body = move || {
        let ShownNotes {
            filter: shown_filter,
            result,
        } = shown.get();
        match result {
            Ok(page) => {
                let filtered = !shown_filter.query.is_empty()
                    || shown_filter.kind.is_some()
                    || !shown_filter.tag.is_empty();
                let total = page.total;
                // 追加的页只在属于当前显示的筛选时才算数。
                let all = Memo::new({
                    let shown_filter = shown_filter.clone();
                    move |_| {
                        let mut all = page.notes.clone();
                        older.with(|extra| {
                            if extra.filter == shown_filter {
                                all.extend(extra.notes.iter().cloned());
                            }
                        });
                        all
                    }
                });
                let empty = Memo::new(move |_| all.with(|all| all.is_empty()));
                let groups = Memo::new(move |_| all.with(|all| group_by_day(all)));
                let loaded_pages = {
                    let shown_filter = shown_filter.clone();
                    move || {
                        older.with(|extra| {
                            if extra.filter == shown_filter {
                                extra.last_page
                            } else {
                                1
                            }
                        })
                    }
                };
                let has_more = {
                    let loaded_pages = loaded_pages.clone();
                    move || i64::from(loaded_pages()) * PAGE_SIZE < total
                };
                let books = books.clone();
                let active_tag = {
                    let tag = shown_filter.tag.clone();
                    Signal::derive(move || tag.clone())
                };
                let on_more = {
                    let base = shown_filter.clone();
                    move |_| {
                        let base = base.clone();
                        let page = loaded_pages() + 1;
                        loading_more.set(true);
                        more_failed.set(false);
                        spawn_local(async move {
                            let result = list_notes(NoteFilter {
                                page,
                                ..base.clone()
                            })
                            .await;
                            let _ = loading_more.try_set(false);
                            match result {
                                Ok(next) => {
                                    let _ = older.try_update(|o| {
                                        if o.filter != base {
                                            *o = OlderPages {
                                                filter: base.clone(),
                                                last_page: 1,
                                                notes: Vec::new(),
                                            };
                                        }
                                        o.last_page = page;
                                        o.notes.extend(next.notes);
                                    });
                                }
                                Err(_) => {
                                    let _ = more_failed.try_set(true);
                                }
                            }
                        });
                    }
                };

                (move || {
                    if empty.get() {
                        if filtered {
                            view! {
                                <EmptyState
                                    icon=IconName::SearchX
                                    title="没有找到相关笔记"
                                    description="换个关键词或类型，或者清除筛选看看全部。"
                                    action=EmptyStateAction::new(
                                        "清除筛选",
                                        move |_| filter.set(first_filter()),
                                    )
                                />
                            }
                            .into_any()
                        } else {
                            view! {
                                <EmptyState
                                    icon=IconName::NotebookPen
                                    title="还没有公开的笔记"
                                    description="随手记下的发现会按天出现在这里。"
                                />
                            }
                            .into_any()
                        }
                    } else {
                        let books = books.clone();
                        let on_more = on_more.clone();
                        let has_more = has_more.clone();
                        view! {
                            <For
                                each=move || groups.get()
                                key=|group| group.key()
                                children=move |group| {
                                    view! {
                                        <DaySection
                                            group
                                            books=books.clone()
                                            today
                                            active_tag
                                            filter
                                        />
                                    }
                                }
                            />
                            <Show when=has_more.clone()>
                                <div class="notes-more">
                                    <Button
                                        variant=ButtonVariant::Secondary
                                        loading=loading_more
                                        onclick=on_more.clone()
                                    >
                                        "更早的记录"
                                    </Button>
                                    <Show when=move || more_failed.get()>
                                        <p class="notes-more-error" role="alert">
                                            "没能加载更早的记录，再试一次。"
                                        </p>
                                    </Show>
                                </div>
                            </Show>
                        }
                        .into_any()
                    }
                })
                .into_any()
            }
            Err(_) => view! {
                <LoadError
                    title="笔记暂时没能加载"
                    retrying=Signal::derive(move || loading() || retrying.get())
                    on_retry=move |_| on_retry.run(())
                />
            }
            .into_any(),
        }
    };

    view! {
        <section
            class="notes-stream page-enter"
            style="--enter-index: 2"
            aria-label="笔记列表"
            aria-busy=move || loading().to_string()
        >
            {body}
        </section>
    }
}

/// 一天的笔记：日期标题（浏览器端再补相对日期）加该天的随记与主题卡片。
#[component]
fn DaySection(
    group: DayGroup,
    books: std::sync::Arc<Vec<Notebook>>,
    today: RwSignal<Option<NaiveDate>>,
    #[prop(into)] active_tag: Signal<String>,
    filter: RwSignal<NoteFilter>,
) -> impl IntoView {
    let date = group.date;
    let items = group
        .notes
        .into_iter()
        .map(|note| {
            if note.kind == NoteKind::Topic {
                let book = notebook_title(&books, &note);
                view! { <TopicCard note book/> }.into_any()
            } else {
                view! {
                    <MomentEntry
                        note
                        active_tag
                        on_tag=move |tag: String| filter.update(|f| f.tag = tag)
                    />
                }
                .into_any()
            }
        })
        .collect_view();
    view! {
        <section class="notes-day" aria-label=date.to_string()>
            <div class="notes-day-head">
                <time class="notes-day-date tabular-nums" datetime=date.to_string()>
                    {date.format("%m.%d").to_string()}
                </time>
                {move || {
                    today
                        .get()
                        .map(|today| {
                            view! { <span class="notes-day-rel">{relative_day(date, today)}</span> }
                        })
                }}
            </div>
            {items}
        </section>
    }
}

/// 随记：无卡片，「时间 + 正文」两列；正文直接渲染，悬停时出现「链接」。
#[component]
fn MomentEntry(
    note: Note,
    #[prop(into)] active_tag: Signal<String>,
    #[prop(into)] on_tag: Callback<String>,
) -> impl IntoView {
    let time = note_time(&note).format("%H:%M").to_string();
    let book = note.notebook_ids.first().copied();
    let href = note_route(&note.slug, book).to_string();
    let body = if note.content_html.is_empty() {
        view! {
            <p class="note-moment-summary">{note.summary.clone()}</p>
            <a class="note-moment-more" href=href.clone()>
                "继续阅读"
            </a>
        }
        .into_any()
    } else {
        view! {
            <div class="note-moment-body">
                <PostContent content_html=note.content_html.clone()/>
            </div>
        }
        .into_any()
    };
    let tags = note
        .tags
        .iter()
        .map(|tag| {
            let pressed = tag.clone();
            let picked = tag.clone();
            view! {
                <button
                    type="button"
                    class="note-moment-tag"
                    aria-pressed=move || (active_tag.get() == pressed).to_string()
                    on:click=move |_| on_tag.run(picked.clone())
                >
                    {format!("#{tag}")}
                </button>
            }
        })
        .collect_view();
    view! {
        <article class="note-moment">
            <time class="note-moment-time tabular-nums" datetime=note_time(&note).to_rfc3339()>
                {time}
            </time>
            <div class="note-moment-main">
                {(!note.title.trim().is_empty())
                    .then(|| view! { <h2 class="note-moment-title">{note.title.clone()}</h2> })}
                {body} <div class="note-moment-foot">
                    {tags}
                    <a class="note-moment-link" href=href aria-label="打开这条随记">
                        <Icon name=IconName::Link size=14/>
                        "链接"
                    </a>
                </div>
            </div>
        </article>
    }
}

/// 主题笔记：浮起卡片，衬线标题、两行摘要与「N 节 · 更新于」。
#[component]
fn TopicCard(note: Note, book: Option<(i32, String)>) -> impl IntoView {
    let updated = note_time(&note).format("%m.%d").to_string();
    let meta = if note.sections > 0 {
        format!("{} 节 · 更新于 {updated}", note.sections)
    } else {
        format!("更新于 {updated}")
    };
    let href = note_route(&note.slug, book.as_ref().map(|(id, _)| *id)).to_string();
    let book_label = book.map(|(_, title)| {
        view! {
            <span class="note-topic-book">
                <Icon name=IconName::BookOpen size=13/>
                {title}
            </span>
        }
    });
    let summary = (!note.summary.trim().is_empty())
        .then(|| view! { <p class="note-topic-summary">{note.summary.clone()}</p> });
    view! {
        <article class="note-topic">
            <div class="note-topic-top">
                <span class="badge">"主题笔记"</span>
                {book_label}
            </div>
            <h2 class="note-topic-title">
                <a class="note-topic-link" href=href>
                    {note.display_title()}
                </a>
            </h2>
            {summary} <div class="note-topic-foot">
                <span class="tabular-nums">{meta}</span>
                <span class="note-topic-arrow" aria-hidden="true">
                    <Icon name=IconName::ArrowRight size=15/>
                </span>
            </div>
        </article>
    }
}

// ======================= 笔记本目录 =======================

/// 笔记本目录路由，对应 `/notes/book/:id`。
pub struct NotebookRoute {
    id: Memo<Option<i32>>,
    books: Resource<Result<Vec<Notebook>, ServerFnError>>,
    notes: Resource<Result<NotePage, ServerFnError>>,
}

#[lazy_route]
impl LazyRoute for NotebookRoute {
    fn data() -> Self {
        let params = use_params_map();
        let id = Memo::new(move |_| {
            params
                .read()
                .get("id")
                .and_then(|id| id.parse::<i32>().ok())
        });
        Self {
            id,
            books: Resource::new(move || id.get(), |_| list_notebooks()),
            notes: Resource::new(
                move || id.get(),
                |id| async move {
                    list_notes(NoteFilter {
                        notebook_id: id,
                        page: 1,
                        ..Default::default()
                    })
                    .await
                },
            ),
        }
    }

    fn view(this: Self) -> AnyView {
        let NotebookRoute { id, books, notes } = this;
        view! {
            <Suspense fallback=|| {
                view! {
                    <div class="notebook">
                        <DelayedSkeleton>
                            <NotebookSkeleton/>
                        </DelayedSkeleton>
                    </div>
                }
            }>
                {move || Suspend::new(async move {
                    let books = books.await;
                    let notes = notes.await;
                    match id.get_untracked() {
                        Some(id) => {
                            view! {
                                <div class="notebook">
                                    <NotebookPage id books notes/>
                                </div>
                            }
                                .into_any()
                        }
                        None => view! { <NotFound/> }.into_any(),
                    }
                })}
            </Suspense>
        }
        .into_any()
    }
}

/// 笔记本页：目录请求失败时如实提示并可重试，不误报「笔记本不存在」。
/// 重试与加载更多都是事件处理里的后台请求，结果写进本组件的镜像信号。
#[component]
fn NotebookPage(
    id: i32,
    books: Result<Vec<Notebook>, ServerFnError>,
    notes: Result<NotePage, ServerFnError>,
) -> impl IntoView {
    let books = RwSignal::new(books);
    let notes = RwSignal::new(notes);
    let books_retrying = RwSignal::new(false);
    let retry_books = move |_| {
        books_retrying.set(true);
        spawn_local(async move {
            let result = list_notebooks().await;
            let _ = books.try_set(result);
            let _ = books_retrying.try_set(false);
        });
    };

    move || match books.get() {
        Err(_) => view! {
            <Title text="笔记本暂时没能加载 · Yggdrasil"/>
            <NotebookBack/>
            <LoadError
                title="笔记本暂时没能加载"
                retrying=books_retrying
                on_retry=retry_books
            />
        }
        .into_any(),
        Ok(books) => match books.iter().find(|b| b.id == id).cloned() {
            None => view! {
                <Title text="笔记本不存在 · Yggdrasil"/>
                <NotebookBack/>
                <EmptyState
                    icon=IconName::BookOpen
                    title="笔记本不存在"
                    description="这个笔记本尚未公开、已归档，或已经被移除。"
                />
            }
            .into_any(),
            Some(book) => {
                let others: Vec<Notebook> = books.iter().filter(|b| b.id != id).cloned().collect();
                view! { <NotebookContent id book others notes/> }.into_any()
            }
        },
    }
}

#[component]
fn NotebookContent(
    id: i32,
    book: Notebook,
    others: Vec<Notebook>,
    notes: RwSignal<Result<NotePage, ServerFnError>>,
) -> impl IntoView {
    let older = RwSignal::new(Vec::<Note>::new());
    let next_page = RwSignal::new(2_i32);
    let loading_more = RwSignal::new(false);
    let more_failed = RwSignal::new(false);
    let notes_retrying = RwSignal::new(false);

    let first_note = move || {
        notes.with(|notes| {
            notes
                .as_ref()
                .ok()
                .and_then(|page| page.notes.first())
                .map(|first| note_route(&first.slug, Some(id)).to_string())
        })
    };
    let on_more = move |_| {
        loading_more.set(true);
        more_failed.set(false);
        spawn_local(async move {
            let filter = NoteFilter {
                notebook_id: Some(id),
                page: next_page.get_untracked(),
                ..Default::default()
            };
            match list_notes(filter).await {
                Ok(next) => {
                    let _ = older.try_update(|o| o.extend(next.notes));
                    let _ = next_page.try_update(|p| *p += 1);
                }
                Err(_) => {
                    let _ = more_failed.try_set(true);
                }
            }
            let _ = loading_more.try_set(false);
        });
    };
    let retry_notes = move |_| {
        notes_retrying.set(true);
        spawn_local(async move {
            let filter = NoteFilter {
                notebook_id: Some(id),
                page: 1,
                ..Default::default()
            };
            let result = list_notes(filter).await;
            let _ = notes.try_set(result);
            let _ = notes_retrying.try_set(false);
        });
    };

    // 条目：首屏页加已追加的页；编号随位置，追加只会在末尾增加，键用 (序号, id)。
    let entries = Memo::new(move |_| {
        notes.with(|notes| match notes {
            Ok(page) => {
                let mut all = page.notes.clone();
                older.with(|extra| all.extend(extra.iter().cloned()));
                all.into_iter().enumerate().collect::<Vec<_>>()
            }
            Err(_) => Vec::new(),
        })
    });
    let has_more = move || {
        notes.with(|notes| {
            notes
                .as_ref()
                .is_ok_and(|page| (entries.with(|e| e.len()) as i64) < page.total)
        })
    };
    let state = Memo::new(move |_| {
        notes.with(|notes| match notes {
            Ok(page) if !page.notes.is_empty() => 1_u8,
            Ok(_) => 2,
            Err(_) => 0,
        })
    });

    let description = (!book.description.trim().is_empty())
        .then(|| view! { <p class="notebook-description">{book.description.clone()}</p> });
    let others_section = (!others.is_empty()).then(|| {
        view! {
            <section class="notebook-others" aria-labelledby="notebook-others-title">
                <h2 id="notebook-others-title" class="section-label">
                    "其他笔记本"
                </h2>
                <div class="notebook-others-grid">
                    {others
                        .into_iter()
                        .map(|other| {
                            view! {
                                <a
                                    class="notebook-other"
                                    href=Route::NotebookDetail { id: other.id }.to_string()
                                >
                                    <span class="notebook-other-title">{other.title}</span>
                                    <span class="notebook-other-count tabular-nums">
                                        {format!("{} 篇", other.note_count)}
                                    </span>
                                </a>
                            }
                        })
                        .collect_view()}
                </div>
            </section>
        }
    });

    view! {
        <Title text=format!("{} · 笔记本", book.title)/>
        <NotebookBack/>
        <header class="notebook-header page-enter">
            <p class="notebook-label">
                <Icon name=IconName::BookOpen size=14/>
                "笔记本"
            </p>
            <h1 class="notebook-title">{book.title.clone()}</h1>
            {description}
            <p class="notebook-meta tabular-nums">
                <span>{format!("{} 篇", book.note_count)}</span>
                <span>"按阅读顺序编排"</span>
                <span>{format!("更新于 {}", book.updated_at.format("%m.%d"))}</span>
            </p>
            {move || {
                first_note()
                    .map(|href| {
                        view! {
                            <a class="btn btn-primary btn-lg notebook-start" href=href>
                                "从第一篇读起"
                                <Icon name=IconName::ArrowRight size=16/>
                            </a>
                        }
                    })
            }}
        </header>
        {move || match state.get() {
            1 => {
                view! {
                    <ol class="notebook-entries page-enter" style="--enter-index: 1">
                        <For
                            each=move || entries.get()
                            key=|(index, note)| (*index, note.id)
                            children=move |(index, note)| {
                                view! {
                                    <li>
                                        <NotebookEntry note number=index + 1 book=id/>
                                    </li>
                                }
                            }
                        />
                    </ol>
                    <Show when=has_more>
                        <div class="notes-more">
                            <Button
                                variant=ButtonVariant::Secondary
                                loading=loading_more
                                onclick=on_more
                            >
                                "加载更多"
                            </Button>
                            <Show when=move || more_failed.get()>
                                <p class="notes-more-error" role="alert">
                                    "没能加载更多笔记，再试一次。"
                                </p>
                            </Show>
                        </div>
                    </Show>
                    <p class="notebook-note">"只列出已发布的笔记。"</p>
                }
                    .into_any()
            }
            2 => {
                view! {
                    <EmptyState
                        icon=IconName::NotebookPen
                        title="这个笔记本还是空的"
                        description="发布的笔记加入后会按顺序列在这里。"
                    />
                }
                    .into_any()
            }
            _ => {
                view! {
                    <LoadError
                        title="目录暂时没能加载"
                        retrying=notes_retrying
                        on_retry=retry_notes
                    />
                }
                    .into_any()
            }
        }}
        {others_section}
    }
}

#[component]
fn NotebookBack() -> impl IntoView {
    view! {
        <a class="notes-back" href=Route::Notes.to_string()>
            <Icon name=IconName::ArrowLeft size=14/>
            "全部笔记"
        </a>
    }
}

#[component]
fn NotebookEntry(note: Note, number: usize, book: i32) -> impl IntoView {
    let updated = note_time(&note).format("%m.%d").to_string();
    let meta = match note.kind {
        NoteKind::Topic if note.sections > 0 => {
            format!("主题笔记 · {} 节 · 更新于 {updated}", note.sections)
        }
        NoteKind::Topic => format!("主题笔记 · 更新于 {updated}"),
        NoteKind::Moment => format!("随记 · {updated}"),
    };
    let summary = (!note.summary.trim().is_empty() && !note.title.trim().is_empty())
        .then(|| view! { <span class="notebook-entry-summary">{note.summary.clone()}</span> });
    view! {
        <a class="notebook-entry" href=note_route(&note.slug, Some(book)).to_string()>
            <span class="notebook-entry-number tabular-nums" aria-hidden="true">
                {format!("{number:02}")}
            </span>
            <span class="notebook-entry-main">
                <span class="notebook-entry-title">{note.display_title()}</span>
                {summary} <span class="notebook-entry-meta tabular-nums">{meta}</span>
            </span>
            <span class="notebook-entry-arrow" aria-hidden="true">
                <Icon name=IconName::ArrowRight size=15/>
            </span>
        </a>
    }
}

// ======================= 阅读页 =======================

/// 笔记阅读路由，对应 `/notes/:slug`。`book` 来自 `?book=`，指明一条属于多个笔记本的
/// 笔记在哪本里读；无法解析时按未指明处理。
pub struct NoteRoute {
    note: Resource<Result<PublicNote, ServerFnError>>,
    context: Resource<Result<Option<NoteContext>, ServerFnError>>,
}

#[lazy_route]
impl LazyRoute for NoteRoute {
    fn data() -> Self {
        let params = use_params_map();
        let query = use_query_map();
        let slug = Memo::new(move |_| params.read().get("slug").unwrap_or_default());
        let book = Memo::new(move |_| {
            query
                .read()
                .get("book")
                .and_then(|book| book.parse::<i32>().ok())
        });
        Self {
            note: Resource::new(move || slug.get(), get_note),
            context: Resource::new(
                move || (slug.get(), book.get()),
                |(slug, book)| get_note_context(slug, book),
            ),
        }
    }

    fn view(this: Self) -> AnyView {
        let NoteRoute { note, context } = this;
        view! {
            <Suspense fallback=|| {
                view! {
                    <div class="notes">
                        <DelayedSkeleton>
                            <NoteReaderSkeleton/>
                        </DelayedSkeleton>
                    </div>
                }
            }>
                {move || Suspend::new(async move {
                    let note = note.await;
                    // 上下文只影响左栏，失败时按不在任何笔记本中处理。
                    let context = context.await.ok().flatten();
                    match note {
                        Ok(note) => {
                            view! {
                                <div class="notes">
                                    <div class="notes-layout note-reader">
                                        <NoteRail note=note.clone() context=context.clone()/>
                                        <NoteDocument note context/>
                                    </div>
                                </div>
                            }
                                .into_any()
                        }
                        Err(err) if is_not_found(&err) => view! { <NotFound/> }.into_any(),
                        Err(_) => view! { <PageLoadFailed/> }.into_any(),
                    }
                })}
            </Suspense>
        }
        .into_any()
    }
}

/// 阅读页左栏：所在笔记本的目录，当前篇浮起并展开本篇小节；不在公开笔记本中时
/// 只有返回链接与本篇小节。小节的当前位置由 yggdrasil-core 的目录 scroll-spy 标出。
#[component]
fn NoteRail(note: PublicNote, context: Option<NoteContext>) -> impl IntoView {
    let body = if let Some(context) = context {
        let description = (!context.notebook.description.trim().is_empty()).then(|| {
            view! { <p class="note-rail-book-description">{context.notebook.description.clone()}</p> }
        });
        view! {
            <div class="note-rail-book">
                <p class="notes-side-label">"笔记本"</p>
                <a
                    class="note-rail-book-title"
                    href=Route::NotebookDetail { id: context.notebook.id }.to_string()
                >
                    {context.notebook.title.clone()}
                </a>
                {description}
            </div>
            <NotebookDirectory
                context
                current=note.slug.clone()
                toc_html=note.toc_html.clone()
                spy=true
            />
        }
        .into_any()
    } else if !note.toc_html.is_empty() {
        view! {
            <div class="note-rail-book">
                <p class="notes-side-label">"本篇"</p>
            </div>
            <NoteSections toc_html=note.toc_html.clone() spy=true/>
        }
        .into_any()
    } else {
        ().into_any()
    };
    view! {
        <aside class="notes-side note-rail" aria-label="笔记本目录">
            <NotebookBack/>
            {body}
        </aside>
    }
}

/// 笔记本目录的编号列表；当前篇浮起，其下展开本篇小节。
#[component]
fn NotebookDirectory(
    context: NoteContext,
    current: String,
    toc_html: String,
    spy: bool,
) -> impl IntoView {
    let book = context.notebook.id;
    let items = context
        .siblings
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let is_current = item.slug == current;
            let sections = (is_current && !toc_html.is_empty())
                .then(|| view! { <NoteSections toc_html=toc_html.clone() spy/> });
            view! {
                <li>
                    <a
                        class="note-directory-item"
                        href=note_route(&item.slug, Some(book)).to_string()
                        aria-current=is_current.then_some("page")
                    >
                        <span class="note-directory-number tabular-nums" aria-hidden="true">
                            {format!("{:02}", index + 1)}
                        </span>
                        <span>{item.title.clone()}</span>
                    </a>
                    {sections}
                </li>
            }
        })
        .collect_view();
    view! { <ol class="note-directory">{items}</ol> }
}

/// 本篇小节（目录 HTML 的一级）。`spy` 为真时挂上 scroll-spy；抽屉里的副本不挂。
#[component]
fn NoteSections(toc_html: String, spy: bool) -> impl IntoView {
    #[cfg(target_arch = "wasm32")]
    {
        Effect::new(move |_| {
            if let (true, Some(window)) = (spy, web_sys::window()) {
                crate::utils::js::invoke_optional_global(
                    &window,
                    "__initTocSidebar",
                    &["#note-rail-toc".into(), "#note-content".into()],
                );
            }
        });
        on_cleanup(move || {
            if let (true, Some(window)) = (spy, web_sys::window()) {
                crate::utils::js::invoke_optional_global(
                    &window,
                    "__disposeTocSidebar",
                    &["#note-rail-toc".into()],
                );
            }
        });
    }
    view! {
        // P5 §8.2：scroll-spy 增强在 yggdrasil-enhance.js，按需注入。
        {spy.then(|| view! { <crate::components::enhance_assets::EnhanceScript/> })}
        <nav id=spy.then_some("note-rail-toc") class="note-sections" aria-label="本篇小节">
            <div class="toc-sidebar-body" inner_html=toc_html></div>
        </nav>
    }
}

/// 笔记正文：类型与篇序、衬线标题、时间与标签、正文，以及笔记本内的上一篇 / 下一篇。
/// 后台编辑器的预览（`preview`）只显示正文部分。
///
/// 组件只运行一次：同一实例内 `note` 不会更新，换笔记须由调用方重建。
#[component]
pub fn NoteDocument(
    note: PublicNote,
    #[prop(optional)] preview: bool,
    #[prop(optional, into)] context: MaybeProp<NoteContext>,
) -> impl IntoView {
    let context = context.get_untracked();
    let title = note.display_title();
    let created = note.created_at.format("%Y.%m.%d").to_string();
    let updated = note.updated_at.format("%m.%d").to_string();
    let book = context.as_ref().map(|c| c.notebook.id);
    let sheet_open = RwSignal::new(false);

    let page_title = (!preview).then(|| view! { <Title text=format!("{title} · 笔记")/> });
    let mobile_entry = (!preview)
        .then(|| {
            let aria_expanded = move || sheet_open.get().to_string();
            if let Some(context) = context.as_ref() {
                Some(
                    view! {
                        <button
                            id="note-directory-trigger"
                            class="note-mobile-entry"
                            type="button"
                            aria-haspopup="dialog"
                            aria-expanded=aria_expanded
                            aria-controls="note-directory-sheet"
                            on:click=move |_| sheet_open.set(true)
                        >
                            <Icon name=IconName::BookOpen size=15/>
                            <span class="tabular-nums">
                                {format!(
                                    "{} · 第 {} / {} 篇",
                                    context.notebook.title,
                                    context.position,
                                    context.total,
                                )}
                            </span>
                            <span class="note-mobile-entry-action">"目录"</span>
                        </button>
                    }
                    .into_any(),
                )
            } else if !note.toc_html.is_empty() {
                Some(
                    view! {
                        <button
                            id="note-directory-trigger"
                            class="note-mobile-entry"
                            type="button"
                            aria-haspopup="dialog"
                            aria-expanded=aria_expanded
                            aria-controls="note-directory-sheet"
                            on:click=move |_| sheet_open.set(true)
                        >
                            <Icon name=IconName::List size=15/>
                            <span>"本篇小节"</span>
                            <span class="note-mobile-entry-action">"目录"</span>
                        </button>
                    }
                    .into_any(),
                )
            } else {
                None
            }
        })
        .flatten();
    let position = context.as_ref().map(|context| {
        view! {
            <span class="tabular-nums">
                {format!("第 {} / {} 篇", context.position, context.total)}
            </span>
        }
    });
    let updated_meta = (note.updated_at.date_naive() != note.created_at.date_naive())
        .then(|| view! { <span>{format!("更新于 {updated}")}</span> });
    let sections_meta =
        (note.sections > 0).then(|| view! { <span>{format!("{} 节", note.sections)}</span> });
    let tags = (!note.tags.is_empty()).then(|| {
        view! {
            <p class="note-document-tags">
                {note
                    .tags
                    .iter()
                    .map(|tag| view! { <span>{format!("#{tag}")}</span> })
                    .collect_view()}
            </p>
        }
    });
    let continue_nav = context
        .as_ref()
        .filter(|c| c.prev.is_some() || c.next.is_some())
        .map(|context| {
            let prev = match context.prev.as_ref() {
                Some(prev) => view! {
                    <a
                        class="note-continue-link"
                        href=note_route(&prev.slug, book).to_string()
                    >
                        <span class="note-continue-dir tabular-nums">
                            {format!("← {:02}", context.position - 1)}
                        </span>
                        <span class="note-continue-title">{prev.title.clone()}</span>
                    </a>
                }
                .into_any(),
                None => view! { <span></span> }.into_any(),
            };
            let next = context.next.as_ref().map(|next| {
                view! {
                    <a
                        class="note-continue-link is-next"
                        href=note_route(&next.slug, book).to_string()
                    >
                        <span class="note-continue-dir tabular-nums">
                            {format!("{:02} →", context.position + 1)}
                        </span>
                        <span class="note-continue-title">{next.title.clone()}</span>
                    </a>
                }
            });
            view! {
                <footer class="note-continue">
                    <p class="note-continue-label">
                        {format!("在《{}》中继续", context.notebook.title)}
                    </p>
                    <nav class="note-continue-links" aria-label="同一笔记本的上一篇与下一篇">
                        {prev} {next}
                    </nav>
                </footer>
            }
        });
    let sheet = (!preview).then(|| {
        let context = context.clone();
        let current = note.slug.clone();
        let toc_html = note.toc_html.clone();
        view! {
            <BottomSheet
                open=sheet_open
                id="note-directory-sheet"
                trigger_id="note-directory-trigger"
                label="笔记本目录"
            >
                <NoteSheetBody
                    context=context.clone()
                    current=current.clone()
                    toc_html=toc_html.clone()
                />
            </BottomSheet>
        }
    });

    view! {
        {page_title}
        <article class="note-document">
            {mobile_entry} <div class="note-document-kicker">
                <span class="badge">{note.kind.label()}</span>
                {position}
            </div> <h1 class="note-document-title">{title.clone()}</h1>
            <p class="note-document-meta tabular-nums">
                <span>{format!("记录于 {created}")}</span>
                {updated_meta}
                {sections_meta}
            </p> {tags}
            <div id="note-content" class="note-document-body">
                <PostContent content_html=note.content_html.clone()/>
            </div> {continue_nav}
        </article>
        {sheet}
    }
}

/// 手机抽屉里的目录：选中一篇或一个小节后收起。锚点点击会被全局处理拦下，收不到
/// click，所以改听 pointerup 与 Enter。
#[component]
fn NoteSheetBody(context: Option<NoteContext>, current: String, toc_html: String) -> impl IntoView {
    let sheet = use_sheet_close();
    let body = match context {
        Some(context) => view! {
            <p class="note-sheet-title">{context.notebook.title.clone()}</p>
            <NotebookDirectory context current toc_html spy=false/>
        }
        .into_any(),
        None => view! {
            <p class="note-sheet-title">"本篇小节"</p>
            <NoteSections toc_html spy=false/>
        }
        .into_any(),
    };
    view! {
        <div
            class="note-sheet"
            on:pointerup=move |_| sheet.close()
            on:keydown=move |event| {
                if event.key() == "Enter" {
                    sheet.close();
                }
            }
        >
            {body}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn note(id: i32, day: u32, hour: u32) -> Note {
        let at = Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap();
        Note {
            id,
            slug: format!("n{id}"),
            version: 1,
            kind: NoteKind::Moment,
            title: String::new(),
            content_md: String::new(),
            content_html: String::new(),
            toc_html: String::new(),
            summary: String::new(),
            tags: Vec::new(),
            created_at: at,
            updated_at: at,
            published_at: Some(at),
            published_version: None,
            knowledge_version: None,
            deleted_at: None,
            notebook_ids: Vec::new(),
            sections: 0,
        }
    }

    #[test]
    fn groups_adjacent_notes_by_day() {
        let groups = group_by_day(&[note(1, 15, 16), note(2, 15, 9), note(3, 12, 21)]);
        let shape: Vec<_> = groups
            .iter()
            .map(|g| (g.date.day(), g.notes.len()))
            .collect();
        assert_eq!(shape, [(15, 2), (12, 1)]);
    }

    #[test]
    fn day_group_key_changes_when_notes_change() {
        let one = group_by_day(&[note(1, 15, 16)]);
        let two = group_by_day(&[note(1, 15, 16), note(2, 15, 9)]);
        assert_ne!(one[0].key(), two[0].key());
        assert_eq!(one[0].key(), group_by_day(&[note(1, 15, 16)])[0].key());
    }

    #[test]
    fn relative_day_names_recent_days_and_other_years() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        assert_eq!(relative_day(day(2026, 10, 15), today), "今天");
        assert_eq!(relative_day(day(2026, 10, 14), today), "昨天");
        // 2026-10-11 是周日。
        assert_eq!(relative_day(day(2026, 10, 11), today), "周日");
        assert_eq!(relative_day(day(2026, 9, 1), today), "");
        assert_eq!(relative_day(day(2025, 12, 31), today), "2025");
    }

    #[test]
    fn note_routes_carry_the_notebook_only_when_known() {
        assert_eq!(note_route("a", Some(3)).to_string(), "/notes/a?book=3");
        // 查询值为空时整个 `?book=` 省略；阅读页把缺省视为「未指明笔记本」。
        assert_eq!(note_route("a", None).to_string(), "/notes/a");
    }

    #[test]
    fn first_filter_is_the_unfiltered_first_page() {
        let filter = first_filter();
        assert_eq!(filter.page, 1);
        assert!(filter.query.is_empty() && filter.tag.is_empty() && filter.kind.is_none());
    }

    #[test]
    fn topic_card_links_to_the_note_within_its_notebook() {
        let mut topic = note(4, 15, 9);
        topic.kind = NoteKind::Topic;
        topic.title = "主题".to_string();
        topic.summary = "摘要".to_string();
        topic.sections = 3;
        let html = Owner::new().with(|| {
            view! { <TopicCard note=topic book=Some((2, "笔记本".to_string()))/> }.to_html()
        });
        assert!(html.contains(r#"href="/notes/n4?book=2""#));
        assert!(html.contains("3 节 · 更新于 10.15"));
        assert!(html.contains("note-topic-book"));
    }

    #[test]
    fn notebook_entry_numbers_and_describes_each_kind() {
        let mut moment = note(5, 15, 9);
        moment.title = "标题".to_string();
        moment.summary = "摘要".to_string();
        let html =
            Owner::new().with(|| view! { <NotebookEntry note=moment number=3 book=9/> }.to_html());
        assert!(html.contains(r#"href="/notes/n5?book=9""#));
        assert!(html.contains("03"));
        assert!(html.contains("随记 · 10.15"));
        assert!(html.contains("notebook-entry-summary"));
    }
}
