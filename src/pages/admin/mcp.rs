//! 管理后台「MCP 接入」页面（`/admin/mcp`）。
//!
//! 为 Codex、Claude Code、Cursor、Cline、Oh-My-Pi、OpenCode 等 AI 客户端签发 bearer
//! 令牌。页面分两块：
//! - 一句话接入：选一个有效令牌，复制带服务地址与令牌的接入指令，粘贴给 Agent。
//! - 已签发的令牌：文章权限、笔记授权、有效期与最近使用；可查看接入配置、编辑权限、
//!   吊销（确认对话框）。
//!
//! 「新建令牌」打开创建对话框；创建成功后进入令牌对话框：明文默认遮罩，可显示、
//! 复制，并按客户端给出已填好地址与令牌的配置片段。配置由服务端生成
//! （`get_mcp_client_configs`，`APP_BASE_URL` 只在服务端可读）。样式见
//! styles/pages/admin-mcp.css。
//!
//! 令牌列表只在浏览器里请求（`Effect` + `spawn_local` 写镜像信号，带世代号防竞态）：
//! 列表会刷新，且位于外层 `<Suspense>` 内，不能用 `Resource`。

mod configs;
mod token_editor;
mod tokens;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::mcp_tokens::list_mcp_tokens;
use crate::components::admin_layout::AdminPageHead;
use crate::components::forms::{FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::ui::{button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconName};
use crate::models::mcp_token::{McpTokenSummary, TokenScope};
use configs::{copy_agent_prompt, TokenDialog};
use token_editor::{CreateTokenDialog, EditPermissionsDialog};
use tokens::TokenTable;

/// `/admin/mcp` 的懒加载路由。令牌数据只在客户端请求，`data()` 无需创建资源。
pub struct McpRoute;

#[lazy_route]
impl LazyRoute for McpRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <Mcp/> }.into_any()
    }
}

/// 文章权限的说明。
pub(super) fn scope_desc(scope: TokenScope) -> &'static str {
    match scope {
        TokenScope::Read => "仅查询已发布文章",
        TokenScope::Write => "read + 文章、评论、标签、媒体的增删改",
        TokenScope::Admin => "write + 站点设置与代码运行器",
    }
}

/// 笔记授权的一行描述。
pub(super) fn notes_desc(token: &McpTokenSummary) -> String {
    let n = &token.notes;
    if !n.read {
        return "未授权".into();
    }
    let what = if n.write {
        "读写、公开与收录"
    } else {
        "只读"
    };
    match &n.notebook_ids {
        Some(ids) => format!("{what} · {} 个笔记本", ids.len()),
        None => format!("{what} · 全部笔记本"),
    }
}

/// 令牌当前可用：未吊销、未过期。
pub(super) fn is_active(token: &McpTokenSummary) -> bool {
    token.revoked_at.is_none() && token.expires_at.is_none_or(|e| e > chrono::Utc::now())
}

/// 令牌对话框里要展示的内容。
#[derive(Clone, PartialEq)]
pub(super) struct OpenToken {
    pub summary: McpTokenSummary,
    pub plaintext: String,
    /// 刚创建（标题与提示不同）。
    pub created: bool,
}

