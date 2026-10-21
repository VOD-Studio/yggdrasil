//! 笔记本管理（`/admin/notebooks`）。
//!
//! 左栏：使用中 / 已归档选项卡与笔记本列表；右栏：选中笔记本的名称、简介、公开开关、
//! 目录顺序（上移下移）与归档，或新建笔记本的表单。名称、简介、公开与顺序一起保存，
//! 只提交改动过的部分。样式见 styles/pages/admin-notes.css。
use super::client_request;
use crate::api::notes::*;
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::forms::ToggleSwitch;
use crate::components::skeletons::notes_admin_skeleton::NotebooksSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, ConfirmDialog, Icon, IconButton, IconName,
    LoadError,
};
use crate::models::note::*;
use dioxus::prelude::*;

/// 右栏正在编辑的对象。
#[derive(Clone, Copy, PartialEq)]
enum Selection {
    Book(i32),
    New,
}

/// 新建表单的键，与编辑某一本（键为 id）不同，切换时重建表单。
const NEW_KEY: &str = "new";

/// 新建前已有的笔记本之外多出来的那本；列表还没刷新时为 `None`。
fn created(items: &[Notebook], known: &[i32]) -> Option<i32> {
    items.iter().map(|b| b.id).find(|id| !known.contains(id))
}

