//! 新建令牌与编辑权限对话框，以及两者共用的笔记授权字段。
use super::{scope_desc, OpenToken};
use crate::api::mcp_tokens::{create_mcp_token, update_mcp_token_permissions, TokenLifetime};
use crate::components::forms::FormInput;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Checkbox, IconButton, IconName, ModalShell,
    SegmentedControl, SegmentedItem, SegmentedKind, SegmentedSize,
};
use crate::models::mcp_token::{validate_token_name, McpTokenSummary, NoteGrant, TokenScope};
use crate::utils::error::failure_message;
use dioxus::prelude::*;

const SCOPES: [TokenScope; 3] = [TokenScope::Read, TokenScope::Write, TokenScope::Admin];
const LIFETIMES: [(TokenLifetime, &str); 5] = [
    (TokenLifetime::Days1, "1 天"),
    (TokenLifetime::Days7, "7 天"),
    (TokenLifetime::Days30, "30 天"),
    (TokenLifetime::Days90, "90 天"),
    (TokenLifetime::Never, "永不过期"),
];

/// 关闭时先播退出动画，再通知父组件卸载。
fn use_close_after_exit(visible: Signal<bool>, on_close: EventHandler<()>) {
    use_effect(move || {
        if !visible() {
            spawn(async move {
                crate::utils::time::sleep_ms(crate::components::ui::EXIT_ANIM_MS).await;
                on_close.call(());
            });
        }
    });
}

/// 笔记授权草稿。
#[derive(Clone, Copy, PartialEq)]
struct GrantDraft {
    read: Signal<bool>,
    write: Signal<bool>,
    restricted: Signal<bool>,
    books: Signal<Vec<i32>>,
}

impl GrantDraft {
    fn new(grant: &NoteGrant) -> Self {
        Self {
            read: Signal::new(grant.read),
            write: Signal::new(grant.write),
            restricted: Signal::new(grant.notebook_ids.is_some()),
            books: Signal::new(grant.notebook_ids.clone().unwrap_or_default()),
        }
    }

    /// 限定笔记本却一个也没选。
    fn missing_books(&self) -> bool {
        (self.read)() && (self.restricted)() && (self.books)().is_empty()
    }

    fn grant(&self) -> NoteGrant {
        let read = (self.read)();
        NoteGrant {
            read,
            write: read && (self.write)(),
            notebook_ids: (read && (self.restricted)()).then(|| (self.books)()),
        }
    }
}

/// 文章权限：三张单选卡片。
#[component]
fn ScopeCards(mut scope: Signal<TokenScope>) -> Element {
    rsx! {
        fieldset { class: "mcp-fieldset",
            legend { "文章权限" }
            div { class: "mcp-scopes", role: "radiogroup", aria_label: "文章权限",
                for s in SCOPES {
                    button {
                        key: "{s.as_str()}",
                        class: "mcp-scope",
                        r#type: "button",
                        role: "radio",
                        aria_checked: "{scope() == s}",
                        onclick: move |_| scope.set(s),
                        span { class: "mcp-scope-key", "{s.as_str()}" }
                        span { class: "mcp-scope-desc", "{scope_desc(s)}" }
                    }
                }
            }
        }
    }
}

