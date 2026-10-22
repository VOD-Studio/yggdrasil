//! 笔记管理（`/admin/notes`）。
//!
//! 标题行（管理笔记本、写笔记）→ 快速记录 Composer（正文与标签，存为私密随记）→
//! 类型选项卡、搜索、公开状态 / 知识库 / 笔记本筛选与回收站开关 → 笔记行（类型字、
//! 标题、公开版本、未发布修改、知识库版本、笔记本与标签、最近修改时间）与底栏分页。
//! 回收站里的行改为「恢复为草稿」。筛选条件按历史记录保存。私人数据只在浏览器
//! 鉴权后读取。样式见 styles/pages/admin-notes.css。
mod notebooks;

use crate::components::admin_layout::AdminPageHead;
use chrono::{DateTime, Datelike, FixedOffset};
use dioxus::prelude::*;

use super::note_drafts::{clear_submitted, use_draft_protection};
use crate::api::notes::*;
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::forms::{FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::skeletons::notes_admin_skeleton::NoteRowsSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconName, LoadError,
    SegmentedControl, SegmentedItem, StatusDot, TablePager,
};
use crate::models::note::*;
use crate::router::Route;
use crate::utils::error::failure_message;
use crate::utils::time::to_local;
pub use notebooks::AdminNotebooks;

/// 列表接口每页固定 20 条。
const NOTES_PER_PAGE: i32 = 20;
/// 输入停顿多久后自动搜索（毫秒）。
const SEARCH_DEBOUNCE_MS: u32 = 300;