#[component]
pub fn AdminNotebooks() -> Element {
    let mut books = use_resource(|| client_request(owned_notebooks()));
    let mut archived = use_signal(|| false);
    let mut selection = use_signal(|| None::<Selection>);
    // 新建时记下已有的笔记本；列表刷新出新的那本后选中它。
    let mut known = use_signal(|| None::<Vec<i32>>);

    let list = match books.read().as_ref() {
        Some(Ok(items)) => Some(items.clone()),
        _ => None,
    };
    // 首次加载、切换选项卡或选中项消失时，选中当前选项卡的第一本。
    use_effect(move || {
        let Some(Ok(items)) = books.read().as_ref().cloned() else {
            return;
        };
        if let Some(before) = known() {
            // 还是刷新前的列表时保持新建表单。
            if let Some(id) = created(&items, &before) {
                known.set(None);
                archived.set(false);
                selection.set(Some(Selection::Book(id)));
            }
            return;
        }
        let shown: Vec<i32> = items
            .iter()
            .filter(|b| b.archived_at.is_some() == archived())
            .map(|b| b.id)
            .collect();
        let keep = match *selection.peek() {
            Some(Selection::New) => true,
            Some(Selection::Book(id)) => shown.contains(&id),
            None => false,
        };
        if !keep {
            selection.set(shown.first().copied().map(Selection::Book));
        }
    });

    let active_count = list
        .as_ref()
        .map_or(0, |l| l.iter().filter(|b| b.archived_at.is_none()).count());
    let archived_count = list.as_ref().map_or(0, |l| l.len() - active_count);
    let current = selection();
    let selected_book = match (current, list.as_ref()) {
        (Some(Selection::Book(id)), Some(items)) => items.iter().find(|b| b.id == id).cloned(),
        _ => None,
    };

    rsx! {
        div { class: "admin-notebooks",
            div { class: "admin-page-head",
                div {
                    h1 { "笔记本" }
                    p { "把相关的记录放在一起，按你的思路排列。" }
                }
                div { class: "admin-page-actions",
                    button {
                        class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                        r#type: "button",
                        onclick: move |_| selection.set(Some(Selection::New)),
                        Icon { name: IconName::Plus, size: 16 }
                        "新建笔记本"
                    }
                }
            }
            match list {
                None if matches!(books.read().as_ref(), Some(Err(_))) => rsx! {
                    LoadError { title: "笔记本加载失败", on_retry: move |_| books.restart() }
                },
                None => rsx! {
                    NotebooksSkeleton {}
                },
                Some(items) if items.is_empty() && current != Some(Selection::New) => rsx! {
                    div { class: "admin-panel",
                        EmptyState {
                            icon: IconName::BookOpen,
                            title: "还没有笔记本",
                            description: "把一组主题笔记放进笔记本，按你的顺序排列；公开后访客可以从第一篇读起。",
                            action: EmptyStateAction {
                                label: "新建笔记本".to_string(),
                                onclick: Callback::new(move |_| selection.set(Some(Selection::New))),
                            },
                        }
                    }
                },
                Some(items) => {
                    let items_ids: Vec<i32> = items.iter().map(|b| b.id).collect();
                    rsx! {
                    div { class: "admin-notebooks-grid",
                        section { class: "admin-panel admin-notebooks-list", aria_label: "笔记本列表",
                            div { class: "admin-notebooks-tabs", role: "tablist", aria_label: "笔记本状态",
                                for (value, label, n) in [(false, "使用中", active_count), (true, "已归档", archived_count)] {
                                    button {
                                        key: "{label}",
                                        r#type: "button",
                                        role: "tab",
                                        aria_selected: "{archived() == value}",
                                        onclick: move |_| {
                                            archived.set(value);
                                            if selection() == Some(Selection::New) {
                                                selection.set(None);
                                            }
                                        },
                                        "{label}"
                                        span { "{n}" }
                                    }
                                }
                            }
                            ul {
                                for book in items.iter().filter(|b| b.archived_at.is_some() == archived()).cloned() {
                                    li { key: "{book.id}",
                                        button {
                                            class: "admin-notebook-item",
                                            r#type: "button",
                                            aria_pressed: "{current == Some(Selection::Book(book.id))}",
                                            onclick: move |_| selection.set(Some(Selection::Book(book.id))),
                                            span { class: "admin-notebook-item-text",
                                                span { class: "admin-notebook-item-title", "{book.title}" }
                                                span { class: "admin-notebook-item-meta",
                                                    "{book.note_count} 篇 · "
                                                    if book.archived_at.is_some() {
                                                        "已归档"
                                                    } else if book.is_public {
                                                        "公开"
                                                    } else {
                                                        "私密"
                                                    }
                                                }
                                            }
                                            if book.is_public && book.archived_at.is_none() {
                                                span { class: "admin-notebook-item-dot", title: "公开" }
                                            }
                                        }
                                    }
                                }
                            }
                            if !items.iter().any(|b| b.archived_at.is_some() == archived()) {
                                p { class: "admin-notebooks-empty",
                                    if archived() { "没有已归档的笔记本。" } else { "没有使用中的笔记本。" }
                                }
                            }
                        }
                        match current {
                            Some(Selection::New) => rsx! {
                                NotebookEditor {
                                    key: "{NEW_KEY}",
                                    book: None,
                                    on_saved: move |_| {
                                        known.set(Some(items_ids.clone()));
                                        books.restart();
                                    },
                                    on_cancel: move |_| selection.set(None),
                                }
                            },
                            _ => rsx! {
                                if let Some(book) = selected_book {
                                    NotebookEditor {
                                        key: "{book.id}",
                                        book: Some(book),
                                        on_saved: move |_| books.restart(),
                                        on_cancel: move |_| {},
                                    }
                                } else {
                                    div { class: "admin-panel admin-notebook-editor admin-notebook-blank",
                                        p { "选择左侧的笔记本进行编辑。" }
                                    }
                                }
                            },
                        }
                    }
                    }
                }
            }
        }
    }
}

