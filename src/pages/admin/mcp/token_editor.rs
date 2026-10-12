//! Token creation and permission editing.
use super::McpPageState;
use crate::api::mcp_tokens::{create_mcp_token, update_mcp_token_permissions, TokenLifetime};
use crate::components::forms::FormSelect;
use crate::components::ui::{BTN_PRIMARY, BTN_PRIMARY_SM};
use crate::models::mcp_token::{McpTokenSummary, TokenScope};
use dioxus::prelude::*;

/// 作用域可选项（下拉菜单）。
#[cfg(target_arch = "wasm32")]
const SCOPE_OPTIONS: &[(TokenScope, &str)] = &[
    (TokenScope::Read, "read — 仅查询已发布文章"),
    (TokenScope::Write, "write — read + 文章/评论/标签/媒体 CRUD"),
    (TokenScope::Admin, "admin — write + 站点设置 / 代码运行器"),
];

/// 有效期可选项（下拉菜单）。
#[cfg(target_arch = "wasm32")]
const LIFETIME_OPTIONS: &[(TokenLifetime, &str)] = &[
    (TokenLifetime::Days1, "1 天"),
    (TokenLifetime::Days7, "7 天"),
    (TokenLifetime::Days30, "30 天"),
    (TokenLifetime::Days90, "90 天"),
    (TokenLifetime::Never, "永不过期"),
];

/// 新建令牌表单卡片。成功后把明文写入共享 context 的一次性弹窗。
#[cfg(target_arch = "wasm32")]
#[component]
pub(super) fn CreateTokenCard() -> Element {
    let mut state: McpPageState = use_context();
    let mut name = use_signal(String::new);
    let mut scope = use_signal(|| TokenScope::Read);
    let mut lifetime = use_signal(|| TokenLifetime::Days30);
    let mut busy = use_signal(|| false);
    let notes_read = use_signal(|| false);
    let notes_write = use_signal(|| false);
    let restricted = use_signal(|| false);
    let selected_books = use_signal(Vec::<i32>::new);
    let mut name_touched = use_signal(|| false);
    let mut submitted = use_signal(|| false);

    let name_error = crate::models::mcp_token::validate_token_name(&name()).err();
    let show_name_error = (name_touched() || submitted()) && name_error.is_some();
    let missing_books = notes_read() && restricted() && selected_books().is_empty();

    let mut created_plaintext = state.created_plaintext;
    let reload_gen = state.reload_gen;
    let mut toast = state.toast;

    rsx! {
        div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 p-6 sm:p-8 flex flex-col gap-6",
            div { class: "flex items-center gap-2.5 border-b border-[var(--color-paper-border)]/60 pb-4",
                svg {
                    class: "w-5 h-5 text-[var(--color-paper-accent)]",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    line { x1: "12", y1: "5", x2: "12", y2: "19" }
                    line { x1: "5", y1: "12", x2: "19", y2: "12" }
                }
                h2 { class: "text-lg sm:text-xl font-bold text-[var(--color-paper-primary)]",
                    "新建访问令牌"
                }
            }

            div { class: "grid grid-cols-1 md:grid-cols-3 gap-5",
                // 名称
                div { class: "flex flex-col gap-2",
                    label { r#for: "mcp-token-name", class: "text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)]",
                        "令牌名称 *"
                    }
                    input {
                        id: "mcp-token-name",
                        class: crate::components::forms::INPUT_CLASS,
                        style: if show_name_error { "border-color: var(--color-red-500);" } else { "" },
                        r#type: "text",
                        required: true,
                        aria_invalid: if show_name_error { "true" } else { "false" },
                        aria_describedby: "mcp-token-name-hint",
                        placeholder: "如 claude-code-macbook",
                        value: name(),
                        oninput: move |e| name.set(e.value()),
                        onblur: move |_| name_touched.set(true),
                    }
                    p {
                        id: "mcp-token-name-hint",
                        class: if show_name_error { "text-xs text-red-500" } else { "text-xs text-paper-secondary" },
                        aria_live: "polite",
                        if show_name_error { "{name_error.unwrap_or_default()}" } else { "必填，最多 64 个字符" }
                    }
                }
                // 作用域
                div { class: "flex flex-col gap-2",
                    label { class: "text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)]",
                        "权限作用域"
                    }
                    FormSelect {
                        value: scope(),
                        options: SCOPE_OPTIONS.to_vec(),
                        onchange: move |s| scope.set(s),
                    }
                }
                // 有效期
                div { class: "flex flex-col gap-2",
                    label { class: "text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)]",
                        "有效期"
                    }
                    FormSelect {
                        value: lifetime(),
                        options: LIFETIME_OPTIONS.to_vec(),
                        onchange: move |l| lifetime.set(l),
                    }
                }
            }

            NoteGrantFields { notes_read, notes_write, restricted, selected_books, submitted: submitted(), id_prefix: "mcp-token" }

            div { class: "pt-1",
                button {
                    class: "{BTN_PRIMARY} inline-flex items-center gap-1.5",
                    disabled: busy(),
                    onclick: move |_| {
                        if busy() {
                            return;
                        }
                        submitted.set(true);
                        let n = name().trim().to_string();
                        if crate::models::mcp_token::validate_token_name(&n).is_err() || missing_books {
                            use wasm_bindgen::JsCast;

                            let selector = if crate::models::mcp_token::validate_token_name(&n).is_err() {
                                "#mcp-token-name"
                            } else {
                                ".mcp-token-notebook input"
                            };
                            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                                let target = document.query_selector(selector).ok().flatten()
                                    .or_else(|| document.query_selector("#mcp-token-restricted input").ok().flatten());
                                if let Some(target) = target.and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok()) {
                                    let _ = target.focus();
                                }
                            }
                            return;
                        }
                        let sc = scope();
                        let lt = lifetime();
                        let notes=crate::models::mcp_token::NoteGrant {read:notes_read(),write:notes_write(),notebook_ids:if notes_read() && restricted(){Some(selected_books())}else{None}};
                        busy.set(true);
                        spawn(async move {
                            match create_mcp_token(n, sc, lt, Some(notes)).await {
                                Ok(resp) => {
                                    created_plaintext.set(Some(resp.plaintext));
                                    name.set(String::new());
                                    name_touched.set(false);
                                    submitted.set(false);
                                    let g = reload_gen();
                                    state.reload_gen.set(g + 1);
                                }
                                Err(e) => {
                                    toast.set(Some((format!("创建失败：{e}"), true)));
                                }
                            }
                            busy.set(false);
                        });
                    },
                    if busy() {
                        "创建中…"
                    } else {
                        "创建访问令牌"
                    }
                }
            }
        }
    }
}