/// 管理后台 MCP 接入页面。
#[component]
pub fn Mcp() -> impl IntoView {
    let toast = use_toast();
    let tokens = RwSignal::new(None::<Vec<McpTokenSummary>>);
    let failed = RwSignal::new(false);
    let reload = RwSignal::new(0_u32);
    let creating = RwSignal::new(false);
    let editing = RwSignal::new(None::<McpTokenSummary>);
    let opened = RwSignal::new(None::<OpenToken>);
    let prompt_token = RwSignal::new(None::<String>);
    let copying = RwSignal::new(false);
    let copied = RwSignal::new(false);

    // `reload` 变化时重新取列表；刷新期间保留旧数据。仅客户端执行（Effect 不在 SSR 运行）。
    let fetch_generation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        reload.track();
        let Some(current) = fetch_generation.try_update_value(|g| {
            *g += 1;
            *g
        }) else {
            return;
        };
        spawn_local(async move {
            let result = list_mcp_tokens().await;
            if fetch_generation.try_get_value() != Some(current) {
                return;
            }
            match result {
                Ok(list) => {
                    let _ = tokens.try_set(Some(list));
                    let _ = failed.try_set(false);
                }
                Err(_) => {
                    let _ = failed.try_set(true);
                }
            }
        });
    });

    let active = Memo::new(move |_| {
        tokens.with(|list| {
            list.iter()
                .flatten()
                .filter(|t| is_active(t))
                .cloned()
                .collect::<Vec<McpTokenSummary>>()
        })
    });
    // 一句话接入默认用最新的有效令牌。
    let chosen = Memo::new(move |_| {
        active.with(|active| {
            prompt_token
                .get()
                .filter(|id| active.iter().any(|t| &t.id == id))
                .or_else(|| active.first().map(|t| t.id.clone()))
        })
    });
    let options = Signal::derive(move || {
        active.with(|active| {
            active
                .iter()
                .map(|t| (t.id.clone(), t.name.clone()))
                .collect::<Vec<(String, String)>>()
        })
    });

    // 仅在“有无可用令牌”切换时重建下拉，避免选项变化时收起已展开的面板。
    let has_options = Memo::new(move |_| !options.with(Vec::is_empty));

    let copy_prompt = move |_| {
        let Some(id) = chosen.get_untracked() else {
            return;
        };
        copying.set(true);
        spawn_local(async move {
            match copy_agent_prompt(id).await {
                Ok(()) => {
                    let _ = copied.try_set(true);
                    toast.success("已复制接入指令，粘贴给 Agent 即可");
                    crate::utils::time::sleep_ms(1600).await;
                    let _ = copied.try_set(false);
                }
                Err(e) => toast.error(e),
            }
            let _ = copying.try_set(false);
        });
    };

    view! {
        <div class="admin-mcp">
            <AdminPageHead
                title="MCP 接入"
                actions=move || {
                    view! {
                        <button
                            class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                            type="button"
                            on:click=move |_| creating.set(true)
                        >
                            <Icon name=IconName::Plus size=16/>
                            "新建令牌"
                        </button>
                    }
                }
            >
                "为 Codex、Claude Code、Cursor、Cline、Oh-My-Pi、OpenCode 等 AI 客户端签发访问令牌。"
            </AdminPageHead>

            <section class="admin-panel mcp-oneline" aria-labelledby="mcp-oneline-h">
                <div class="mcp-oneline-head">
                    <h2 id="mcp-oneline-h">"一句话接入"</h2>
                    <span>"复制时自动附上服务地址和所选令牌"</span>
                </div>
                <p class="mcp-oneline-text">
                    "请为当前客户端接入 Yggdrasil MCP，保留已有配置，并验证连接与工具列表。"
                </p>
                <div class="mcp-oneline-actions">
                    {move || {
                        if !has_options.get() {
                            view! { <span class="mcp-muted">"还没有可用的令牌，先新建一个。"</span> }
                                .into_any()
                        } else {
                            view! {
                                <span class="mcp-muted">"使用令牌"</span>
                                <FormSelect
                                    id="mcp-prompt-token"
                                    value=Signal::derive(move || chosen.get().unwrap_or_default())
                                    options=options
                                    trigger_class=FORM_SELECT_COMPACT_CLASS
                                    aria_label="使用令牌"
                                    onchange=move |id: String| prompt_token.set(Some(id))
                                />
                            }
                                .into_any()
                        }
                    }}
                    <button
                        class=format!(
                            "{} mcp-copy-prompt",
                            button_class(ButtonVariant::Primary, ButtonSize::Sm),
                        )
                        type="button"
                        data-copied=move || copied.get().then_some("true")
                        disabled=move || chosen.get().is_none() || copying.get()
                        on:click=copy_prompt
                    >
                        {move || {
                            view! {
                                <Icon
                                    name=if copied.get() { IconName::Check } else { IconName::Copy }
                                    size=14
                                />
                            }
                        }}
                        {move || {
                            if copied.get() {
                                "已复制"
                            } else if copying.get() {
                                "准备中…"
                            } else {
                                "复制给 Agent"
                            }
                        }}
                    </button>
                </div>
            </section>

            <TokenTable
                tokens=tokens
                failed=failed
                on_retry=move |_| reload.update(|n| *n += 1)
                on_open=move |t: OpenToken| opened.set(Some(t))
                on_edit=move |t: McpTokenSummary| editing.set(Some(t))
                on_changed=move |_| reload.update(|n| *n += 1)
            />
        </div>

        {move || {
            creating
                .get()
                .then(|| {
                    view! {
                        <CreateTokenDialog
                            on_close=move |_| creating.set(false)
                            on_created=move |t: OpenToken| {
                                creating.set(false);
                                reload.update(|n| *n += 1);
                                opened.set(Some(t));
                            }
                        />
                    }
                })
        }}
        {move || {
            editing
                .get()
                .map(|token| {
                    view! {
                        <EditPermissionsDialog
                            token=token
                            on_close=move |_| editing.set(None)
                            on_saved=move |_| {
                                editing.set(None);
                                reload.update(|n| *n += 1);
                            }
                        />
                    }
                })
        }}
        {move || {
            opened
                .get()
                .map(|token| {
                    view! { <TokenDialog token=token on_close=move |_| opened.set(None)/> }
                })
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::mcp_token::NoteGrant;

    fn token(notes: NoteGrant) -> McpTokenSummary {
        McpTokenSummary {
            id: "t".into(),
            name: "t".into(),
            scope: TokenScope::Read,
            notes,
            created_at: chrono::Utc::now(),
            expires_at: None,
            last_used_at: None,
            revoked_at: None,
        }
    }

    #[test]
    fn expired_or_revoked_tokens_are_inactive() {
        let mut t = token(NoteGrant::default());
        assert!(is_active(&t));
        t.expires_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        assert!(!is_active(&t));
        t.expires_at = None;
        t.revoked_at = Some(chrono::Utc::now());
        assert!(!is_active(&t));
    }
}