/// 右栏：名称、简介、公开、目录顺序与归档。`book` 为 `None` 时是新建表单。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn NotebookEditor(
    book: Option<Notebook>,
    on_saved: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let toast = use_toast();
    let id = book.as_ref().map(|b| b.id);
    let is_archived = book.as_ref().is_some_and(|b| b.archived_at.is_some());
    let base_title = book.as_ref().map(|b| b.title.clone()).unwrap_or_default();
    let base_description = book
        .as_ref()
        .map(|b| b.description.clone())
        .unwrap_or_default();
    let base_public = book.as_ref().is_some_and(|b| b.is_public);
    let mut title = use_signal(|| base_title.clone());
    let mut description = use_signal(|| base_description.clone());
    let mut public = use_signal(|| base_public);
    let mut busy = use_signal(|| false);
    let mut confirm_archive = use_signal(|| false);

    let mut order = use_resource(move || async move {
        match id {
            Some(id) => client_request(notebook_order(id)).await.map(Some),
            None => Ok(None),
        }
    });
    // 目录：服务端顺序与当前编排。
    let mut baseline = use_signal(Vec::<NotebookEntry>::new);
    let mut rows = use_signal(Vec::<NotebookEntry>::new);
    let mut moved = use_signal(|| None::<i32>);
    use_effect(move || {
        if let Some(Ok(Some(items))) = order.read().as_ref() {
            baseline.set(items.clone());
            rows.set(items.clone());
        }
    });

    let meta_dirty =
        title() != base_title || description() != base_description || public() != base_public;
    let order_dirty = rows.read().iter().map(|r| r.id).collect::<Vec<_>>()
        != baseline.read().iter().map(|r| r.id).collect::<Vec<_>>();
    let dirty = meta_dirty || order_dirty;
    let can_save = !busy() && !title().trim().is_empty() && (dirty || id.is_none());

    let save = move |_| {
        if busy() {
            return;
        }
        busy.set(true);
        let ids: Vec<i32> = rows.peek().iter().map(|r| r.id).collect();
        spawn(async move {
            let mut result = Ok(());
            if meta_dirty || id.is_none() {
                result = save_notebook(NotebookInput {
                    id,
                    title: title(),
                    description: description(),
                    is_public: public(),
                })
                .await;
            }
            if let (Ok(()), Some(book_id), true) = (&result, id, order_dirty) {
                result = reorder_notebook(book_id, ids).await;
                if result.is_ok() {
                    let saved = rows.peek().clone();
                    baseline.set(saved);
                    moved.set(None);
                }
            }
            match result {
                Ok(()) => {
                    toast.success(if id.is_some() {
                        "笔记本已保存"
                    } else {
                        "笔记本已创建"
                    });
                    on_saved.call(());
                }
                Err(e) => {
                    toast.error(format!("保存失败：{e}"));
                    order.restart();
                }
            }
            busy.set(false);
        });
    };
    let mut set_archived = move |archive: bool| {
        let Some(book_id) = id else {
            return;
        };
        busy.set(true);
        spawn(async move {
            match archive_notebook(book_id, archive).await {
                Ok(()) => {
                    toast.success(if archive {
                        "已归档，公开目录不再显示"
                    } else {
                        "已恢复，目录保持私密"
                    });
                    on_saved.call(());
                }
                Err(e) => toast.error(format!("操作失败：{e}")),
            }
            busy.set(false);
        });
    };
    let mut shift = move |index: usize, up: bool| {
        let to = if up { index - 1 } else { index + 1 };
        rows.with_mut(|r| r.swap(index, to));
        moved.set(rows.peek().get(to).map(|r| r.id));
    };

    let heading = if id.is_some() {
        "编辑笔记本"
    } else {
        "新建笔记本"
    };
    let count = rows.read().len();
    rsx! {
        section { class: "admin-panel admin-notebook-editor", aria_labelledby: "notebook-editor-h",
            div { class: "admin-notebook-fields",
                h2 { id: "notebook-editor-h", "{heading}" }
                div {
                    label { class: "admin-notebook-label", r#for: "notebook-title", "名称" }
                    input {
                        id: "notebook-title",
                        class: "field",
                        maxlength: 100,
                        required: true,
                        value: "{title}",
                        oninput: move |ev| title.set(ev.value()),
                    }
                }
                div {
                    label { class: "admin-notebook-label", r#for: "notebook-description", "简介" }
                    textarea {
                        id: "notebook-description",
                        class: "field field-textarea",
                        rows: "2",
                        maxlength: 1000,
                        placeholder: "一两句话说明这本笔记写什么",
                        value: "{description}",
                        oninput: move |ev| description.set(ev.value()),
                    }
                }
                div { class: "admin-notebook-switch",
                    ToggleSwitch {
                        checked: public(),
                        disabled: is_archived,
                        aria_label: "公开展示这个笔记本".to_string(),
                        ontoggle: move |_| public.toggle(),
                    }
                    div {
                        p { "公开展示这个笔记本" }
                        p { class: "admin-notebook-hint",
                            if is_archived {
                                "已归档的笔记本须先恢复，才能公开。"
                            } else {
                                "访客只能看到其中已发布的笔记。"
                            }
                        }
                    }
                }
            }

            if id.is_some() {
                div { class: "admin-notebook-order",
                    div { class: "admin-notebook-order-head",
                        h3 { "目录顺序" }
                        span { "上移或下移调整阅读顺序，保存后生效" }
                    }
                    match order.read().as_ref() {
                        None => rsx! {
                            p { class: "admin-notebook-hint", role: "status", "正在读取目录…" }
                        },
                        Some(Err(_)) => rsx! {
                            LoadError { title: "目录加载失败", on_retry: move |_| order.restart() }
                        },
                        Some(Ok(_)) if count == 0 => rsx! {
                            p { class: "admin-notebook-hint",
                                "这个笔记本还没有笔记。在笔记编辑器的「设置」里把笔记收进来。"
                            }
                        },
                        Some(Ok(_)) => rsx! {
                            ol { class: "admin-notebook-rows",
                                for (index, entry) in rows().into_iter().enumerate() {
                                    li {
                                        key: "{entry.id}",
                                        "data-moved": (moved() == Some(entry.id)).then_some("true"),
                                        span { class: "admin-notebook-row-n", {format!("{:02}", index + 1)} }
                                        span { class: "admin-notebook-row-text",
                                            span { class: "admin-notebook-row-title", "{entry.title}" }
                                            span { class: "admin-notebook-row-state", "{entry.state_label()}" }
                                        }
                                        IconButton {
                                            icon: IconName::ChevronUp,
                                            label: format!("上移「{}」", entry.title),
                                            size: ButtonSize::Sm,
                                            disabled: index == 0 || busy(),
                                            onclick: move |_| shift(index, true),
                                        }
                                        IconButton {
                                            icon: IconName::ChevronDown,
                                            label: format!("下移「{}」", entry.title),
                                            size: ButtonSize::Sm,
                                            disabled: index + 1 == count || busy(),
                                            onclick: move |_| shift(index, false),
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }

            div { class: "admin-notebook-foot",
                if id.is_some() {
                    div { class: "admin-notebook-archive",
                        if is_archived {
                            button {
                                class: "admin-notebook-link",
                                r#type: "button",
                                disabled: busy(),
                                onclick: move |_| set_archived(false),
                                "恢复这个笔记本"
                            }
                            p { "恢复后目录保持私密，需要时再打开公开。" }
                        } else {
                            button {
                                class: "admin-notebook-link",
                                r#type: "button",
                                disabled: busy(),
                                onclick: move |_| confirm_archive.set(true),
                                "归档这个笔记本"
                            }
                            p { "归档会隐藏公开目录，不删除笔记，也不撤回笔记自身的发布或 AI 授权。" }
                        }
                    }
                } else {
                    button {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                        r#type: "button",
                        onclick: move |_| on_cancel.call(()),
                        "取消"
                    }
                }
                button {
                    class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                    r#type: "button",
                    disabled: !can_save,
                    onclick: save,
                    if busy() {
                        "保存中…"
                    } else if id.is_none() {
                        "创建笔记本"
                    } else if dirty {
                        "保存"
                    } else {
                        "已保存"
                    }
                }
            }
        }
        ConfirmDialog {
            visible: confirm_archive,
            title: "归档这个笔记本？",
            description: "公开目录会隐藏，笔记本身不受影响：已发布的笔记仍然公开，AI 授权也保留。之后可以在「已归档」里恢复。",
            confirm_label: "归档",
            icon: IconName::Archive,
            on_confirm: move |_| set_archived(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_finds_the_notebook_missing_before_save() {
        let book = |id| Notebook {
            id,
            ..Default::default()
        };
        assert_eq!(created(&[book(3), book(9), book(4)], &[3, 4]), Some(9));
        // 列表尚未刷新。
        assert_eq!(created(&[book(3), book(4)], &[3, 4]), None);
    }
}
