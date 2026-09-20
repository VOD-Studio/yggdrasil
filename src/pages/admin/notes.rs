//! 笔记管理与笔记本编辑。
use crate::api::notes::*;
use crate::components::forms::INPUT_CLASS;
use crate::components::ui::{FilterTabs, BTN_PRIMARY_SM, BTN_SECONDARY as BTN_SECONDARY_SM};
use crate::models::note::*;
use crate::router::Route;

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
            form { class: "notes-quick", onsubmit: move |ev| {
                ev.prevent_default();
                if busy() || quick().trim().is_empty() {return;}
                busy.set(true); error.set(String::new());
                let content_md=quick();
                spawn(async move {
                    match save_note(NoteDraft {content_md,..Default::default()}).await {
                        Ok(note) => {quick.set(String::new()); navigator.push(Route::EditNote {id:note.id});},
                        Err(e) => error.set(e.to_string()),
                    }
                    busy.set(false);
                });
            },
                textarea { aria_label: "快速记录", placeholder: "有什么想记下的？一句话、一段代码，或一个刚发现的链接。", value: "{quick}", oninput: move |ev| quick.set(ev.value()) }
                footer { span { "先存为私密草稿，稍后整理。" } button { class: BTN_PRIMARY_SM, r#type: "submit", disabled: busy() || quick().trim().is_empty(), if busy() { "保存中…" } else { "记下来 →" } } }
            }
            FilterTabs { items: vec![("all","全部"),("moment","随记"),("topic","主题笔记"),("trash","回收站")],
                active_value: if filter().trash {"trash".to_string()} else {filter().kind.map(|k|k.as_str()).unwrap_or("all").to_string()},
                on_change: move |value: String| filter.with_mut(|f| {f.trash=value=="trash";f.kind=match value.as_str(){"moment"=>Some(NoteKind::Moment),"topic"=>Some(NoteKind::Topic),_=>None};f.page=1;})
            }
            form { class: "notes-search", onsubmit: move |ev| {ev.prevent_default();filter.with_mut(|f| {f.query=query();f.page=1;});},
                input { r#type: "search", aria_label: "筛选笔记", placeholder: "搜索标题、正文或标签", value: "{query}", oninput: move |ev| query.set(ev.value()) }
                button { r#type: "submit", "搜索" }
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

#[component]
pub fn AdminNotebooks() -> Element {
    let mut books = use_resource(|| client_request(owned_notebooks()));
    let mut editing = use_signal(|| None::<Notebook>);
    let mut form_open = use_signal(|| false);
    let mut order = use_signal(|| None::<i32>);
    rsx! {
        div { class:"notes-admin",
            div {class:"notes-admin-heading",
                div {h1 {"笔记本"} p {"把相关的记录放在一起，按你的思路排列。"}}
                div {class:"notes-admin-actions",
                    Link {class:BTN_SECONDARY_SM,to:Route::AdminNotes {},"返回笔记"}
                    button {class:BTN_PRIMARY_SM,onclick:move |_| {editing.set(None);form_open.set(true);},"＋ 新建笔记本"}
                }
            }
            if form_open() {
                for key in std::iter::once(editing().map(|b|b.id).unwrap_or(0)) {
                    NotebookForm {key:"{key}",book:editing(),on_done:move |_| {form_open.set(false);books.restart();}}
                }
            }
            if let Some(id)=order() {
                for key in std::iter::once(id) {NotebookOrder {key:"{key}",id,on_close:move |_|order.set(None)}}
            }
            div {class:"notes-admin-list",
                match books.read().as_ref() {
                    Some(Ok(items))=>rsx! {
                        if items.is_empty() {crate::components::empty_state::EmptyState {title:"还没有笔记本",description:"为你的第一个主题取个名字。"}}
                        for book in items {
                            div {class:"notes-admin-row",key:"{book.id}",
                                div {h3 {"{book.title}"} p {"{book.note_count} 篇 · " if book.is_public {"公开笔记本"} else {"私密笔记本"}} p {"{book.description}"}}
                                div {class:"notes-admin-actions",
                                    button {class:BTN_SECONDARY_SM,onclick:{let book=book.clone();move |_| {editing.set(Some(book.clone()));form_open.set(true);}},"编辑"}
                                    button {class:BTN_SECONDARY_SM,onclick:{let id=book.id;move |_|order.set(Some(id))},"编排目录"}
                                }
                            }
                        }
                    },
                    Some(Err(e))=>rsx! {p {class:"notes-error",role:"alert","{e}"}},
                    None=>rsx! {crate::pages::notes::NotesSkeleton {}},
                }
            }
        }
    }
}

#[component]
fn NotebookForm(book: Option<Notebook>, on_done: EventHandler<()>) -> Element {
    let id = book.as_ref().map(|b| b.id);
    let mut title = use_signal(|| book.as_ref().map(|b| b.title.clone()).unwrap_or_default());
    let mut description = use_signal(|| {
        book.as_ref()
            .map(|b| b.description.clone())
            .unwrap_or_default()
    });
    let mut public = use_signal(|| book.as_ref().is_some_and(|b| b.is_public));
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    rsx! {
        form {class:"notes-quick space-y-4",onsubmit:move |ev| {ev.prevent_default();if busy(){return;}busy.set(true);spawn(async move {
            match save_notebook(NotebookInput {id,title:title(),description:description(),is_public:public()}).await {Ok(())=>on_done.call(()),Err(e)=>error.set(e.to_string())};busy.set(false);
        });},
            label {"名称" input {class:INPUT_CLASS,required:true,maxlength:100,value:"{title}",oninput:move |ev|title.set(ev.value())}}
            label {"介绍" textarea {class:INPUT_CLASS,maxlength:1000,value:"{description}",oninput:move |ev|description.set(ev.value())}}
            label {class:"flex items-center gap-2",crate::components::ui::Checkbox {checked:public(),onchange:move |v|public.set(v)} "公开展示这个笔记本（其中仅已发布的笔记对访客可见）"}
            if !error().is_empty() {p {class:"notes-error",role:"alert","{error}"}}
            div {class:"notes-admin-actions",button {class:BTN_PRIMARY_SM,r#type:"submit",disabled:busy(),"保存笔记本"} button {class:BTN_SECONDARY_SM,r#type:"button",onclick:move |_|on_done.call(()),"取消"}}
        }
    }
}

#[component]
fn NotebookOrder(id: i32, on_close: EventHandler<()>) -> Element {
    let response = use_resource(move || client_request(notebook_order(id)));
    let mut rows = use_signal(Vec::<(i32, String)>::new);
    let mut initialized = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut busy = use_signal(|| false);
    use_effect(move || {
        if !initialized() {
            if let Some(Ok(items)) = response.read().as_ref() {
                rows.set(items.clone());
                initialized.set(true);
            }
        }
    });
    rsx! {
        section {class:"notes-quick",aria_label:"编排笔记本目录",
            h2 {class:"font-semibold mb-3","编排目录"}
            p {class:"notes-status mb-3","上移或下移调整阅读顺序，保存后生效。"}
            if let Some(Err(e))=response.read().as_ref(){p {class:"notes-error",role:"alert","{e}"}}
            if response.read().is_none(){p {role:"status","正在加载目录…"}}
            for (index,(note_id,title)) in rows().iter().enumerate() {
                div {class:"flex justify-between gap-3 py-2",key:"{note_id}",span {"{title}"}
                    div {class:"notes-admin-actions",
                        button {r#type:"button",disabled:index==0,aria_label:"上移",onclick:move |_|rows.with_mut(|r|r.swap(index,index-1)),"↑"}
                        button {r#type:"button",disabled:index+1==rows().len(),aria_label:"下移",onclick:move |_|rows.with_mut(|r|r.swap(index,index+1)),"↓"}
                    }
                }
            }
            if !error().is_empty() {p {class:"notes-error",role:"alert","{error}"}}
            div {class:"notes-admin-actions mt-4",
                button {class:BTN_PRIMARY_SM,disabled:busy() || !initialized(),onclick:move |_| {busy.set(true);let ids=rows().iter().map(|r|r.0).collect();spawn(async move {match reorder_notebook(id,ids).await {Ok(())=>on_close.call(()),Err(e)=>error.set(e.to_string())};busy.set(false);});},"保存顺序"}
                button {class:BTN_SECONDARY_SM,onclick:move |_|on_close.call(()),"关闭"}
            }
        }
    }
}
