//! 新建令牌与编辑权限对话框，以及两者共用的笔记授权字段。
use super::{scope_desc, OpenToken};
use crate::api::mcp_tokens::{create_mcp_token, update_mcp_token_permissions, TokenLifetime};
use crate::components::forms::FormInput;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Checkbox, IconButton, IconName, ModalShell,
    SegmentedControl, SegmentedItem, SegmentedKind, SegmentedSize,
};
use crate::models::mcp_token::{validate_token_name, McpTokenSummary, NoteGrant, TokenScope};
use crate::models::note::Notebook;
use crate::utils::error::failure_message;
use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;

const SCOPES: [TokenScope; 3] = [TokenScope::Read, TokenScope::Write, TokenScope::Admin];
const LIFETIMES: [(TokenLifetime, &str); 5] = [
    (TokenLifetime::Days1, "1 天"),
    (TokenLifetime::Days7, "7 天"),
    (TokenLifetime::Days30, "30 天"),
    (TokenLifetime::Days90, "90 天"),
    (TokenLifetime::Never, "永不过期"),
];

/// 关闭时先播退出动画，再通知父组件卸载。
pub(super) fn use_close_after_exit(visible: RwSignal<bool>, on_close: Callback<()>) {
    Effect::new(move |_| {
        if !visible.get() {
            spawn_local(async move {
                crate::utils::time::sleep_ms(crate::components::ui::EXIT_ANIM_MS).await;
                let _ = on_close.try_run(());
            });
        }
    });
}

/// 笔记授权草稿。
#[derive(Clone, Copy)]
struct GrantDraft {
    read: RwSignal<bool>,
    write: RwSignal<bool>,
    restricted: RwSignal<bool>,
    books: RwSignal<Vec<i32>>,
}

impl GrantDraft {
    fn new(grant: &NoteGrant) -> Self {
        Self {
            read: RwSignal::new(grant.read),
            write: RwSignal::new(grant.write),
            restricted: RwSignal::new(grant.notebook_ids.is_some()),
            books: RwSignal::new(grant.notebook_ids.clone().unwrap_or_default()),
        }
    }

    /// 限定笔记本却一个也没选（读取响应式状态，可在闭包里追踪）。
    fn missing_books(&self) -> bool {
        self.read.get() && self.restricted.get() && self.books.with(Vec::is_empty)
    }

    fn grant(&self) -> NoteGrant {
        let read = self.read.get_untracked();
        NoteGrant {
            read,
            write: read && self.write.get_untracked(),
            notebook_ids: (read && self.restricted.get_untracked())
                .then(|| self.books.get_untracked()),
        }
    }
}