/// 私人数据仅在浏览器鉴权后读取，避免进入按 URL 共享的 SSR 缓存。
pub(super) async fn client_request<T>(request: impl std::future::Future<Output = T>) -> T {
    #[cfg(target_arch = "wasm32")]
    {
        request.await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        drop(request);
        std::future::pending().await
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
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn AdminNotes() -> Element {
    let entry_id = use_hook(crate::bridges::navigation::entry_id);
    let saved = use_hook(|| {
        crate::bridges::navigation::read_state::<NotesHistoryState>("admin-notes")
            .unwrap_or_default()
    });
    let toast = use_toast();
    let navigator = use_navigator();
    let mut filter = use_signal(|| NoteFilter {
        page: saved.filter.page.max(1),
        ..saved.filter.clone()
    });
    let mut search_input = use_signal(|| saved.search_input.clone());
    let mut search_seq = use_signal(|| 0_u64);
    let mut restoring = use_signal(|| None::<i32>);
    let mut response = use_resource(move || client_request(list_owned_notes(filter())));
    let books = use_resource(|| client_request(owned_notebooks()));

    use_effect(move || {
        crate::bridges::navigation::write_state(
            &entry_id,
            "admin-notes",
            &NotesHistoryState {
                filter: filter(),
                search_input: search_input(),
            },
        );
    });

    let mut submit_search = move || {
        let q = search_input.peek().trim().to_string();
        if q != filter.peek().query {
            filter.with_mut(|f| {
                f.query = q;
                f.page = 1;
            });
        }
    };
    let mut schedule_search = move || {
        search_seq += 1;
        let id = search_seq();
        spawn(async move {
            crate::utils::time::sleep_ms(SEARCH_DEBOUNCE_MS).await;
            if search_seq() == id {
                submit_search();
            }
        });
    };
    let restore = move |note: Note| {
        restoring.set(Some(note.id));
        spawn(async move {
            match act_on_note(note.id, note.version, NoteAction::Restore).await {
                Ok(_) => {
                    toast.success(format!("已把「{}」恢复为草稿", note.display_title()));
                    response.restart();
                }
                Err(e) => toast.error(failure_message("恢复失败", &e)),
            }
            restoring.set(None);
        });
    };

    let current = filter();
    let trash = current.trash;
    let filtering = !current.query.is_empty()
        || current.published.is_some()
        || current.knowledge.is_some()
        || current.notebook_id.is_some();
    let book_list = match books.read().as_ref() {
        Some(Ok(items)) => items.clone(),
        _ => Vec::new(),
    };
    let mut book_options: Vec<(Option<i32>, String)> = vec![(None, "笔记本".to_string())];
    book_options.extend(book_list.iter().map(|b| (Some(b.id), b.title.clone())));
    let now = crate::utils::time::local_now();

    rsx! {
        div { class: "admin-notes",
            AdminPageHead {
                title: "笔记",
                actions: rsx! {
                    Link {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                        to: Route::AdminNotebooks {},
                        "管理笔记本"
                    }
                    Link {
                        class: "{button_class(ButtonVariant::Primary, ButtonSize::Md)} admin-hide-mobile",
                        to: Route::NewNote {},
                        Icon { name: IconName::Plus, size: 16 }
                        "写笔记"
                    }
                },
                "捕捉此刻的想法，也为长久的知识留一个位置。"
            }

            QuickNote { on_saved: move |_| response.restart() }

            div { class: "admin-toolbar admin-notes-toolbar",
                if trash {
                    p { class: "admin-notes-trash-title",
                        Icon { name: IconName::Trash, size: 15 }
                        "回收站"
                        span { "恢复后为私密草稿，公开与收录需要重新确认。" }
                    }
                } else {
                    SegmentedControl {
                        aria_label: "笔记类型",
                        items: vec![
                            SegmentedItem::new("all".to_string(), "全部"),
                            SegmentedItem::new("moment".to_string(), "随记"),
                            SegmentedItem::new("topic".to_string(), "主题笔记"),
                        ],
                        value: kind_value(current.kind),
                        on_change: move |value: String| filter.with_mut(|f| {
                            f.kind = match value.as_str() {
                                "moment" => Some(NoteKind::Moment),
                                "topic" => Some(NoteKind::Topic),
                                _ => None,
                            };
                            f.page = 1;
                        }),
                    }
                }
                div { class: "admin-toolbar-filters",
                    label { class: "field field-sm admin-filter-search",
                        Icon { name: IconName::Search, size: 14 }
                        span { class: "sr-only", "搜索标题、正文或标签" }
                        input {
                            r#type: "search",
                            placeholder: "搜索标题、正文或标签",
                            value: "{search_input}",
                            oninput: move |e| {
                                search_input.set(e.value());
                                schedule_search();
                            },
                            onkeydown: move |e| {
                                if e.key() == Key::Enter && !e.data().is_composing() {
                                    search_seq += 1;
                                    submit_search();
                                } else if e.key() == Key::Escape && !search_input().is_empty() {
                                    e.prevent_default();
                                    search_input.set(String::new());
                                    search_seq += 1;
                                    submit_search();
                                }
                            },
                        }
                    }
                    if !trash {
                        FormSelect {
                            id: "admin-notes-published",
                            value: current.published,
                            options: vec![(None, "公开状态"), (Some(true), "已公开"), (Some(false), "私密")],
                            trigger_class: FORM_SELECT_COMPACT_CLASS,
                            aria_label: "按公开状态筛选",
                            onchange: move |value: Option<bool>| filter.with_mut(|f| {
                                f.published = value;
                                f.page = 1;
                            }),
                        }
                        FormSelect {
                            id: "admin-notes-knowledge",
                            value: current.knowledge,
                            options: vec![(None, "AI 知识库"), (Some(true), "已收录"), (Some(false), "未收录")],
                            trigger_class: FORM_SELECT_COMPACT_CLASS,
                            aria_label: "按知识库收录筛选",
                            onchange: move |value: Option<bool>| filter.with_mut(|f| {
                                f.knowledge = value;
                                f.page = 1;
                            }),
                        }
                        if book_options.len() > 1 {
                            FormSelect {
                                id: "admin-notes-notebook",
                                value: current.notebook_id,
                                options: book_options,
                                trigger_class: FORM_SELECT_COMPACT_CLASS,
                                aria_label: "按笔记本筛选",
                                onchange: move |value: Option<i32>| filter.with_mut(|f| {
                                    f.notebook_id = value;
                                    f.page = 1;
                                }),
                            }
                        }
                    }
                    button {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                        r#type: "button",
                        aria_pressed: "{trash}",
                        onclick: move |_| filter.with_mut(|f| {
                            f.trash = !f.trash;
                            f.page = 1;
                        }),
                        if trash {
                            Icon { name: IconName::ArrowLeft, size: 14 }
                            "返回笔记"
                        } else {
                            "回收站"
                        }
                    }
                }
            }

            match response.read().as_ref() {
                None => rsx! {
                    NoteRowsSkeleton {}
                },
                Some(Err(_)) => rsx! {
                    LoadError { title: "笔记列表加载失败", on_retry: move |_| response.restart() }
                },
                Some(Ok(page)) if page.notes.is_empty() => rsx! {
                    div { class: "admin-panel",
                        if filtering {
                            EmptyState {
                                icon: IconName::SearchX,
                                title: "没有符合条件的笔记",
                                description: "换个关键词，或清除筛选再试。",
                                action: EmptyStateAction {
                                    label: "清除筛选".to_string(),
                                    onclick: Callback::new(move |_| {
                                        search_input.set(String::new());
                                        filter.with_mut(|f| {
                                            *f = NoteFilter { kind: f.kind, trash: f.trash, ..Default::default() };
                                        });
                                    }),
                                },
                            }
                        } else if trash {
                            EmptyState {
                                icon: IconName::Trash,
                                title: "回收站是空的",
                                description: "移入回收站的笔记会出现在这里，可以随时恢复。",
                            }
                        } else {
                            EmptyState {
                                icon: IconName::NotebookPen,
                                title: "还没有笔记",
                                description: "从上面的快速记录开始，或者写一篇主题笔记。",
                                action: EmptyStateAction {
                                    label: "写笔记".to_string(),
                                    onclick: Callback::new(move |_| {
                                        navigator.push(Route::NewNote {});
                                    }),
                                },
                            }
                        }
                    }
                },
                Some(Ok(page)) => rsx! {
                    div { class: "admin-panel admin-notes-list",
                        ul {
                            for note in page.notes.iter().cloned() {
                                NoteRow {
                                    key: "{note.id}",
                                    when: short_when(to_local(note.updated_at), now),
                                    books: note
                                        .notebook_ids
                                        .iter()
                                        .filter_map(|id| book_list.iter().find(|b| b.id == *id))
                                        .map(|b| b.title.clone())
                                        .collect::<Vec<_>>(),
                                    restoring: restoring() == Some(note.id),
                                    on_restore: restore,
                                    note,
                                }
                            }
                        }
                        TablePager {
                            page: current.page.max(1),
                            total: page.total,
                            per_page: NOTES_PER_PAGE,
                            unit: "条",
                            on_change: move |page| filter.with_mut(|f| f.page = page),
                        }
                    }
                },
            }

            Link { class: "admin-fab", to: Route::NewNote {}, aria_label: "写笔记",
                Icon { name: IconName::Plus, size: 18 }
                "写笔记"
            }
        }
    }
}

/// 快速记录：正文与标签存为私密随记。正文未变时进入编辑器，保存期间继续输入则留在本页。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn QuickNote(on_saved: EventHandler<()>) -> Element {
    let toast = use_toast();
    let navigator = use_navigator();
    let mut quick = use_signal(String::new);
    let mut tags = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut last_saved = use_signal(|| None::<i32>);
    let (mut recovery, cache_failed) =
        use_draft_protection("quick".into(), quick, move || !quick().is_empty() || busy());
    let recovering = recovery().is_some();
    let empty = quick().trim().is_empty();

    let submit = move |ev: FormEvent| {
        ev.prevent_default();
        if busy() || recovery().is_some() || quick().trim().is_empty() {
            return;
        }
        busy.set(true);
        let content_md = quick();
        let tag_list = parse_tags(&tags());
        spawn(async move {
            let result = save_note(NoteDraft {
                content_md: content_md.clone(),
                tags: tag_list,
                ..Default::default()
            })
            .await;
            busy.set(false);
            match result {
                Ok(note) => {
                    tags.set(String::new());
                    let unchanged = quick.with_mut(|current| clear_submitted(current, &content_md));
                    if unchanged {
                        navigator.push(Route::EditNote { id: note.id });
                    } else {
                        last_saved.set(Some(note.id));
                        on_saved.call(());
                    }
                }
                Err(e) => toast.error(failure_message("保存失败", &e)),
            }
        });
    };

    rsx! {
        if let Some(local) = recovery() {
            div { class: "admin-notes-banner", role: "alert",
                Icon { name: IconName::RotateCcwClock, size: 16 }
                p { "这个标签页有尚未保存的快速记录。" }
                button {
                    class: button_class(ButtonVariant::Secondary, ButtonSize::Sm),
                    r#type: "button",
                    onclick: move |_| {
                        quick.set(local.clone());
                        recovery.set(None);
                    },
                    "恢复记录"
                }
                button {
                    class: button_class(ButtonVariant::Ghost, ButtonSize::Sm),
                    r#type: "button",
                    onclick: move |_| recovery.set(None),
                    "放弃"
                }
            }
        }
        form { class: "admin-notes-quick", onsubmit: submit,
            textarea {
                class: "admin-notes-quick-text",
                aria_label: "快速记录",
                rows: "2",
                disabled: recovering,
                placeholder: "有什么想记下的？一句话、一段代码，或一个刚发现的链接。",
                value: "{quick}",
                oninput: move |ev| quick.set(ev.value()),
                onkeydown: move |e| {
                    let m = e.modifiers();
                    if (m.meta() || m.ctrl()) && e.key() == Key::Enter {
                        e.prevent_default();
                        #[cfg(target_arch = "wasm32")]
                        if let Some(form) = web_sys::window()
                            .and_then(|w| w.document())
                            .and_then(|d| d.query_selector(".admin-notes-quick").ok().flatten())
                        {
                            use wasm_bindgen::JsCast;
                            if let Ok(form) = form.dyn_into::<web_sys::HtmlFormElement>() {
                                let _ = form.request_submit();
                            }
                        }
                    }
                },
            }
            div { class: "admin-notes-quick-foot",
                input {
                    class: "admin-notes-quick-tags",
                    aria_label: "随记标签",
                    placeholder: "标签：Rust, 排错, 读书",
                    disabled: recovering,
                    value: "{tags}",
                    oninput: move |ev| tags.set(ev.value()),
                }
                span { class: "admin-notes-quick-hint", "先存为私密草稿，稍后整理。" }
                button {
                    class: button_class(ButtonVariant::Primary, ButtonSize::Sm),
                    r#type: "submit",
                    disabled: busy() || recovering || empty,
                    if busy() { "保存中…" } else { "记下" }
                }
            }
        }
        if cache_failed() {
            p { class: "admin-notes-note", role: "alert",
                "浏览器无法保存本地恢复副本，请先保存到服务端再离开。"
            }
        }
        if let Some(id) = last_saved() {
            p { class: "admin-notes-note", role: "status",
                "上一条已保存，继续输入的内容已保留。"
                Link { to: Route::EditNote { id }, "查看已保存记录" }
            }
        }
    }
}

/// 笔记的一行。回收站里的行不进入编辑器，右侧给出「恢复为草稿」。
#[component]
fn NoteRow(
    note: Note,
    when: String,
    books: Vec<String>,
    restoring: bool,
    on_restore: EventHandler<Note>,
) -> Element {
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
    let meta = rsx! {
        span { class: "admin-notes-meta",
            if let Some(v) = note.published_version {
                StatusDot { label: "公开 v{v}", tone: "clay" }
            } else {
                StatusDot { label: "私密", class: "admin-notes-private" }
            }
            if ahead {
                span { class: "admin-notes-ahead", "有未发布修改" }
            }
            if let Some(v) = note.knowledge_version {
                span { class: "admin-notes-pill", "知识库 v{v}" }
            }
            for book in books {
                span { key: "{book}", "《{book}》" }
            }
            if !tags.is_empty() {
                span { "{tags}" }
            }
        }
    };
    if note.deleted_at.is_some() {
        let deleted = note
            .deleted_at
            .map(|d| to_local(d).format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        return rsx! {
            li { class: "admin-notes-row", "aria-busy": restoring.then_some("true"),
                span { class: "admin-notes-glyph", aria_hidden: "true", "{glyph}" }
                span { class: "admin-notes-main",
                    span { class: "admin-notes-title", "{title}" }
                    {meta}
                }
                span { class: "admin-notes-side",
                    time { "删除于 {deleted}" }
                    button {
                        class: button_class(ButtonVariant::Secondary, ButtonSize::Sm),
                        r#type: "button",
                        disabled: restoring,
                        onclick: move |_| on_restore.call(note.clone()),
                        "恢复为草稿"
                    }
                }
            }
        };
    }
    rsx! {
        li {
            Link { class: "admin-notes-row", to: Route::EditNote { id: note.id },
                span { class: "admin-notes-glyph", aria_hidden: "true", "{glyph}" }
                span { class: "admin-notes-main",
                    span { class: "admin-notes-title", "{title}" }
                    {meta}
                }
                span { class: "admin-notes-side",
                    time { "{when}" }
                    span { class: "admin-notes-edit", aria_hidden: "true",
                        "编辑"
                        Icon { name: IconName::ArrowRight, size: 13 }
                    }
                }
            }
        }
    }
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