/// 笔记授权：读取、写入草稿，可限定笔记本。
#[component]
fn GrantFields(draft: GrantDraft, submitted: bool) -> Element {
    let GrantDraft {
        mut read,
        mut write,
        mut restricted,
        mut books,
    } = draft;
    let notebooks = use_resource(|| {
        crate::pages::admin::notes::client_request(crate::api::notes::owned_notebooks())
    });
    let missing = draft.missing_books();
    rsx! {
        fieldset { class: "mcp-fieldset",
            legend { "笔记授权" }
            p { class: "mcp-muted", "与文章权限独立。写入只生成工作稿，不会自动发布。" }
            label { class: "mcp-check",
                Checkbox {
                    checked: read(),
                    onchange: move |v: bool| {
                        read.set(v);
                        if !v {
                            write.set(false);
                        }
                    },
                }
                "读取我的知识库"
            }
            label { class: "mcp-check",
                Checkbox {
                    checked: write(),
                    onchange: move |v: bool| {
                        write.set(v);
                        if v {
                            read.set(true);
                        }
                    },
                }
                "允许创建和修改笔记草稿"
            }
            if read() {
                label { class: "mcp-check",
                    Checkbox { checked: restricted(), onchange: move |v| restricted.set(v) }
                    "只授权指定的笔记本"
                }
                if restricted() {
                    div { class: "mcp-books", role: "group", aria_label: "授权的笔记本",
                        match notebooks.read().as_ref() {
                            Some(Ok(items)) if items.is_empty() => rsx! {
                                span { class: "mcp-muted", "还没有笔记本。" }
                            },
                            Some(Ok(items)) => rsx! {
                                for book in items.iter().cloned() {
                                    button {
                                        key: "{book.id}",
                                        class: "note-book",
                                        r#type: "button",
                                        aria_pressed: "{books().contains(&book.id)}",
                                        onclick: move |_| books.with_mut(|ids| {
                                            if let Some(i) = ids.iter().position(|x| *x == book.id) {
                                                ids.remove(i);
                                            } else {
                                                ids.push(book.id);
                                            }
                                        }),
                                        "{book.title}"
                                    }
                                }
                            },
                            Some(Err(_)) => rsx! {
                                span { class: "mcp-error", "笔记本加载失败。" }
                            },
                            None => rsx! {
                                span { class: "mcp-muted", "正在读取笔记本…" }
                            },
                        }
                    }
                    if submitted && missing {
                        p { class: "mcp-error", role: "alert", "请至少选择一个笔记本。" }
                    }
                }
            }
        }
    }
}

/// 新建令牌对话框。创建成功后把明文交给令牌对话框。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn CreateTokenDialog(
    on_close: EventHandler<()>,
    on_created: EventHandler<OpenToken>,
) -> Element {
    let toast = use_toast();
    let mut visible = use_signal(|| true);
    let mut closing = use_signal(|| false);
    let mut name = use_signal(String::new);
    let scope = use_signal(|| TokenScope::Read);
    let mut lifetime = use_signal(|| TokenLifetime::Days30);
    let draft = use_hook(|| GrantDraft::new(&NoteGrant::default()));
    let mut submitted = use_signal(|| false);
    let mut busy = use_signal(|| false);
    use_close_after_exit(visible, on_close);

    let name_error = validate_token_name(&name()).err();
    let length = name().trim().chars().count();
    let submit = move |e: FormEvent| {
        e.prevent_default();
        submitted.set(true);
        if busy() || validate_token_name(&name()).is_err() || draft.missing_books() {
            return;
        }
        busy.set(true);
        let (n, sc, lt, grant) = (
            name().trim().to_string(),
            scope(),
            lifetime(),
            draft.grant(),
        );
        spawn(async move {
            match create_mcp_token(n, sc, lt, Some(grant)).await {
                Ok(resp) => on_created.call(OpenToken {
                    summary: resp.summary,
                    plaintext: resp.plaintext,
                    created: true,
                }),
                Err(e) => toast.error(failure_message("创建失败", &e)),
            }
            busy.set(false);
        });
    };

    rsx! {
        ModalShell {
            visible,
            closing,
            title: "新建令牌",
            overlay_padding: "p-4",
            panel_class: "mcp-dialog",
            form { class: "mcp-form", onsubmit: submit,
                div { class: "mcp-dialog-head",
                    div {
                        h2 { "新建令牌" }
                        p { "创建后会显示令牌明文，之后也可以在列表里重新查看。" }
                    }
                    IconButton {
                        icon: IconName::X,
                        label: "关闭",
                        size: ButtonSize::Sm,
                        onclick: move |_| {
                            closing.set(true);
                            visible.set(false);
                        },
                    }
                }
                div { class: "mcp-dialog-body",
                    div {
                        label { class: "mcp-label", r#for: "mcp-token-name",
                            span { "名称" }
                            span { class: "mcp-count", "{length} / 64" }
                        }
                        FormInput {
                            id: Some("mcp-token-name".to_string()),
                            placeholder: "如 claude-code-macbook",
                            autofocus: true,
                            invalid: submitted() && name_error.is_some(),
                            value: name(),
                            oninput: move |v: String| name.set(v),
                        }
                        if submitted() {
                            if let Some(err) = name_error {
                                p { class: "mcp-error", role: "alert", "{err}" }
                            }
                        }
                    }
                    ScopeCards { scope }
                    GrantFields { draft, submitted: submitted() }
                    div { class: "mcp-lifetime",
                        span { class: "mcp-label", "有效期" }
                        SegmentedControl {
                            items: LIFETIMES
                                .iter()
                                .map(|(v, l)| SegmentedItem::new(*v, *l))
                                .collect::<Vec<_>>(),
                            value: lifetime(),
                            on_change: move |v| lifetime.set(v),
                            aria_label: "有效期",
                            kind: SegmentedKind::Radio,
                            size: SegmentedSize::Sm,
                        }
                    }
                }
                div { class: "mcp-dialog-foot",
                    button {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                        r#type: "button",
                        onclick: move |_| {
                            closing.set(true);
                            visible.set(false);
                        },
                        "取消"
                    }
                    button {
                        class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                        r#type: "submit",
                        disabled: busy() || name().trim().is_empty(),
                        if busy() { "创建中…" } else { "创建令牌" }
                    }
                }
            }
        }
    }
}

