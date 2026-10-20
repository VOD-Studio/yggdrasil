//! 笔记：列表 `/notes`、笔记本目录 `/notes/book/:id` 与阅读页 `/notes/:slug?book=`。
//!
//! 列表与阅读页共用 880px 容器内的「200px 左栏 + 内容」布局：列表页左栏是筛选，
//! 阅读页左栏是所在笔记本的目录，进入阅读时布局不跳。
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use dioxus::prelude::*;

use crate::api::notes::{get_note, get_note_context, list_notebooks, list_notes, note_stats};
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::post::post_content::PostContent;
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{
    use_sheet_close, BottomSheet, Button, ButtonVariant, Icon, IconName, LoadError,
};
use crate::models::note::*;
use crate::router::Route;
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
fn use_today() -> Signal<Option<NaiveDate>> {
    let mut today = use_signal(|| None::<NaiveDate>);
    use_effect(move || {
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

// ======================= 列表页 =======================

#[component]
pub fn Notes() -> Element {
    rsx! {
        document::Title { "笔记 · Yggdrasil" }
        div { class: "notes",
            SuspenseBoundary {
                fallback: move |_| rsx! { DelayedSkeleton { NotesPageSkeleton {} } },
                NotesBody {}
            }
        }
    }
}

/// 当前显示的笔记列表及其筛选；与请求中的筛选不同时表示新结果仍在加载。
#[derive(Clone, PartialEq)]
struct ShownNotes {
    filter: NoteFilter,
    result: Option<Result<NotePage, ServerFnError>>,
}

/// 首屏随 SSR 预取挂载时的筛选；之后的切换在后台请求，结果返回前继续显示当前列表。
/// 重新运行 `use_server_future` 会再次挂起回骨架屏，页面高度坍缩并带动滚动位置跳动。
/// 回到挂载时的筛选直接复用预取结果。
///
/// 同时返回重试：重新请求当前显示失败的那一份。
fn use_note_page(
    filter: Signal<NoteFilter>,
) -> Result<(Memo<ShownNotes>, Callback<()>), RenderError> {
    let first_filter = use_hook(|| filter.peek().clone());
    let mut first = use_server_future({
        let first_filter = first_filter.clone();
        move || list_notes(first_filter.clone())
    })?;
    // 挂载时的筛选直接使用预取结果；其它筛选才请求，重新开始时取消上一个请求。
    // 闭包每次渲染都会被替换，不能在闭包里保存状态。
    let mut next = use_resource({
        let first_filter = first_filter.clone();
        move || {
            let wanted = filter();
            let fetch = wanted != first_filter;
            async move {
                if fetch {
                    Some((wanted.clone(), list_notes(wanted).await))
                } else {
                    None
                }
            }
        }
    });
    let shown = use_memo({
        let first_filter = first_filter.clone();
        move || match &*next.read() {
            Some(Some((filter, result))) => ShownNotes {
                filter: filter.clone(),
                result: Some(result.clone()),
            },
            _ => ShownNotes {
                filter: first_filter.clone(),
                result: first.read().clone(),
            },
        }
    });
    let retry = use_callback(move |()| {
        if *filter.peek() == first_filter {
            first.restart();
        } else {
            next.restart();
        }
    });
    Ok((shown, retry))
}

/// 「更早的记录」追加的页：属于哪个筛选、已加载到第几页。筛选变化后自然作废。
#[derive(Clone, Default, PartialEq)]
struct OlderPages {
    filter: NoteFilter,
    last_page: i32,
    notes: Vec<Note>,
}

#[component]
fn NotesBody() -> Element {
    let entry_id = use_hook(crate::bridges::navigation::entry_id);
    let initial = use_hook(|| {
        let mut filter =
            crate::bridges::navigation::read_state::<NoteFilter>("notes").unwrap_or_default();
        filter.page = 1;
        filter
    });
    let filter = use_signal(|| initial.clone());
    use_effect(move || crate::bridges::navigation::write_state(&entry_id, "notes", &filter()));
    // SSR 只轮询正在挂起的任务，预取须逐个等待；提前发出的第二个请求会丢失唤醒。
    let books = use_server_future(list_notebooks)?;
    let stats = use_server_future(note_stats)?;
    let (notes, retry) = use_note_page(filter)?;

    let books: Vec<Notebook> = books
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let stats: Option<NoteStats> = stats.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();

    rsx! {
        header { class: "notes-header page-enter",
            div {
                h1 { class: "notes-title", "笔记" }
                p { class: "notes-intro", "随手记下的发现，和反复琢磨后成形的主题。" }
            }
            if let Some(stats) = stats.as_ref() {
                p { class: "notes-count tabular-nums",
                    "{stats.total()} 条 · {books.len()} 个笔记本"
                }
            }
        }
        div { class: "notes-layout",
            NotesSide { filter, books: books.clone(), stats }
            NoteStream { filter, notes, books, on_retry: retry }
        }
    }
}

/// 左栏：筛选框、类型、笔记本与常用标签。窄屏只留筛选框与横排类型。
#[component]
fn NotesSide(
    mut filter: Signal<NoteFilter>,
    books: Vec<Notebook>,
    stats: Option<NoteStats>,
) -> Element {
    let mut query = use_signal(|| filter.peek().query.clone());
    let mut composing = use_signal(|| false);
    let mut request = use_signal(|| 0_u64);
    let mut apply_query = move || {
        request += 1;
        let id = request();
        if composing() {
            return;
        }
        spawn(async move {
            sleep_ms(FILTER_DEBOUNCE_MS).await;
            let q = query().trim().to_string();
            if request() == id && filter.peek().query != q {
                filter.with_mut(|f| f.query = q);
            }
        });
    };
    let current = filter();
    let count = |kind: Option<NoteKind>| {
        stats.as_ref().map(|s| match kind {
            None => s.total(),
            Some(NoteKind::Moment) => s.moments,
            Some(NoteKind::Topic) => s.topics,
        })
    };
    let tags = stats
        .as_ref()
        .map(|s| s.tags.iter().take(SIDE_TAGS).cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    rsx! {
        aside { class: "notes-side page-enter", style: "--enter-index: 1", aria_label: "笔记筛选",
            form {
                class: "notes-filter",
                role: "search",
                onsubmit: move |event| {
                    event.prevent_default();
                    request += 1;
                    let q = query().trim().to_string();
                    filter.with_mut(|f| f.query = q);
                },
                Icon { name: IconName::Search, size: 14 }
                label { class: "sr-only", r#for: "notes-filter-input", "筛选笔记" }
                input {
                    id: "notes-filter-input",
                    r#type: "search",
                    placeholder: "筛选笔记",
                    autocomplete: "off",
                    maxlength: "200",
                    value: "{query}",
                    oninput: move |event| {
                        query.set(event.value());
                        apply_query();
                    },
                    oncompositionstart: move |_| composing.set(true),
                    oncompositionend: move |_| {
                        composing.set(false);
                        apply_query();
                    },
                }
            }
            nav { class: "notes-side-group", aria_label: "笔记类型",
                p { class: "notes-side-label", "类型" }
                div { class: "notes-kinds",
                    for (kind, label) in KINDS {
                        button {
                            key: "{label}",
                            r#type: "button",
                            class: "notes-side-item",
                            aria_pressed: "{current.kind == kind}",
                            onclick: move |_| {
                                if filter.peek().kind != kind {
                                    filter.with_mut(|f| f.kind = kind);
                                }
                            },
                            span { "{label}" }
                            if let Some(n) = count(kind) {
                                span { class: "notes-side-count tabular-nums", "{n}" }
                            }
                        }
                    }
                }
            }
            if !books.is_empty() {
                nav { class: "notes-side-group notes-side-extra", aria_label: "笔记本",
                    p { class: "notes-side-label", "笔记本" }
                    for book in books {
                        Link {
                            key: "{book.id}",
                            class: "notes-side-item",
                            to: Route::NotebookDetail { id: book.id },
                            Icon { name: IconName::BookOpen, size: 15, class: "notes-side-icon" }
                            span { class: "notes-side-text", "{book.title}" }
                            span { class: "notes-side-count tabular-nums", "{book.note_count}" }
                        }
                    }
                }
            }
            if !tags.is_empty() {
                div { class: "notes-side-group notes-side-extra",
                    p { class: "notes-side-label", id: "notes-tags-label", "标签" }
                    div { class: "notes-side-tags", role: "group", aria_labelledby: "notes-tags-label",
                        for (tag, n) in tags {
                            button {
                                key: "{tag}",
                                r#type: "button",
                                class: "chip",
                                aria_pressed: "{current.tag == tag}",
                                title: "{n} 条",
                                onclick: {
                                    let tag = tag.clone();
                                    move |_| {
                                        let next = if filter.peek().tag == tag { String::new() } else { tag.clone() };
                                        filter.with_mut(|f| f.tag = next);
                                    }
                                },
                                "{tag}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 按天分组的笔记流，底部「更早的记录」按页追加。
#[component]
fn NoteStream(
    mut filter: Signal<NoteFilter>,
    notes: Memo<ShownNotes>,
    books: Vec<Notebook>,
    on_retry: Callback<()>,
) -> Element {
    let today = use_today();
    let mut older = use_signal(OlderPages::default);
    let mut loading_more = use_signal(|| false);
    let mut more_failed = use_signal(|| false);
    let shown = notes.read().clone();
    let loading = shown.filter != filter();
    let filtered = !shown.filter.query.is_empty()
        || shown.filter.kind.is_some()
        || !shown.filter.tag.is_empty();

    let body = match &shown.result {
        Some(Ok(page)) => {
            let mut all = page.notes.clone();
            let extra = older.read();
            if extra.filter == shown.filter {
                all.extend(extra.notes.iter().cloned());
            }
            let loaded_pages = if extra.filter == shown.filter {
                extra.last_page
            } else {
                1
            };
            let has_more = i64::from(loaded_pages) * PAGE_SIZE < page.total;
            let groups = group_by_day(&all);
            if all.is_empty() {
                rsx! {
                    if filtered {
                        EmptyState {
                            icon: IconName::SearchX,
                            title: "没有找到相关笔记",
                            description: "换个关键词或类型，或者清除筛选看看全部。",
                            action: EmptyStateAction {
                                label: "清除筛选".to_string(),
                                onclick: Callback::new(move |_| filter.set(NoteFilter { page: 1, ..Default::default() })),
                            },
                        }
                    } else {
                        EmptyState { icon: IconName::NotebookPen, title: "还没有公开的笔记", description: "随手记下的发现会按天出现在这里。" }
                    }
                }
            } else {
                let base = shown.filter.clone();
                rsx! {
                    for group in groups {
                        section { key: "{group.date}", class: "notes-day", aria_label: "{group.date}",
                            div { class: "notes-day-head",
                                time { class: "notes-day-date tabular-nums", datetime: "{group.date}",
                                    {group.date.format("%m.%d").to_string()}
                                }
                                if let Some(today) = today() {
                                    span { class: "notes-day-rel", {relative_day(group.date, today)} }
                                }
                            }
                            for note in group.notes {
                                if note.kind == NoteKind::Topic {
                                    TopicCard {
                                        key: "{note.id}-{note.version}",
                                        book: notebook_title(&books, &note),
                                        note,
                                    }
                                } else {
                                    MomentEntry {
                                        key: "{note.id}-{note.version}",
                                        note,
                                        active_tag: shown.filter.tag.clone(),
                                        on_tag: move |tag: String| filter.with_mut(|f| f.tag = tag),
                                    }
                                }
                            }
                        }
                    }
                    if has_more {
                        div { class: "notes-more",
                            Button {
                                variant: ButtonVariant::Secondary,
                                loading: loading_more(),
                                onclick: move |_| {
                                    let base = base.clone();
                                    let page = loaded_pages + 1;
                                    loading_more.set(true);
                                    more_failed.set(false);
                                    spawn(async move {
                                        let result = list_notes(NoteFilter { page, ..base.clone() }).await;
                                        loading_more.set(false);
                                        match result {
                                            Ok(next) => older.with_mut(|o| {
                                                if o.filter != base {
                                                    *o = OlderPages { filter: base.clone(), last_page: 1, notes: Vec::new() };
                                                }
                                                o.last_page = page;
                                                o.notes.extend(next.notes);
                                            }),
                                            Err(_) => more_failed.set(true),
                                        }
                                    });
                                },
                                "更早的记录"
                            }
                            if more_failed() {
                                p { class: "notes-more-error", role: "alert", "没能加载更早的记录，再试一次。" }
                            }
                        }
                    }
                }
            }
        }
        Some(Err(_)) => rsx! {
            LoadError {
                title: "笔记暂时没能加载",
                retrying: loading,
                on_retry: move |_| on_retry.call(()),
            }
        },
        None => rsx! { NotesStreamSkeleton {} },
    };

    rsx! {
        section {
            class: "notes-stream page-enter",
            style: "--enter-index: 2",
            aria_label: "笔记列表",
            aria_busy: "{loading}",
            {body}
        }
    }
}

/// 随记：无卡片，「时间 + 正文」两列；正文直接渲染，悬停时出现「链接」。
#[component]
fn MomentEntry(note: Note, active_tag: String, on_tag: EventHandler<String>) -> Element {
    let time = note_time(&note).format("%H:%M").to_string();
    let book = note.notebook_ids.first().copied();
    rsx! {
        article { class: "note-moment",
            time { class: "note-moment-time tabular-nums", datetime: note_time(&note).to_rfc3339(), "{time}" }
            div { class: "note-moment-main",
                if !note.title.trim().is_empty() {
                    h2 { class: "note-moment-title", "{note.title}" }
                }
                if note.content_html.is_empty() {
                    p { class: "note-moment-summary", "{note.summary}" }
                    Link { class: "note-moment-more", to: note_route(&note.slug, book), "继续阅读" }
                } else {
                    div { class: "note-moment-body", PostContent { content_html: note.content_html.clone() } }
                }
                div { class: "note-moment-foot",
                    for tag in note.tags.iter().cloned() {
                        button {
                            key: "{tag}",
                            r#type: "button",
                            class: "note-moment-tag",
                            aria_pressed: "{active_tag == tag}",
                            onclick: move |_| on_tag.call(tag.clone()),
                            "#{tag}"
                        }
                    }
                    Link {
                        class: "note-moment-link",
                        to: note_route(&note.slug, book),
                        aria_label: "打开这条随记",
                        Icon { name: IconName::Link, size: 14 }
                        "链接"
                    }
                }
            }
        }
    }
}

/// 主题笔记：浮起卡片，衬线标题、两行摘要与「N 节 · 更新于」。
#[component]
fn TopicCard(note: Note, book: Option<(i32, String)>) -> Element {
    let updated = note_time(&note).format("%m.%d").to_string();
    let meta = if note.sections > 0 {
        format!("{} 节 · 更新于 {updated}", note.sections)
    } else {
        format!("更新于 {updated}")
    };
    rsx! {
        article { class: "note-topic",
            div { class: "note-topic-top",
                span { class: "badge", "主题笔记" }
                if let Some((_, title)) = book.as_ref() {
                    span { class: "note-topic-book",
                        Icon { name: IconName::BookOpen, size: 13 }
                        "{title}"
                    }
                }
            }
            h2 { class: "note-topic-title",
                Link { class: "note-topic-link", to: note_route(&note.slug, book.as_ref().map(|(id, _)| *id)),
                    {note.display_title()}
                }
            }
            if !note.summary.trim().is_empty() {
                p { class: "note-topic-summary", "{note.summary}" }
            }
            div { class: "note-topic-foot",
                span { class: "tabular-nums", "{meta}" }
                span { class: "note-topic-arrow", aria_hidden: "true", Icon { name: IconName::ArrowRight, size: 15 } }
            }
        }
    }
}

// ======================= 笔记本目录 =======================

#[component]
pub fn NotebookDetail(id: i32) -> Element {
    rsx! {
        for key in std::iter::once(id) {
            div { key: "{key}", class: "notebook",
                SuspenseBoundary {
                    fallback: move |_| rsx! { DelayedSkeleton { NotebookSkeleton {} } },
                    NotebookPage { id: key }
                }
            }
        }
    }
}

#[component]
fn NotebookPage(id: i32) -> Element {
    let books = use_server_future(list_notebooks)?;
    let mut notes = use_server_future(move || {
        list_notes(NoteFilter {
            notebook_id: Some(id),
            page: 1,
            ..Default::default()
        })
    })?;
    let mut older = use_signal(Vec::<Note>::new);
    let mut next_page = use_signal(|| 2_i32);
    let mut loading_more = use_signal(|| false);

    let books: Vec<Notebook> = books
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let Some(book) = books.iter().find(|b| b.id == id).cloned() else {
        return rsx! {
            document::Title { "笔记本不存在 · Yggdrasil" }
            NotebookBack {}
            EmptyState { icon: IconName::BookOpen, title: "笔记本不存在", description: "这个笔记本尚未公开、已归档，或已经被移除。" }
        };
    };
    let others: Vec<Notebook> = books.iter().filter(|b| b.id != id).cloned().collect();
    let page = notes.read().as_ref().and_then(|r| r.as_ref().ok()).cloned();

    rsx! {
        document::Title { "{book.title} · 笔记本" }
        NotebookBack {}
        header { class: "notebook-header page-enter",
            p { class: "notebook-label",
                Icon { name: IconName::BookOpen, size: 14 }
                "笔记本"
            }
            h1 { class: "notebook-title", "{book.title}" }
            if !book.description.trim().is_empty() {
                p { class: "notebook-description", "{book.description}" }
            }
            p { class: "notebook-meta tabular-nums",
                span { "{book.note_count} 篇" }
                span { "按阅读顺序编排" }
                span { "更新于 {book.updated_at.format(\"%m.%d\")}" }
            }
            if let Some(first) = page.as_ref().and_then(|p| p.notes.first()) {
                Link {
                    class: "btn btn-primary btn-lg notebook-start",
                    to: note_route(&first.slug, Some(id)),
                    "从第一篇读起"
                    Icon { name: IconName::ArrowRight, size: 16 }
                }
            }
        }
        match page {
            Some(page) if !page.notes.is_empty() => {
                let mut all = page.notes.clone();
                all.extend(older.read().iter().cloned());
                let has_more = (all.len() as i64) < page.total;
                rsx! {
                    ol { class: "notebook-entries page-enter", style: "--enter-index: 1",
                        for (index, note) in all.into_iter().enumerate() {
                            li { key: "{note.id}",
                                NotebookEntry { note, number: index + 1, book: id }
                            }
                        }
                    }
                    if has_more {
                        div { class: "notes-more",
                            Button {
                                variant: ButtonVariant::Secondary,
                                loading: loading_more(),
                                onclick: move |_| {
                                    loading_more.set(true);
                                    spawn(async move {
                                        let filter = NoteFilter { notebook_id: Some(id), page: next_page(), ..Default::default() };
                                        if let Ok(next) = list_notes(filter).await {
                                            older.write().extend(next.notes);
                                            next_page += 1;
                                        }
                                        loading_more.set(false);
                                    });
                                },
                                "加载更多"
                            }
                        }
                    }
                    p { class: "notebook-note", "只列出已发布的笔记。" }
                }
            }
            Some(_) => rsx! {
                EmptyState { icon: IconName::NotebookPen, title: "这个笔记本还是空的", description: "发布的笔记加入后会按顺序列在这里。" }
            },
            None => rsx! {
                LoadError { title: "目录暂时没能加载", on_retry: move |_| notes.restart() }
            },
        }
        if !others.is_empty() {
            section { class: "notebook-others", aria_labelledby: "notebook-others-title",
                h2 { id: "notebook-others-title", class: "section-label", "其他笔记本" }
                div { class: "notebook-others-grid",
                    for other in others {
                        Link { key: "{other.id}", class: "notebook-other", to: Route::NotebookDetail { id: other.id },
                            span { class: "notebook-other-title", "{other.title}" }
                            span { class: "notebook-other-count tabular-nums", "{other.note_count} 篇" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NotebookBack() -> Element {
    rsx! {
        Link { class: "notes-back", to: Route::Notes {},
            Icon { name: IconName::ArrowLeft, size: 14 }
            "全部笔记"
        }
    }
}

#[component]
fn NotebookEntry(note: Note, number: usize, book: i32) -> Element {
    let updated = note_time(&note).format("%m.%d").to_string();
    let meta = match note.kind {
        NoteKind::Topic if note.sections > 0 => {
            format!("主题笔记 · {} 节 · 更新于 {updated}", note.sections)
        }
        NoteKind::Topic => format!("主题笔记 · 更新于 {updated}"),
        NoteKind::Moment => format!("随记 · {updated}"),
    };
    rsx! {
        Link { class: "notebook-entry", to: note_route(&note.slug, Some(book)),
            span { class: "notebook-entry-number tabular-nums", aria_hidden: "true", {format!("{number:02}")} }
            span { class: "notebook-entry-main",
                span { class: "notebook-entry-title", {note.display_title()} }
                if !note.summary.trim().is_empty() && !note.title.trim().is_empty() {
                    span { class: "notebook-entry-summary", "{note.summary}" }
                }
                span { class: "notebook-entry-meta tabular-nums", "{meta}" }
            }
            span { class: "notebook-entry-arrow", aria_hidden: "true", Icon { name: IconName::ArrowRight, size: 15 } }
        }
    }
}

// ======================= 阅读页 =======================

/// 阅读页。`book` 来自 `?book=`，指明一条属于多个笔记本的笔记在哪本里读。
#[component]
pub fn NoteDetail(slug: String, #[props(default)] book: String) -> Element {
    let book = book.parse::<i32>().ok();
    rsx! {
        for (key, slug) in std::iter::once((format!("{slug}|{book:?}"), slug)) {
            div { key: "{key}", class: "notes",
                SuspenseBoundary {
                    fallback: move |_| rsx! { DelayedSkeleton { NoteReaderSkeleton {} } },
                    NoteReader { slug, book }
                }
            }
        }
    }
}

#[component]
fn NoteReader(slug: String, book: Option<i32>) -> Element {
    let note_slug = slug.clone();
    let note = use_server_future(move || get_note(note_slug.clone()))?;
    let context = use_server_future(move || get_note_context(slug.clone(), book))?;
    let note = note.read().as_ref().map(|r| r.as_ref().ok().cloned());
    // 上下文只影响左栏，失败时按不在任何笔记本中处理。
    let context = context
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .flatten();

    match note {
        Some(Some(note)) => rsx! {
            div { class: "notes-layout note-reader",
                NoteRail { note: note.clone(), context: context.clone() }
                NoteDocument { note, context }
            }
        },
        Some(None) => rsx! {
            div { class: "note-missing",
                NotebookBack {}
                EmptyState {
                    icon: IconName::NotebookPen,
                    title: "这条笔记暂时无法阅读",
                    description: "笔记可能尚未公开、已撤回，或服务暂时不可用。",
                }
            }
        },
        None => rsx! {},
    }
}

/// 阅读页左栏：所在笔记本的目录，当前篇浮起并展开本篇小节；不在公开笔记本中时
/// 只有返回链接与本篇小节。小节的当前位置由 yggdrasil-core 的目录 scroll-spy 标出。
#[component]
fn NoteRail(note: Note, context: Option<NoteContext>) -> Element {
    rsx! {
        aside { class: "notes-side note-rail", aria_label: "笔记本目录",
            NotebookBack {}
            if let Some(context) = context.as_ref() {
                div { class: "note-rail-book",
                    p { class: "notes-side-label", "笔记本" }
                    Link { class: "note-rail-book-title", to: Route::NotebookDetail { id: context.notebook.id },
                        "{context.notebook.title}"
                    }
                    if !context.notebook.description.trim().is_empty() {
                        p { class: "note-rail-book-description", "{context.notebook.description}" }
                    }
                }
                NotebookDirectory { context: context.clone(), current: note.slug.clone(), toc_html: note.toc_html.clone(), spy: true }
            } else if !note.toc_html.is_empty() {
                div { class: "note-rail-book",
                    p { class: "notes-side-label", "本篇" }
                }
                NoteSections { toc_html: note.toc_html.clone(), spy: true }
            }
        }
    }
}

/// 笔记本目录的编号列表；当前篇浮起，其下展开本篇小节。
#[component]
fn NotebookDirectory(
    context: NoteContext,
    current: String,
    toc_html: String,
    spy: bool,
) -> Element {
    let book = context.notebook.id;
    rsx! {
        ol { class: "note-directory",
            for (index, item) in context.siblings.iter().enumerate() {
                li { key: "{item.slug}",
                    Link {
                        class: "note-directory-item",
                        to: note_route(&item.slug, Some(book)),
                        aria_current: (item.slug == current).then_some("page"),
                        span { class: "note-directory-number tabular-nums", aria_hidden: "true", {format!("{:02}", index + 1)} }
                        span { "{item.title}" }
                    }
                    if item.slug == current && !toc_html.is_empty() {
                        NoteSections { toc_html: toc_html.clone(), spy }
                    }
                }
            }
        }
    }
}

/// 本篇小节（目录 HTML 的一级）。`spy` 为真时挂上 scroll-spy；抽屉里的副本不挂。
#[component]
fn NoteSections(toc_html: String, spy: bool) -> Element {
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        if let (true, Some(window)) = (spy, web_sys::window()) {
            crate::utils::js::invoke_optional_global(
                &window,
                "__initTocSidebar",
                &["#note-rail-toc".into(), "#note-content".into()],
            );
        }
    });
    #[cfg(target_arch = "wasm32")]
    use_drop(move || {
        if let (true, Some(window)) = (spy, web_sys::window()) {
            crate::utils::js::invoke_optional_global(
                &window,
                "__disposeTocSidebar",
                &["#note-rail-toc".into()],
            );
        }
    });
    rsx! {
        nav {
            id: spy.then_some("note-rail-toc"),
            class: "note-sections",
            aria_label: "本篇小节",
            div { class: "toc-sidebar-body", dangerous_inner_html: "{toc_html}" }
        }
    }
}

/// 笔记正文：类型与篇序、衬线标题、时间与标签、正文，以及笔记本内的上一篇 / 下一篇。
/// 后台编辑器的预览（`preview`）只显示正文部分。
#[component]
pub fn NoteDocument(
    note: Note,
    #[props(default)] preview: bool,
    #[props(default)] context: Option<NoteContext>,
) -> Element {
    let title = note.display_title();
    let created = note.created_at.format("%Y.%m.%d").to_string();
    let updated = note.updated_at.format("%m.%d").to_string();
    let book = context.as_ref().map(|c| c.notebook.id);
    let mut sheet_open = use_signal(|| false);
    rsx! {
        if !preview {
            document::Title { "{title} · 笔记" }
        }
        article { class: "note-document",
            if !preview {
                if let Some(context) = context.as_ref() {
                    button {
                        id: "note-directory-trigger",
                        class: "note-mobile-entry",
                        r#type: "button",
                        aria_haspopup: "dialog",
                        aria_expanded: "{sheet_open}",
                        aria_controls: "note-directory-sheet",
                        onclick: move |_| sheet_open.set(true),
                        Icon { name: IconName::BookOpen, size: 15 }
                        span { class: "tabular-nums", "{context.notebook.title} · 第 {context.position} / {context.total} 篇" }
                        span { class: "note-mobile-entry-action", "目录" }
                    }
                } else if !note.toc_html.is_empty() {
                    button {
                        id: "note-directory-trigger",
                        class: "note-mobile-entry",
                        r#type: "button",
                        aria_haspopup: "dialog",
                        aria_expanded: "{sheet_open}",
                        aria_controls: "note-directory-sheet",
                        onclick: move |_| sheet_open.set(true),
                        Icon { name: IconName::List, size: 15 }
                        span { "本篇小节" }
                        span { class: "note-mobile-entry-action", "目录" }
                    }
                }
            }
            div { class: "note-document-kicker",
                span { class: "badge", "{note.kind.label()}" }
                if let Some(context) = context.as_ref() {
                    span { class: "tabular-nums", "第 {context.position} / {context.total} 篇" }
                }
            }
            h1 { class: "note-document-title", "{title}" }
            p { class: "note-document-meta tabular-nums",
                span { "记录于 {created}" }
                if note.updated_at.date_naive() != note.created_at.date_naive() {
                    span { "更新于 {updated}" }
                }
                if note.sections > 0 {
                    span { "{note.sections} 节" }
                }
            }
            if !note.tags.is_empty() {
                p { class: "note-document-tags",
                    for tag in note.tags.iter() {
                        span { key: "{tag}", "#{tag}" }
                    }
                }
            }
            div { id: "note-content", class: "note-document-body",
                PostContent { content_html: note.content_html.clone() }
            }
            if let Some(context) = context.as_ref().filter(|c| c.prev.is_some() || c.next.is_some()) {
                footer { class: "note-continue",
                    p { class: "note-continue-label", "在《{context.notebook.title}》中继续" }
                    nav { class: "note-continue-links", aria_label: "同一笔记本的上一篇与下一篇",
                        if let Some(prev) = context.prev.as_ref() {
                            Link { class: "note-continue-link", to: note_route(&prev.slug, book),
                                span { class: "note-continue-dir tabular-nums", {format!("← {:02}", context.position - 1)} }
                                span { class: "note-continue-title", "{prev.title}" }
                            }
                        } else {
                            span {}
                        }
                        if let Some(next) = context.next.as_ref() {
                            Link { class: "note-continue-link is-next", to: note_route(&next.slug, book),
                                span { class: "note-continue-dir tabular-nums", {format!("{:02} →", context.position + 1)} }
                                span { class: "note-continue-title", "{next.title}" }
                            }
                        }
                    }
                }
            }
        }
        if !preview {
            BottomSheet {
                open: sheet_open,
                id: "note-directory-sheet",
                trigger_id: "note-directory-trigger",
                label: "笔记本目录",
                NoteSheetBody { context, current: note.slug.clone(), toc_html: note.toc_html.clone() }
            }
        }
    }
}

/// 手机抽屉里的目录：选中一篇或一个小节后收起。锚点点击会被全局处理拦下，收不到
/// click，所以改听 pointerup 与 Enter。
#[component]
fn NoteSheetBody(context: Option<NoteContext>, current: String, toc_html: String) -> Element {
    let sheet = use_sheet_close();
    rsx! {
        div {
            class: "note-sheet",
            onpointerup: move |_| sheet.close(),
            onkeydown: move |event| {
                if event.key() == Key::Enter {
                    sheet.close();
                }
            },
            if let Some(context) = context {
                p { class: "note-sheet-title", "{context.notebook.title}" }
                NotebookDirectory { context, current, toc_html, spy: false }
            } else {
                p { class: "note-sheet-title", "本篇小节" }
                NoteSections { toc_html, spy: false }
            }
        }
    }
}

// ======================= 骨架屏 =======================

/// 通用笔记占位（后台笔记列表与编辑器也在用）。
#[component]
pub fn NotesSkeleton() -> Element {
    rsx! {
        div { class: "notes-skeleton", role: "status", aria_label: "正在加载笔记",
            for i in 0..3 {
                SkeletonBox { key: "{i}", class: "h-20 w-full rounded-[10px]" }
            }
        }
    }
}

#[component]
fn NotesStreamSkeleton() -> Element {
    rsx! {
        div { class: "notes-stream-skeleton", aria_hidden: "true",
            SkeletonBox { class: "h-3 w-24" }
            for i in 0..2 {
                div { key: "{i}", class: "note-moment",
                    SkeletonBox { class: "h-3 w-10" }
                    div {
                        SkeletonBox { class: "h-4 w-full" }
                        SkeletonBox { class: "h-4 mt-2 w-3/4" }
                    }
                }
            }
            SkeletonBox { class: "h-36 w-full rounded-[10px] mt-3" }
        }
    }
}

/// 列表页整页占位：页头、左栏与笔记流，与真实布局对齐。
#[component]
pub fn NotesPageSkeleton() -> Element {
    rsx! {
        div { class: "notes", role: "status", aria_label: "正在加载笔记",
            div { class: "notes-header", aria_hidden: "true",
                div {
                    h1 { class: "notes-title", "笔记" }
                    SkeletonBox { class: "h-4 mt-3 w-72 max-w-full" }
                }
            }
            div { class: "notes-layout", aria_hidden: "true",
                div { class: "notes-side",
                    SkeletonBox { class: "h-[34px] w-full rounded-lg" }
                    div { class: "notes-side-group",
                        for i in 0..3 {
                            SkeletonBox { key: "{i}", class: "h-8 w-full rounded-lg mt-1" }
                        }
                    }
                }
                div { class: "notes-stream", NotesStreamSkeleton {} }
            }
        }
    }
}

#[component]
fn NotebookSkeleton() -> Element {
    rsx! {
        div { role: "status", aria_label: "正在加载笔记本",
            div { aria_hidden: "true",
                SkeletonBox { class: "h-3 w-20" }
                SkeletonBox { class: "h-9 mt-8 w-56" }
                SkeletonBox { class: "h-4 mt-4 w-full" }
                SkeletonBox { class: "h-10 mt-6 w-36 rounded-[10px]" }
                div { class: "notebook-entries",
                    for i in 0..4 {
                        div { key: "{i}", class: "notebook-entry",
                            SkeletonBox { class: "h-5 w-7" }
                            div {
                                SkeletonBox { class: "h-5 w-3/5" }
                                SkeletonBox { class: "h-3 mt-3 w-full" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 阅读页占位：左栏目录与正文。
#[component]
pub fn NoteReaderSkeleton() -> Element {
    rsx! {
        div { class: "notes-layout note-reader", role: "status", aria_label: "正在加载笔记",
            div { class: "notes-side note-rail", aria_hidden: "true",
                SkeletonBox { class: "h-3 w-20" }
                for i in 0..4 {
                    SkeletonBox { key: "{i}", class: "h-7 w-full rounded-lg mt-2" }
                }
            }
            div { class: "note-document", aria_hidden: "true",
                SkeletonBox { class: "h-5 w-24 rounded-full" }
                SkeletonBox { class: "h-9 mt-4 w-4/5" }
                SkeletonBox { class: "h-3 mt-4 w-48" }
                div { class: "flex flex-col gap-3 mt-10",
                    for i in 0..6 {
                        SkeletonBox { key: "{i}", class: "h-3.5 w-full" }
                    }
                }
            }
        }
    }
}

/// 路由切换时的整页占位：按目标路由选择。
#[component]
pub fn NotesRouteSkeleton(route: Route) -> Element {
    match route {
        Route::NoteDetail { .. } => rsx! { div { class: "notes", NoteReaderSkeleton {} } },
        Route::NotebookDetail { .. } => rsx! { div { class: "notebook", NotebookSkeleton {} } },
        _ => rsx! { NotesPageSkeleton {} },
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
        // 路由宏总会写出查询键；空值在阅读页按「未指明笔记本」处理。
        assert_eq!(note_route("a", None).to_string(), "/notes/a?book=");
        let parsed: Route = "/notes/a".parse().unwrap();
        assert_eq!(parsed, note_route("a", None));
    }
}
