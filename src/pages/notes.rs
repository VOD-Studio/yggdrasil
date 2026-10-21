//! 数字花园的笔记入口：随记时间流、主题笔记和有序笔记本。
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

use crate::api::notes::{get_note, list_notebooks, list_notes};
use crate::components::empty_state::EmptyState;
use crate::components::post::{post_content::PostContent, post_toc::PostToc};
use crate::models::note::*;
use crate::models::public_note::PublicNote;
use crate::router::Route;
use dioxus::prelude::*;

/// 笔记类型标签；其后的「笔记本」标签只切换到目录，不改变列表筛选。
const KIND_TABS: [(Option<NoteKind>, &str); 3] = [
    (None, "全部"),
    (Some(NoteKind::Moment), "随记"),
    (Some(NoteKind::Topic), "主题笔记"),
];
const BOOKS_TAB: usize = KIND_TABS.len();

fn kind_tab(kind: Option<NoteKind>) -> usize {
    KIND_TABS.iter().position(|(k, _)| *k == kind).unwrap_or(0)
}

#[component]
pub fn Notes() -> Element {
    rsx! {
        document::Title { "笔记 · Yggdrasil" }
        div { class: "notes-page",
            header { class: "notes-hero",
                div {
                    p { class: "notes-eyebrow", "FIELD NOTES / 生长中的想法" }
                    h1 { "随手记下，" span { "慢慢成形。" } }
                    p { class: "notes-intro", "一些此刻的发现，一些反复琢磨的答案。" }
                }
                div { class: "notes-hero-mark", aria_hidden: "true",
                    svg { view_box: "0 0 120 140", fill: "none", stroke: "currentColor", stroke_width: "1.3",
                        path { d: "M58 130V38M58 100C26 104 13 85 14 63C40 61 58 74 58 100ZM59 77C88 77 103 56 101 32C74 35 58 48 59 77ZM58 48C43 37 42 20 52 8C66 20 72 36 58 48Z" }
                        path { d: "M35 131H82M58 100L29 78M59 77L88 46", opacity: "0.5" }
                    }
                    span { "保持好奇 / KEEP GROWING" }
                }
            }
            NoteGarden {}
        }
    }
}