/// 编辑令牌权限：不轮换密钥、不改有效期。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn EditPermissionsDialog(
    token: McpTokenSummary,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
) -> Element {
    let toast = use_toast();
    let mut visible = use_signal(|| true);
    let mut closing = use_signal(|| false);
    let scope = use_signal(|| token.scope);
    let draft = use_hook(|| GrantDraft::new(&token.notes));
    let mut submitted = use_signal(|| false);
    let mut busy = use_signal(|| false);
    use_close_after_exit(visible, on_close);
    let id = token.id.clone();
    let save = move |e: FormEvent| {
        e.prevent_default();
        submitted.set(true);
        if busy() || draft.missing_books() {
            return;
        }
        busy.set(true);
        let (id, sc, grant) = (id.clone(), scope(), draft.grant());
        spawn(async move {
            match update_mcp_token_permissions(id, sc, grant).await {
                Ok(()) => {
                    toast.success("权限已更新，原令牌继续有效");
                    on_saved.call(());
                }
                Err(e) => toast.error(failure_message("保存失败", &e)),
            }
            busy.set(false);
        });
    };
    rsx! {
        ModalShell {
            visible,
            closing,
            title: "编辑令牌权限",
            overlay_padding: "p-4",
            panel_class: "mcp-dialog",
            form { class: "mcp-form", onsubmit: save,
                div { class: "mcp-dialog-head",
                    div {
                        h2 { "编辑令牌权限" }
                        p { "「{token.name}」继续使用原来的密钥与有效期。" }
                    }
                    IconButton {
                        icon: IconName::X,
                        label: "关闭",
                        size: ButtonSize::Sm,
                        onclick: move |_| {
                            closing.set(true);
                            visible.set(false);
                        },
                    }
                }
                div { class: "mcp-dialog-body",
                    ScopeCards { scope }
                    GrantFields { draft, submitted: submitted() }
                }
                div { class: "mcp-dialog-foot",
                    button {
                        class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                        r#type: "button",
                        onclick: move |_| {
                            closing.set(true);
                            visible.set(false);
                        },
                        "取消"
                    }
                    button {
                        class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                        r#type: "submit",
                        disabled: busy(),
                        if busy() { "保存中…" } else { "保存权限" }
                    }
                }
            }
        }
    }
}