/// 文章权限：三张单选卡片。
#[component]
fn ScopeCards(scope: RwSignal<TokenScope>) -> impl IntoView {
    view! {
        <fieldset class="mcp-fieldset">
            <legend>"文章权限"</legend>
            <div class="mcp-scopes" role="radiogroup" aria-label="文章权限">
                {SCOPES
                    .into_iter()
                    .map(|s| {
                        view! {
                            <button
                                class="mcp-scope"
                                type="button"
                                role="radio"
                                aria-checked=move || (scope.get() == s).to_string()
                                on:click=move |_| scope.set(s)
                            >
                                <span class="mcp-scope-key">{s.as_str()}</span>
                                <span class="mcp-scope-desc">{scope_desc(s)}</span>
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
        </fieldset>
    }
}

/// 笔记授权：知识库读取与笔记管理，可限定笔记本。
#[component]
fn GrantFields(draft: GrantDraft, #[prop(into)] submitted: Signal<bool>) -> impl IntoView {
    let GrantDraft {
        read,
        write,
        restricted,
        books,
    } = draft;
    // 对话框只在浏览器里打开：笔记本只在客户端请求（私人数据不进 SSR 缓存）。
    // 不用 Resource：它会被外层 Suspense 计入挂起。`None` = 读取中。
    let notebooks = RwSignal::new(None::<Result<Vec<Notebook>, ()>>);
    Effect::new(move |_| {
        spawn_local(async move {
            let result = crate::api::notes::owned_notebooks().await.map_err(|_| ());
            let _ = notebooks.try_set(Some(result));
        });
    });

    view! {
        <fieldset class="mcp-fieldset">
            <legend>"笔记授权"</legend>
            <p class="mcp-muted">"与文章权限独立。笔记写入授权也允许公开、撤回及管理 AI 知识库收录；创建和编辑不会自动公开或收录。"</p>
            <label class="mcp-check">
                <Checkbox
                    checked=read
                    onchange=move |v: bool| {
                        read.set(v);
                        if !v {
                            write.set(false);
                        }
                    }
                />
                "读取我的知识库"
            </label>
            <label class="mcp-check">
                <Checkbox
                    checked=write
                    onchange=move |v: bool| {
                        write.set(v);
                        if v {
                            read.set(true);
                        }
                    }
                />
                "允许编辑、公开和收录笔记"
            </label>
            {move || {
                read.get()
                    .then(|| {
                        view! {
                            <label class="mcp-check">
                                <Checkbox
                                    checked=restricted
                                    onchange=move |v: bool| restricted.set(v)
                                />
                                "只授权指定的笔记本"
                            </label>
                            {move || {
                                restricted
                                    .get()
                                    .then(|| {
                                        view! {
                                            <div class="mcp-books" role="group" aria-label="授权的笔记本">
                                                {move || match notebooks.get() {
                                                    Some(Ok(items)) if items.is_empty() => {
                                                        view! { <span class="mcp-muted">"还没有笔记本。"</span> }
                                                            .into_any()
                                                    }
                                                    Some(Ok(items)) => {
                                                        items
                                                            .into_iter()
                                                            .map(|book| {
                                                                let id = book.id;
                                                                view! {
                                                                    <button
                                                                        class="note-book"
                                                                        type="button"
                                                                        aria-pressed=move || {
                                                                            books.with(|ids| ids.contains(&id)).to_string()
                                                                        }
                                                                        on:click=move |_| {
                                                                            books
                                                                                .update(|ids| {
                                                                                    if let Some(i) = ids.iter().position(|x| *x == id) {
                                                                                        ids.remove(i);
                                                                                    } else {
                                                                                        ids.push(id);
                                                                                    }
                                                                                })
                                                                        }
                                                                    >
                                                                        {book.title}
                                                                    </button>
                                                                }
                                                            })
                                                            .collect_view()
                                                            .into_any()
                                                    }
                                                    Some(Err(())) => {
                                                        view! { <span class="mcp-error">"笔记本加载失败。"</span> }
                                                            .into_any()
                                                    }
                                                    None => {
                                                        view! { <span class="mcp-muted">"正在读取笔记本…"</span> }
                                                            .into_any()
                                                    }
                                                }}
                                            </div>
                                            {move || {
                                                (submitted.get() && draft.missing_books())
                                                    .then(|| {
                                                        view! {
                                                            <p class="mcp-error" role="alert">
                                                                "请至少选择一个笔记本。"
                                                            </p>
                                                        }
                                                    })
                                            }}
                                        }
                                    })
                            }}
                        }
                    })
            }}
        </fieldset>
    }
}

/// 新建令牌对话框。创建成功后把明文交给令牌对话框。
#[component]
pub(super) fn CreateTokenDialog(
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_created: Callback<OpenToken>,
) -> impl IntoView {
    let toast = use_toast();
    let visible = RwSignal::new(true);
    let closing = RwSignal::new(false);
    let name = RwSignal::new(String::new());
    let scope = RwSignal::new(TokenScope::Read);
    let lifetime = RwSignal::new(TokenLifetime::Days30);
    let draft = GrantDraft::new(&NoteGrant::default());
    let submitted = RwSignal::new(false);
    let busy = RwSignal::new(false);
    use_close_after_exit(visible, on_close);

    let lifetime_items: Vec<SegmentedItem<TokenLifetime>> = LIFETIMES
        .iter()
        .map(|(v, l)| SegmentedItem::new(*v, *l))
        .collect();
    let name_error = move || name.with(|n| validate_token_name(n).err());
    let length = move || name.with(|n| n.trim().chars().count());
    let close = move || {
        closing.set(true);
        visible.set(false);
    };
    let submit = move |e: SubmitEvent| {
        e.prevent_default();
        submitted.set(true);
        if busy.get_untracked() || name_error().is_some() || draft.missing_books() {
            return;
        }
        busy.set(true);
        let (n, sc, lt, grant) = (
            name.get_untracked().trim().to_string(),
            scope.get_untracked(),
            lifetime.get_untracked(),
            draft.grant(),
        );
        spawn_local(async move {
            match create_mcp_token(n, sc, lt, Some(grant)).await {
                Ok(resp) => {
                    let _ = on_created.try_run(OpenToken {
                        summary: resp.summary,
                        plaintext: resp.plaintext,
                        created: true,
                    });
                }
                Err(e) => toast.error(failure_message("创建失败", &e)),
            }
            let _ = busy.try_set(false);
        });
    };

    view! {
        <ModalShell
            visible=visible
            closing=closing
            title="新建令牌"
            overlay_padding="p-4"
            panel_class="mcp-dialog"
        >
            <form class="mcp-form" on:submit=submit>
                <div class="mcp-dialog-head">
                    <div>
                        <h2>"新建令牌"</h2>
                        <p>"创建后会显示令牌明文，之后也可以在列表里重新查看。"</p>
                    </div>
                    <IconButton
                        icon=IconName::X
                        label="关闭"
                        size=ButtonSize::Sm
                        onclick=move |_| close()
                    />
                </div>
                <div class="mcp-dialog-body">
                    <div>
                        <label class="mcp-label" for="mcp-token-name">
                            <span>"名称"</span>
                            <span class="mcp-count">{move || format!("{} / 64", length())}</span>
                        </label>
                        <FormInput
                            id="mcp-token-name".to_string()
                            placeholder="如 claude-code-macbook"
                            autofocus=true
                            invalid=move || submitted.get() && name_error().is_some()
                            value=name
                            oninput=move |v: String| name.set(v)
                        />
                        {move || {
                            submitted
                                .get()
                                .then(name_error)
                                .flatten()
                                .map(|err| view! { <p class="mcp-error" role="alert">{err}</p> })
                        }}
                    </div>
                    <ScopeCards scope=scope/>
                    <GrantFields draft=draft submitted=submitted/>
                    <div class="mcp-lifetime">
                        <span class="mcp-label">"有效期"</span>
                        <SegmentedControl
                            items=lifetime_items.clone()
                            value=lifetime
                            on_change=move |v: TokenLifetime| lifetime.set(v)
                            aria_label="有效期"
                            kind=SegmentedKind::Radio
                            size=SegmentedSize::Sm
                        />
                    </div>
                </div>
                <div class="mcp-dialog-foot">
                    <button
                        class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                        type="button"
                        on:click=move |_| close()
                    >
                        "取消"
                    </button>
                    <button
                        class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                        type="submit"
                        disabled=move || busy.get() || name.with(|n| n.trim().is_empty())
                    >
                        {move || if busy.get() { "创建中…" } else { "创建令牌" }}
                    </button>
                </div>
            </form>
        </ModalShell>
    }
}

/// 编辑令牌权限：不轮换密钥、不改有效期。
#[component]
pub(super) fn EditPermissionsDialog(
    token: McpTokenSummary,
    #[prop(into)] on_close: Callback<()>,
    #[prop(into)] on_saved: Callback<()>,
) -> impl IntoView {
    let toast = use_toast();
    let visible = RwSignal::new(true);
    let closing = RwSignal::new(false);
    let scope = RwSignal::new(token.scope);
    let draft = GrantDraft::new(&token.notes);
    let submitted = RwSignal::new(false);
    let busy = RwSignal::new(false);
    use_close_after_exit(visible, on_close);
    let id = StoredValue::new(token.id.clone());
    let subtitle = format!("「{}」继续使用原来的密钥与有效期。", token.name);
    let close = move || {
        closing.set(true);
        visible.set(false);
    };
    let save = move |e: SubmitEvent| {
        e.prevent_default();
        submitted.set(true);
        if busy.get_untracked() || draft.missing_books() {
            return;
        }
        busy.set(true);
        let (id, sc, grant) = (id.get_value(), scope.get_untracked(), draft.grant());
        spawn_local(async move {
            match update_mcp_token_permissions(id, sc, grant).await {
                Ok(()) => {
                    toast.success("权限已更新，原令牌继续有效");
                    let _ = on_saved.try_run(());
                }
                Err(e) => toast.error(failure_message("保存失败", &e)),
            }
            let _ = busy.try_set(false);
        });
    };
    view! {
        <ModalShell
            visible=visible
            closing=closing
            title="编辑令牌权限"
            overlay_padding="p-4"
            panel_class="mcp-dialog"
        >
            <form class="mcp-form" on:submit=save>
                <div class="mcp-dialog-head">
                    <div>
                        <h2>"编辑令牌权限"</h2>
                        <p>{subtitle.clone()}</p>
                    </div>
                    <IconButton
                        icon=IconName::X
                        label="关闭"
                        size=ButtonSize::Sm
                        onclick=move |_| close()
                    />
                </div>
                <div class="mcp-dialog-body">
                    <ScopeCards scope=scope/>
                    <GrantFields draft=draft submitted=submitted/>
                </div>
                <div class="mcp-dialog-foot">
                    <button
                        class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                        type="button"
                        on:click=move |_| close()
                    >
                        "取消"
                    </button>
                    <button
                        class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                        type="submit"
                        disabled=move || busy.get()
                    >
                        {move || if busy.get() { "保存中…" } else { "保存权限" }}
                    </button>
                </div>
            </form>
        </ModalShell>
    }
}
