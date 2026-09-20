//! 数字花园的笔记入口：随记时间流、主题笔记和有序笔记本。
use crate::api::notes::{get_note, list_notebooks, list_notes};
use crate::components::empty_state::EmptyState;
use crate::components::post::{post_content::PostContent, post_toc::PostToc};
use crate::models::note::*;
use crate::router::Route;
use dioxus::prelude::*;

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
    let initial = use_hook(|| {
        crate::bridges::navigation::read_state::<NoteFilter>("notes").unwrap_or_default()
    });
    let mut filter = use_signal(|| initial.clone());
    let mut query = use_signal(|| initial.query.clone());
    let mut books_only = use_signal(|| false);
    use_effect(move || crate::bridges::navigation::write_state(&entry_id, "notes", &filter()));
    let books = use_server_future(list_notebooks)?;
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
        div { class: "notes-tabs", aria_label: "笔记类型",
            for (kind,label) in [(None,"全部"),(Some(NoteKind::Moment),"随记"),(Some(NoteKind::Topic),"主题笔记")] {
                button { r#type: "button", aria_pressed: !books_only() && filter().kind==kind,
                    class: if !books_only() && filter().kind==kind { "active" } else { "" },
                    onclick: move |_| { books_only.set(false); filter.with_mut(|f| {f.kind=kind; f.page=1;}); }, "{label}"
                }
            }
            button { r#type: "button", aria_pressed: books_only(), class: if books_only() { "active" } else { "" }, onclick: move |_| books_only.set(true), "笔记本" }
        }
        if let Some(Ok(items)) = books.read().as_ref() {
            if !items.is_empty() && (books_only() || (filter().kind.is_none() && filter().query.is_empty() && filter().tag.is_empty())) {
                section { class: "notebook-section", aria_label: "笔记本",
                    div { class: "notes-section-heading", h2 { "沿着一个主题" } span { "把零散的发现，放在一起。" } }
                    div { class: "notebook-grid",
                        for book in items.iter().take(if books_only() {usize::MAX} else {4}) {
                            NotebookCard { key: "{book.id}", book: book.clone() }
                        }
                    }
                }
            } else if books_only() {
                EmptyState { title: "笔记本正在生长", description: "整理好的主题会出现在这里。" }
            }
        }
        if !books_only() {
            if !filter().tag.is_empty() {
                button { class: "note-tag", onclick: move |_| filter.with_mut(|f| {f.tag.clear();f.page=1;}), "#{filter().tag} ×" }
            }
            SuspenseBoundary { fallback: |_| rsx! { NotesSkeleton {} },
                NoteFeed { filter, on_tag: move |tag| filter.with_mut(|f| {f.tag=tag;f.page=1;}) }
            }
        }
        p { class: "notes-colophon", "想法不必完整，也值得被记下。" }
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

#[component]
fn NoteFeed(mut filter: Signal<NoteFilter>, on_tag: EventHandler<String>) -> Element {
    let response = use_server_future(move || list_notes(filter()))?;
    let data = response.read();
    rsx! {
        section { class: "note-feed", aria_label: "笔记列表",
            div { class: "notes-section-heading", h2 { if !filter().query.is_empty() { "搜索结果" } else if filter().notebook_id.is_some() { "笔记目录" } else { "最近记录" } }
                if let Some(Ok(page)) = data.as_ref() { span { "{page.total} 条记录" } }
            }
            match data.as_ref() {
                Some(Ok(page)) if !page.notes.is_empty() => rsx! {
                    for note in &page.notes { NoteEntry { key: "{note.id}-{note.version}", note: note.clone(), on_tag } }
                    div { class: "notes-pagination",
                        button { disabled: filter().page<=1, onclick: move |_| filter.with_mut(|f| f.page=(f.page-1).max(1)), "← 上一页" }
                        span { "第 {filter().page.max(1)} 页" }
                        button { disabled: i64::from(filter().page.max(1))*20>=page.total, onclick: move |_| filter.with_mut(|f| f.page=f.page.max(1)+1), "下一页 →" }
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
    let books = use_server_future(list_notebooks)?;
    let mut filter = use_signal(|| NoteFilter {
        notebook_id: Some(id),
        ..Default::default()
    });
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
        if !filter().tag.is_empty() {
            button {class:"note-tag",onclick:move |_|filter.with_mut(|f|{f.tag.clear();f.page=1;}),"#{filter().tag} ×"}
        }
        NoteFeed { filter, on_tag: move |tag| filter.with_mut(|f| {f.tag=tag;f.page=1;}) }
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
pub fn NoteDocument(note: Note, #[props(default)] preview: bool) -> Element {
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
