//! 笔记列表、筛选与快速记录。
mod notebooks;
use super::note_drafts::{clear_submitted, use_draft_protection};
use crate::api::notes::*;
use crate::components::forms::{FormInput, FormSelect, INPUT_INLINE_CLASS};
use crate::components::ui::{FilterTabs, BTN_PRIMARY_SM, BTN_SECONDARY as BTN_SECONDARY_SM};
use crate::models::note::*;
use crate::router::Route;
pub use notebooks::AdminNotebooks;

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
use dioxus::prelude::*;

#[component]
pub fn AdminNotes() -> Element {
    let mut filter = use_signal(NoteFilter::default);
    let mut query = use_signal(String::new);
    let mut quick = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut last_saved = use_signal(|| None::<i32>);
    let (mut recovery, cache_failed) =
        use_draft_protection("quick".into(), quick, move || !quick().is_empty() || busy());
    let mut response = use_resource(move || client_request(list_owned_notes(filter())));
    let navigator = navigator();
    rsx! {
        div { class: "notes-admin",
            div { class: "notes-admin-heading",
                div { h1 { "笔记" } p { "捕捉此刻的想法，也为长久的知识留一个位置。" } }
                div { class: "notes-admin-actions",
                    Link { class: BTN_SECONDARY_SM, to: Route::AdminNotebooks {}, "管理笔记本" }
                    Link { class: BTN_PRIMARY_SM, to: Route::NewNote {}, "＋ 写笔记" }
                }
            }
            if !error().is_empty() { p { class: "notes-error", role: "alert", "{error}" } }
            if cache_failed() {p {class:"notes-error",role:"alert","浏览器无法保存本地恢复副本，请先保存到服务端再离开。"}}
            if let Some(local)=recovery() {
                div {class:"notes-error",role:"alert",p {"此标签页有尚未保存的快速记录。"}
                    button {class:BTN_SECONDARY_SM,onclick:move |_| {quick.set(local.clone());recovery.set(None);},"恢复记录"}
                    button {class:"note-read",onclick:move |_|recovery.set(None),"放弃恢复"}
                }
            }
            if let Some(id)=last_saved() {p {class:"notes-status",role:"status","上一条已保存，继续输入的内容已保留。 " Link {class:"note-read",to:Route::EditNote {id},"查看已保存记录 →"}}}
            form { class: "notes-quick", onsubmit: move |ev| {
                ev.prevent_default();
                if busy() || recovery().is_some() || quick().trim().is_empty() {return;}
                busy.set(true); error.set(String::new());
                let content_md=quick();
                spawn(async move {
                    let result=save_note(NoteDraft {content_md:content_md.clone(),..Default::default()}).await;
                    busy.set(false);
                    match result {
                        Ok(note) => {
                            let unchanged=quick.with_mut(|current|clear_submitted(current,&content_md));
                            if unchanged {navigator.push(Route::EditNote {id:note.id});}
                            else {last_saved.set(Some(note.id));response.restart();}
                        },
                        Err(e) => error.set(e.to_string()),
                    }
                });
            },
                textarea { aria_label: "快速记录", disabled:recovery().is_some(),placeholder: "有什么想记下的？一句话、一段代码，或一个刚发现的链接。", value: "{quick}", oninput: move |ev| quick.set(ev.value()) }
                footer { span { "先存为私密草稿，稍后整理。" } button { class: BTN_PRIMARY_SM, r#type: "submit", disabled: busy() || recovery().is_some() || quick().trim().is_empty(), if busy() { "保存中…" } else { "记下来 →" } } }
            }
            FilterTabs { items: vec![("all","全部"),("moment","随记"),("topic","主题笔记"),("trash","回收站")],
                active_value: if filter().trash {"trash".to_string()} else {filter().kind.map(|k|k.as_str()).unwrap_or("all").to_string()},
                on_change: move |value: String| filter.with_mut(|f| {f.trash=value=="trash";f.kind=match value.as_str(){"moment"=>Some(NoteKind::Moment),"topic"=>Some(NoteKind::Topic),_=>None};f.page=1;})
            }
            form {
                class: "flex items-center gap-2 mt-4",
                onsubmit: move |ev| {
                    ev.prevent_default();
                    filter.with_mut(|f| {
                        f.query = query();
                        f.page = 1;
                    });
                },
                FormInput {
                    r#type: "search",
                    aria_label: "筛选笔记",
                    placeholder: "搜索标题、正文或标签",
                    value: query(),
                    class: INPUT_INLINE_CLASS,
                    oninput: move |v: String| {
                        query.set(v.clone());
                        if v.is_empty() && !filter().query.is_empty() {
                            filter.with_mut(|f| {
                                f.query = String::new();
                                f.page = 1;
                            });
                        }
                    },
                }
                button {
                    class: "{BTN_PRIMARY_SM} shrink-0",
                    r#type: "submit",
                    "搜索"
                }
                if !filter().query.is_empty() {
                    button {
                        class: "{BTN_SECONDARY_SM} shrink-0",
                        r#type: "button",
                        onclick: move |_| {
                            query.set(String::new());
                            filter.with_mut(|f| {
                                f.query = String::new();
                                f.page = 1;
                            });
                        },
                        "清除"
                    }
                }
            }
            div { class: "notes-admin-filters mt-4",
                div { class: "notes-admin-filter",
                    label { r#for: "notes-published-filter", "公开状态" }
                    FormSelect {
                        id: Some("notes-published-filter".to_string()),
                        aria_label: "按公开状态筛选",
                        value: filter().published,
                        options: vec![(None, "全部"), (Some(true), "已公开"), (Some(false), "未公开（私密）")],
                        onchange: move |value: Option<bool>| filter.with_mut(|f| {
                            f.published = value;
                            f.page = 1;
                        }),
                    }
                }
                div { class: "notes-admin-filter",
                    label { r#for: "notes-knowledge-filter", "知识库状态" }
                    FormSelect {
                        id: Some("notes-knowledge-filter".to_string()),
                        aria_label: "按知识库状态筛选",
                        value: filter().knowledge,
                        options: vec![(None, "全部"), (Some(true), "已收录"), (Some(false), "未收录")],
                        onchange: move |value: Option<bool>| filter.with_mut(|f| {
                            f.knowledge = value;
                            f.page = 1;
                        }),
                    }
                }
            }
            div { class: "notes-admin-list mt-5",
                match response.read().as_ref() {
                    Some(Ok(page)) => rsx! {
                        if page.notes.is_empty() { crate::components::empty_state::EmptyState {title:"还没有笔记",description:"从上面的快速记录开始，或者写一篇主题笔记。"} }
                        for note in &page.notes {
                            div { class: "notes-admin-row", key: "{note.id}",
                                div { h3 { Link {to:Route::EditNote {id:note.id}, "{note.display_title()}"} }
                                    p { "{note.kind.label()} · "
                                        if note.deleted_at.is_some() { "回收站" } else if note.published_version.is_some() { "已公开" } else { "私密" }
                                        if note.knowledge_version.is_some() { " · 已收录知识库" }
                                        " · v{note.version}"
                                    }
                                }
                                div { class: "notes-admin-actions",
                                    if note.deleted_at.is_some() {
                                        button { class:BTN_SECONDARY_SM, onclick:{let n=note.clone(); move |_| {let n=n.clone();spawn(async move {match act_on_note(n.id,n.version,NoteAction::Restore).await {Ok(_)=>response.restart(),Err(e)=>error.set(e.to_string())}});}}, "恢复为草稿" }
                                    } else {
                                        Link { class:BTN_SECONDARY_SM, to:Route::EditNote {id:note.id}, "编辑 →" }
                                    }
                                }
                            }
                        }
                        div { class:"notes-pagination",
                            button { disabled:filter().page<=1, onclick:move |_|filter.with_mut(|f|f.page=(f.page-1).max(1)), "← 上一页" }
                            span {"共 {page.total} 条"}
                            button { disabled:i64::from(filter().page.max(1))*20>=page.total,onclick:move |_|filter.with_mut(|f|f.page=f.page.max(1)+1),"下一页 →"}
                        }
                    },
                    Some(Err(e))=>rsx! {p {class:"notes-error",role:"alert","{e}"}},
                    None=>rsx! {crate::pages::notes::NotesSkeleton {}},
                }
            }
        }
    }
}