#[component]
fn NoteGarden() -> Element {
    let entry_id = use_hook(crate::bridges::navigation::entry_id);
    // 页码从 1 开始，标签切换写入的筛选才能与挂载时的预取筛选相等。
    let initial = use_hook(|| {
        let mut filter =
            crate::bridges::navigation::read_state::<NoteFilter>("notes").unwrap_or_default();
        filter.page = filter.page.max(1);
        filter
    });
    let mut filter = use_signal(|| initial.clone());
    let mut query = use_signal(|| initial.query.clone());
    let mut books_only = use_signal(|| false);
    use_effect(move || crate::bridges::navigation::write_state(&entry_id, "notes", &filter()));
    // SSR 只轮询正在挂起的任务，预取须逐个等待；提前发出的第二个请求会丢失唤醒。
    let books = use_server_future(list_notebooks)?;
    let notes = use_note_page(filter)?;
    let shown = notes.read();
    let motion = use_hook(|| {
        Rc::new(RefCell::new(PanelMotion::new(PanelView::Feed(
            shown.filter.clone(),
        ))))
    });
    let requested = filter();
    let loading = shown.filter != requested;
    // 笔记本目录无需请求，立即切换；回到笔记列表时等结果返回再整体替换，期间保留当前面板。
    let view = if books_only() {
        PanelView::Books
    } else if loading {
        motion.borrow().view.clone()
    } else {
        PanelView::Feed(shown.filter.clone())
    };
    let books_view = view == PanelView::Books;
    let (swap, direction) = motion.borrow_mut().show(view);
    let all_notes =
        shown.filter.kind.is_none() && shown.filter.query.is_empty() && shown.filter.tag.is_empty();
    let active_tab = if books_only() {
        BOOKS_TAB
    } else {
        kind_tab(requested.kind)
    };
    rsx! {
        form { class: "notes-search", onsubmit: move |ev| {
            ev.prevent_default();
            filter.with_mut(|f| { f.query=query(); f.page=1; });
            books_only.set(false);
        },
            span { aria_hidden: "true", "⌕" }
            input { r#type: "search", aria_label: "搜索笔记", placeholder: "寻找一个想法、关键词或标签…", value: "{query}", oninput: move |ev| query.set(ev.value()) }
            button { r#type: "submit", "搜索" }
        }
        NoteTabs { active: active_tab, onselect: move |tab: usize| {
            books_only.set(tab == BOOKS_TAB);
            if let Some(&(kind, _)) = KIND_TABS.get(tab) {
                let current = filter.peek().clone();
                if current.kind != kind || current.page > 1 {
                    filter.with_mut(|f| { f.kind = kind; f.page = 1; });
                }
            }
        } }
        div { class: "notes-panel", "data-swap": swap, "data-direction": direction, aria_busy: loading && !books_view,
            if let Some(Ok(items)) = books.read().as_ref() {
                if !items.is_empty() && (books_view || all_notes) {
                    section { class: "notebook-section", aria_label: "笔记本",
                        div { class: "notes-section-heading", h2 { "沿着一个主题" } span { "把零散的发现，放在一起。" } }
                        div { class: "notebook-grid",
                            for book in items.iter().take(if books_view {usize::MAX} else {4}) {
                                NotebookCard { key: "{book.id}", book: book.clone() }
                            }
                        }
                    }
                } else if books_view {
                    EmptyState { title: "笔记本正在生长", description: "整理好的主题会出现在这里。" }
                }
            }
            if !books_view {
                if !shown.filter.tag.is_empty() {
                    button { class: "note-tag", onclick: move |_| filter.with_mut(|f| {f.tag.clear();f.page=1;}), "#{shown.filter.tag} ×" }
                }
                NoteFeed { filter, notes, on_tag: move |tag| filter.with_mut(|f| {f.tag=tag;f.page=1;}) }
            }
        }
        p { class: "notes-colophon", "想法不必完整，也值得被记下。" }
    }
}

/// 按钮下沿相对标签栏的位置，供独立下划线滑动。
#[derive(Clone, Copy, PartialEq)]
struct TabUnderline {
    x: f64,
    y: f64,
    width: f64,
}

/// 笔记类型标签。SSR 与测量前由当前按钮自带下划线；水合后测量按钮位置，改由独立下划线在标签间滑动。
#[component]
fn NoteTabs(active: usize, onselect: EventHandler<usize>) -> Element {
    let underline = use_signal(|| None::<TabUnderline>);
    #[cfg(target_arch = "wasm32")]
    let mut tabs = use_signal(|| None::<web_sys::Element>);
    #[cfg(target_arch = "wasm32")]
    use_effect(use_reactive((&active,), move |(active,)| {
        if let Some(tabs) = tabs.read().as_ref() {
            measure_underline(tabs, active, underline);
        }
    }));
    let labels = KIND_TABS.iter().map(|(_, label)| *label).chain(["笔记本"]);
    rsx! {
        div { class: "notes-tabs", role: "group", aria_label: "笔记类型",
            "data-ready": underline().is_some().then_some("true"),
            onmounted: move |_event| {
                #[cfg(target_arch = "wasm32")]
                tabs.set(_event.data().downcast::<web_sys::Element>().cloned());
            },
            // 视口宽度跨过断点会改变间距，字体加载也会改变按钮宽度。
            onresize: move |_| {
                #[cfg(target_arch = "wasm32")]
                if let Some(tabs) = tabs.peek().as_ref() {
                    measure_underline(tabs, active, underline);
                }
            },
            for (index, label) in labels.enumerate() {
                button { r#type: "button", aria_pressed: index == active,
                    class: if index == active { "active" } else { "" },
                    onclick: move |_| onselect.call(index), "{label}"
                }
            }
            if let Some(TabUnderline { x, y, width }) = underline() {
                span { class: "notes-tab-indicator", aria_hidden: "true",
                    style: "--tab-x: {x}px; --tab-y: {y}px; --tab-w: {width}px;"
                }
            }
        }
    }
}

/// 用视口坐标求按钮相对标签栏的位置，保留小数像素，与按钮自带下划线重合。
#[cfg(target_arch = "wasm32")]
fn measure_underline(
    tabs: &web_sys::Element,
    active: usize,
    mut underline: Signal<Option<TabUnderline>>,
) {
    use wasm_bindgen::JsCast;
    let Some(button) = tabs
        .query_selector_all("button")
        .ok()
        .and_then(|buttons| buttons.item(active as u32))
    else {
        return;
    };
    let Some(button) = button.dyn_ref::<web_sys::Element>() else {
        return;
    };
    let bar = tabs.get_bounding_client_rect();
    let rect = button.get_bounding_client_rect();
    let next = Some(TabUnderline {
        x: rect.left() - bar.left(),
        y: rect.bottom() - bar.top(),
        width: rect.width(),
    });
    if *underline.peek() != next {
        underline.set(next);
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
fn use_note_page(filter: Signal<NoteFilter>) -> Result<Memo<ShownNotes>, RenderError> {
    let first_filter = use_hook(|| filter.peek().clone());
    let first = use_server_future({
        let first_filter = first_filter.clone();
        move || list_notes(first_filter.clone())
    })?;
    // 挂载时的筛选直接使用预取结果；其它筛选才请求，重新开始时取消上一个请求。
    // 闭包每次渲染都会被替换，不能在闭包里保存状态。
    let next = use_resource({
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
    Ok(use_memo(move || match &*next.read() {
        Some(Some((filter, result))) => ShownNotes {
            filter: filter.clone(),
            result: Some(result.clone()),
        },
        _ => ShownNotes {
            filter: first_filter.clone(),
            result: first.read().clone(),
        },
    }))
}

/// 面板实际显示的视图：笔记本目录，或某个筛选的笔记列表。
#[derive(Clone, PartialEq)]
enum PanelView {
    Books,
    Feed(NoteFilter),
}

impl PanelView {
    /// 先比较标签顺序，再比较页码，决定进入方向。
    fn position(&self) -> (usize, i32) {
        match self {
            Self::Books => (BOOKS_TAB, 1),
            Self::Feed(filter) => (kind_tab(filter.kind), filter.page.max(1)),
        }
    }
}

/// 记录上一次显示的视图。内容替换时在同一次渲染中交替动画名，CSS 才会重播进入动效。
struct PanelMotion {
    view: PanelView,
    swaps: u32,
    direction: Ordering,
}

impl PanelMotion {
    fn new(view: PanelView) -> Self {
        Self {
            view,
            swaps: 0,
            direction: Ordering::Equal,
        }
    }

    /// 返回 `data-swap` 与 `data-direction`；首屏内容不播放进入动效。
    fn show(&mut self, view: PanelView) -> (Option<&'static str>, &'static str) {
        if view != self.view {
            self.direction = view.position().cmp(&self.view.position());
            self.swaps += 1;
            self.view = view;
        }
        let swap = match self.swaps {
            0 => None,
            n if n % 2 == 1 => Some("odd"),
            _ => Some("even"),
        };
        let direction = match self.direction {
            Ordering::Greater => "forward",
            Ordering::Less => "backward",
            Ordering::Equal => "none",
        };
        (swap, direction)
    }
}

#[component]
fn NotebookCard(book: Notebook) -> Element {
    rsx! {
        Link { class: "notebook-card", to: Route::NotebookDetail { id: book.id },
            span { class: "notebook-spine", aria_hidden: "true" }
            div { class: "notebook-top", span { "NOTEBOOK" } span { "↗" } }
            h3 { "{book.title}" }
            p { "{book.description}" }
            footer { span { "{book.note_count} 篇笔记" } span { "持续整理" } }
        }
    }
}

/// 渲染当前显示的结果；翻页基于显示中的页码，连续点击不会越过尚未返回的页面。
#[component]
fn NoteFeed(
    mut filter: Signal<NoteFilter>,
    notes: Memo<ShownNotes>,
    on_tag: EventHandler<String>,
) -> Element {
    let shown = notes.read();
    let ShownNotes {
        filter: current,
        result,
    } = &*shown;
    let page_no = current.page.max(1);
    rsx! {
        section { class: "note-feed", aria_label: "笔记列表",
            div { class: "notes-section-heading", h2 { if !current.query.is_empty() { "搜索结果" } else if current.notebook_id.is_some() { "笔记目录" } else { "最近记录" } }
                if let Some(Ok(page)) = result { span { "{page.total} 条记录" } }
            }
            match result {
                Some(Ok(page)) if !page.notes.is_empty() => rsx! {
                    for note in &page.notes { NoteEntry { key: "{note.id}-{note.version}", note: note.clone(), on_tag } }
                    div { class: "notes-pagination",
                        button { disabled: page_no<=1, onclick: move |_| filter.with_mut(|f| f.page=page_no-1), "← 上一页" }
                        span { "第 {page_no} 页" }
                        button { disabled: i64::from(page_no)*20>=page.total, onclick: move |_| filter.with_mut(|f| f.page=page_no+1), "下一页 →" }
                    }
                },
                Some(Ok(_)) => rsx! { EmptyState { title: "这里还没有笔记", description: "换个关键词看看，或等下一次灵感落下。" } },
                Some(Err(_)) => rsx! { p { role: "alert", "笔记暂时加载失败，请刷新重试。" } },
                None => rsx! { NotesSkeleton {} },
            }
        }
    }
}

#[component]
fn NoteEntry(note: Note, on_tag: EventHandler<String>) -> Element {
    let title = note.display_title();
    rsx! {
        article { class: "note-entry",
            div { class: "note-date", time { datetime: note.updated_at.to_rfc3339(), {note.updated_at.format("%m.%d").to_string()} } span { {note.updated_at.format("%Y").to_string()} } }
            div { class: "note-entry-body",
                div { class: "note-entry-top", span { class: "note-kind", "{note.kind.label()}" }
                    Link { class: "note-permalink", to: Route::NoteDetail { slug: note.slug.clone() }, aria_label: "阅读这条笔记", "↗" }
                }
                if !note.title.is_empty() { h3 { Link { to: Route::NoteDetail { slug: note.slug.clone() }, "{title}" } } }
                if note.kind==NoteKind::Moment && !note.content_html.is_empty() {
                    div { class: "note-moment-content", PostContent { content_html: note.content_html.clone() } }
                } else {
                    p { class: "note-summary", "{note.summary}" }
                    Link { class: "note-read", to: Route::NoteDetail { slug: note.slug.clone() }, "继续阅读 →" }
                }
                div { class: "note-tags", for tag in note.tags { button { class: "note-tag", onclick: move |_| on_tag.call(tag.clone()), "#{tag}" } } }
            }
        }
    }
}

#[component]
pub fn NotebookDetail(id: i32) -> Element {
    rsx! { for key in std::iter::once(id) { NotebookReader { key: "{key}", id: key } } }
}

#[component]
fn NotebookReader(id: i32) -> Element {
    let mut filter = use_signal(|| NoteFilter {
        notebook_id: Some(id),
        page: 1,
        ..Default::default()
    });
    let books = use_server_future(list_notebooks)?;
    let notes = use_note_page(filter)?;
    let shown = notes.read();
    let motion = use_hook(|| {
        Rc::new(RefCell::new(PanelMotion::new(PanelView::Feed(
            shown.filter.clone(),
        ))))
    });
    let loading = shown.filter != filter();
    let (swap, direction) = motion
        .borrow_mut()
        .show(PanelView::Feed(shown.filter.clone()));
    let book = books
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .and_then(|books| books.iter().find(|b| b.id == id))
        .cloned();
    let Some(book) = book else {
        return rsx! { EmptyState {title:"笔记本不存在",description:"这个笔记本尚未公开，或已经被移除。"} };
    };
    rsx! {
        document::Title { "{book.title} · 笔记本" }
        Link { class: "note-read", to: Route::Notes {}, "← 全部笔记" }
        header { class: "notes-hero notebook-heading", div {
            p { class: "notes-eyebrow", "NOTEBOOK / 主题索引" }
            h1 { "{book.title}" } p { class: "notes-intro", "{book.description}" }
        } }
        div { class: "notes-panel", "data-swap": swap, "data-direction": direction, aria_busy: loading,
            if !shown.filter.tag.is_empty() {
                button {class:"note-tag",onclick:move |_|filter.with_mut(|f|{f.tag.clear();f.page=1;}),"#{shown.filter.tag} ×"}
            }
            NoteFeed { filter, notes, on_tag: move |tag| filter.with_mut(|f| {f.tag=tag;f.page=1;}) }
        }
    }
}

#[component]
pub fn NoteDetail(slug: String) -> Element {
    rsx! { for key in std::iter::once(slug) { NoteReader { key: "{key}", slug: key } } }
}

#[component]
fn NoteReader(slug: String) -> Element {
    let note = use_server_future(move || get_note(slug.clone()))?;
    let data = note.read();
    match data.as_ref() {
        Some(Ok(note)) => rsx! { NoteDocument { note: note.clone() } },
        Some(Err(_)) => {
            rsx! { EmptyState { title: "这条笔记暂时无法阅读", description: "笔记可能尚未公开、已撤回，或服务暂时不可用。" } }
        }
        None => rsx! { NotesSkeleton {} },
    }
}

#[component]
pub fn NoteDocument(note: PublicNote, #[props(default)] preview: bool) -> Element {
    let title = note.display_title();
    rsx! {
        document::Title { "{title} · 笔记" }
        article { class: "note-document post-single",
            if !preview { Link { class: "note-read", to: Route::Notes {}, "← 回到笔记" } }
            header { class: "note-document-header",
                p { class: "notes-eyebrow", "{note.kind.label()} / FIELD NOTE" }
                h1 { "{title}" }
                p { class: "note-document-meta", {format!("更新于 {} · 版本 {}", note.updated_at.format("%Y-%m-%d"), note.version)} }
                div { class: "note-tags", for tag in &note.tags { span { class: "note-tag", "#{tag}" } } }
            }
            if !note.toc_html.is_empty() { PostToc { toc_html: note.toc_html.clone(), title: "本篇目录" } }
            PostContent { content_html: note.content_html.clone() }
            footer { class: "notes-colophon", {format!("记录于 {} · 想法仍在生长。", note.created_at.format("%Y-%m-%d"))} }
        }
    }
}

#[component]
pub fn NotesSkeleton() -> Element {
    rsx! { div { class: "notes-skeleton", role: "status", aria_label: "正在加载笔记",
        for i in 0..3 { div { key: "{i}", class: "notes-skeleton-line" } }
    } }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(kind: Option<NoteKind>, page: i32) -> PanelView {
        PanelView::Feed(NoteFilter {
            kind,
            page,
            ..Default::default()
        })
    }

    #[test]
    fn first_view_and_unchanged_views_do_not_replay_enter_motion() {
        let mut motion = PanelMotion::new(feed(None, 0));
        assert_eq!(motion.show(feed(None, 0)), (None, "none"));

        assert_eq!(motion.show(PanelView::Books), (Some("odd"), "forward"));
        // 后台请求期间保留同一视图，不应重复播放。
        assert_eq!(motion.show(PanelView::Books), (Some("odd"), "forward"));
    }

    #[test]
    fn each_replacement_alternates_animation_and_follows_tab_order() {
        let mut motion = PanelMotion::new(feed(Some(NoteKind::Topic), 1));
        assert_eq!(
            motion.show(feed(Some(NoteKind::Moment), 1)),
            (Some("odd"), "backward")
        );
        assert_eq!(motion.show(PanelView::Books), (Some("even"), "forward"));
        assert_eq!(motion.show(feed(None, 1)), (Some("odd"), "backward"));
    }

    #[test]
    fn pages_and_searches_within_a_tab_choose_direction_by_page() {
        let mut motion = PanelMotion::new(feed(None, 0));
        assert_eq!(motion.show(feed(None, 2)), (Some("odd"), "forward"));
        assert_eq!(motion.show(feed(None, 1)), (Some("even"), "backward"));
        let search = PanelView::Feed(NoteFilter {
            query: "rust".into(),
            page: 1,
            ..Default::default()
        });
        assert_eq!(motion.show(search), (Some("odd"), "none"));
    }
}