/// 创建和编辑令牌共用的笔记授权字段。
#[cfg(target_arch = "wasm32")]
#[component]
fn NoteGrantFields(
    mut notes_read: Signal<bool>,
    mut notes_write: Signal<bool>,
    mut restricted: Signal<bool>,
    mut selected_books: Signal<Vec<i32>>,
    submitted: bool,
    id_prefix: &'static str,
) -> Element {
    let mut books = use_resource(|| {
        crate::pages::admin::notes::client_request(crate::api::notes::owned_notebooks())
    });
    let missing_books = notes_read() && restricted() && selected_books().is_empty();
    rsx! {
        fieldset { class: "min-w-0 space-y-3 text-sm border-t border-paper-border pt-4",
            legend { class: "font-semibold", "笔记知识库授权" }
            p { class: "text-xs text-paper-secondary", "与文章权限独立。只读授权可检索已收录的私人笔记；写入授权可读写工作稿，不会自动发布。" }
            label { class: "flex items-center gap-2",
                crate::components::ui::Checkbox { checked: notes_read(), onchange: move |v| { notes_read.set(v); if !v { notes_write.set(false); } } }
                "读取我的知识库"
            }
            label { class: "flex items-center gap-2",
                crate::components::ui::Checkbox { checked: notes_write(), onchange: move |v| { notes_write.set(v); if v { notes_read.set(true); } } }
                "允许创建和修改笔记草稿"
            }
            if notes_read() {
                label { id: "{id_prefix}-restricted", class: "flex items-center gap-2",
                    crate::components::ui::Checkbox { checked: restricted(), onchange: move |v| restricted.set(v) }
                    "仅授权指定笔记本"
                }
                if restricted() {
                    match books.read().as_ref() {
                        Some(Ok(items)) => rsx! {
                            for book in items {
                                label { class: "{id_prefix}-notebook flex items-center gap-2 ml-4", key: "{book.id}",
                                    crate::components::ui::Checkbox {
                                        checked: selected_books().contains(&book.id),
                                        onchange: { let id = book.id; move |v| selected_books.with_mut(|ids| { ids.retain(|x| *x != id); if v { ids.push(id); } }) },
                                    }
                                    "{book.title}"
                                }
                            }
                            if items.is_empty() { p { class: "text-xs text-paper-secondary", "请先在笔记管理中创建笔记本。" } }
                        },
                        Some(Err(e)) => rsx! {
                            p { role: "alert", class: "text-xs text-red-500", "笔记本加载失败：{e}" }
                            button { r#type: "button", class: "text-xs text-paper-accent", onclick: move |_| books.restart(), "重试" }
                        },
                        None => rsx! { p { class: "text-xs text-paper-secondary", "正在加载笔记本…" } },
                    }
                    p {
                        class: if submitted && missing_books { "text-xs text-red-500" } else { "text-xs text-paper-secondary" },
                        aria_live: "polite",
                        if missing_books { "请至少选择一个笔记本（必选）" }
                    }
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
pub(super) fn EditTokenPermissionsModal(token: McpTokenSummary) -> Element {
    let mut state: McpPageState = use_context();
    let mut visible = use_signal(|| true);
    let mut closing = use_signal(|| false);
    let mut scope = use_signal(|| token.scope);
    let notes_read = use_signal(|| token.notes.read);
    let notes_write = use_signal(|| token.notes.write);
    let restricted = use_signal(|| token.notes.notebook_ids.is_some());
    let selected_books = use_signal(|| token.notes.notebook_ids.clone().unwrap_or_default());
    let mut submitted = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    use_effect(move || {
        if !visible() && !closing() && !busy() {
            state.editing.set(None);
        }
    });
    rsx! {
        crate::components::ui::ModalShell {
            visible, closing, title: "编辑令牌权限", overlay_padding: "p-4",
            panel_class: "w-full max-w-xl max-h-[90dvh] overflow-y-auto p-6",
            div { class: "space-y-5", onkeydown: move |e| {
                if e.key() == Key::Escape && !busy() { closing.set(true); visible.set(false); }
            },
                h2 { class: "text-lg font-bold", "编辑权限 · {token.name}" }
                p { class: "text-sm text-paper-secondary", "保存后下次调用生效，无需更换客户端令牌。有效期保持不变。" }
                fieldset { disabled: busy(), class: "min-w-0 space-y-5",
                    div { class: "space-y-2",
                        label { class: "text-sm font-semibold", "权限作用域" }
                        FormSelect { value: scope(), options: SCOPE_OPTIONS.to_vec(), onchange: move |v| scope.set(v) }
                    }
                    NoteGrantFields { notes_read, notes_write, restricted, selected_books, submitted: submitted(), id_prefix: "mcp-edit-token" }
                }
                if let Some(message) = error() {
                    p { role: "alert", class: "text-sm text-red-500", "{message}" }
                }
                div { class: "flex justify-end gap-3",
                    button {
                        class: crate::components::ui::BTN_SECONDARY,
                        disabled: busy(),
                        onclick: move |_| { closing.set(true); visible.set(false); },
                        "取消"
                    }
                    button {
                        class: BTN_PRIMARY_SM, disabled: busy(),
                        onclick: move |_| {
                            if busy() { return; }
                            submitted.set(true);
                            if notes_read() && restricted() && selected_books().is_empty() { return; }
                            let notes = crate::models::mcp_token::NoteGrant {
                                read: notes_read(), write: notes_write(),
                                notebook_ids: if notes_read() && restricted() { Some(selected_books()) } else { None },
                            };
                            let id = token.id.clone();
                            let scope = scope();
                            busy.set(true);
                            error.set(None);
                            spawn(async move {
                                match update_mcp_token_permissions(id, scope, notes).await {
                                    Ok(()) => {
                                        state.reload_gen.with_mut(|g| *g += 1);
                                        state.toast.set(Some(("权限已更新，原令牌继续有效".into(), false)));
                                        closing.set(true);
                                        visible.set(false);
                                    }
                                    Err(e) => error.set(Some(format!("保存失败：{e}"))),
                                }
                                busy.set(false);
                            });
                        },
                        if busy() { "保存中…" } else { "保存权限" }
                    }
                }
            }
        }
    }
}
