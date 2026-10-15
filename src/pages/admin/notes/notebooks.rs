use super::client_request;
use crate::api::notes::*;
use crate::components::forms::INPUT_CLASS;
use crate::components::ui::{FilterTabs, BTN_PRIMARY_SM, BTN_SECONDARY as BTN_SECONDARY_SM};
use crate::models::note::*;
use crate::router::Route;
use dioxus::prelude::*;

#[component]
pub fn AdminNotebooks() -> Element {
    let mut books = use_resource(|| client_request(owned_notebooks()));
    let mut editing = use_signal(|| None::<Notebook>);
    let mut form_open = use_signal(|| false);
    let mut order_id = use_signal(|| None::<i32>);
    let mut order_open = use_signal(|| false);
    let mut archived = use_signal(|| false);
    let mut confirm_archive = use_signal(|| None::<i32>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);

    let mut close_order = move |()| {
        order_open.set(false);
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            crate::utils::time::sleep_ms(320).await;
            if !*order_open.peek() {
                order_id.set(None);
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        order_id.set(None);
    };

    let mut toggle_order = move |id: i32| {
        if order_open() && order_id() == Some(id) {
            close_order(());
        } else {
            form_open.set(false);
            order_id.set(Some(id));
            order_open.set(true);
        }
    };

    rsx! {
        div { class:"notes-admin",
            div {class:"notes-admin-heading",
                div {h1 {"笔记本"} p {"把相关的记录放在一起，按你的思路排列。"}}
                div {class:"notes-admin-actions",
                    Link {class:BTN_SECONDARY_SM,to:Route::AdminNotes {},"返回笔记"}
                    button {
                        class: BTN_PRIMARY_SM,
                        onclick: move |_| {
                            editing.set(None);
                            form_open.set(true);
                            if order_open() {
                                close_order(());
                            }
                        },
                        "＋ 新建笔记本"
                    }
                }
            }
            if form_open() {
                for key in std::iter::once(editing().map(|b|b.id).unwrap_or(0)) {
                    NotebookForm {key:"{key}",book:editing(),on_done:move |_| {form_open.set(false);books.restart();}}
                }
            }
            div {
                class: "notebook-order-panel",
                "data-open": "{order_open()}",
                inert: if order_open() { None } else { Some("") },
                div {
                    class: "overflow-hidden min-h-0",
                    if let Some(id) = order_id() {
                        NotebookOrder {
                            key: "{id}",
                            id,
                            on_close: move |_| close_order(()),
                        }
                    }
                }
            }
            FilterTabs {
                items: vec![("active","使用中"),("archived","已归档")],
                active_value: if archived(){"archived".to_string()}else{"active".to_string()},
                on_change: move |value: String| {
                    archived.set(value == "archived");
                    confirm_archive.set(None);
                    if order_open() {
                        close_order(());
                    }
                }
            }
            p {class:"notes-status my-3","归档隐藏公开目录，不删除笔记，也不撤回笔记自身的发布或 AI 授权。恢复后目录保持私密。"}
            if !error().is_empty(){p {class:"notes-error",role:"alert","{error}"}}
            div {class:"notes-admin-list",
                match books.read().as_ref() {
                    Some(Ok(items))=>rsx! {
                        if !items.iter().any(|b|b.archived_at.is_some()==archived()) {crate::components::empty_state::EmptyState {title:"这里还没有笔记本",description:"新建一个主题，或切换使用中和已归档列表。"}}
                        for book in items.iter().filter(|b|b.archived_at.is_some()==archived()) {
                            div {class:"notes-admin-row",key:"{book.id}",
                                div {class:"notes-admin-row-main",
                                    h3 {"{book.title}"}
                                    p {class:"notes-admin-row-meta","{book.note_count} 篇 · "
                                        span {class:if book.is_public {"notes-admin-row-public"} else {"notes-admin-row-private"},if book.is_public {"公开笔记本"} else {"私密笔记本"}}
                                    }
                                    if !book.description.trim().is_empty() {p {class:"notes-admin-row-description","{book.description}"}}
                                }
                                div {class:"notes-admin-actions notes-admin-row-actions",
                                    button {
                                        class: BTN_SECONDARY_SM,
                                        disabled: busy(),
                                        onclick: {
                                            let book = book.clone();
                                            move |_| {
                                                editing.set(Some(book.clone()));
                                                form_open.set(true);
                                                if order_open() {
                                                    close_order(());
                                                }
                                            }
                                        },
                                        "编辑"
                                    }
                                    button {
                                        class: if order_open() && order_id() == Some(book.id) {
                                            format!("{BTN_SECONDARY_SM} ring-2 ring-[var(--color-paper-accent)] text-[var(--color-paper-primary)]")
                                        } else {
                                            BTN_SECONDARY_SM.to_string()
                                        },
                                        disabled: busy(),
                                        onclick: {
                                            let id = book.id;
                                            move |_| toggle_order(id)
                                        },
                                        "编排目录"
                                    }
                                    if archived() || confirm_archive()==Some(book.id) {
                                        button {class:BTN_SECONDARY_SM,disabled:busy(),onclick:{let id=book.id;move |_|async move {
                                            busy.set(true);error.set(String::new());
                                            match archive_notebook(id,!archived()).await {
                                                Ok(())=>{
                                                    confirm_archive.set(None);
                                                    form_open.set(false);
                                                    if order_id() == Some(id) {
                                                        close_order(());
                                                    }
                                                    books.restart();
                                                },
                                                Err(e)=>error.set(e.to_string())
                                            };
                                            busy.set(false);
                                        }},if archived(){"恢复笔记本"}else{"确认归档"}}
                                        if !archived(){button {class:"note-read",onclick:move |_|confirm_archive.set(None),"取消"}}
                                    } else {button {class:"note-read",disabled:busy(),onclick:{let id=book.id;move |_|confirm_archive.set(Some(id))},"归档"}}
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
    let archived = book.as_ref().is_some_and(|b| b.archived_at.is_some());
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
            fieldset {disabled:archived,
                label {class:"flex items-center gap-2",crate::components::ui::Checkbox {checked:public(),onchange:move |v|public.set(v)} "公开展示这个笔记本（已归档时须先恢复；其中仅已发布的笔记对访客可见）"}
            }
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
        section {
            class: "notes-quick animate-notebook-order-enter",
            aria_label: "编排笔记本目录",
            h2 { class: "font-semibold mb-3", "编排目录" }
            p { class: "notes-status mb-3", "上移或下移调整阅读顺序，保存后生效。" }
            if let Some(Err(e)) = response.read().as_ref() {
                p { class: "notes-error", role: "alert", "{e}" }
            }
            if response.read().is_none() {
                p { class: "notes-status animate-pulse", role: "status", "正在加载目录…" }
            }
            for (index, (note_id, title)) in rows().iter().enumerate() {
                div {
                    class: "flex justify-between items-center gap-3 py-2 transition-all duration-150",
                    key: "{note_id}",
                    span { "{title}" }
                    div {
                        class: "notes-admin-actions",
                        button {
                            r#type: "button",
                            disabled: index == 0,
                            aria_label: "上移",
                            onclick: move |_| rows.with_mut(|r| r.swap(index, index - 1)),
                            "↑"
                        }
                        button {
                            r#type: "button",
                            disabled: index + 1 == rows().len(),
                            aria_label: "下移",
                            onclick: move |_| rows.with_mut(|r| r.swap(index, index + 1)),
                            "↓"
                        }
                    }
                }
            }
            if !error().is_empty() {
                p { class: "notes-error", role: "alert", "{error}" }
            }
            div {
                class: "notes-admin-actions mt-4",
                button {
                    class: BTN_PRIMARY_SM,
                    disabled: busy() || !initialized(),
                    onclick: move |_| {
                        busy.set(true);
                        let ids = rows().iter().map(|r| r.0).collect();
                        spawn(async move {
                            match reorder_notebook(id, ids).await {
                                Ok(()) => on_close.call(()),
                                Err(e) => error.set(e.to_string()),
                            };
                            busy.set(false);
                        });
                    },
                    "保存顺序"
                }
                button {
                    class: BTN_SECONDARY_SM,
                    onclick: move |_| on_close.call(()),
                    "关闭"
                }
            }
        }
    }
}
