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

mod configs;
mod token_editor;
mod tokens;

use dioxus::prelude::*;

use crate::components::ui::{button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconName};
use crate::models::mcp_token::{McpTokenSummary, TokenScope};
use configs::{copy_agent_prompt, TokenDialog};
use token_editor::{CreateTokenDialog, EditPermissionsDialog};
use tokens::TokenTable;

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
    let what = if n.write { "读写草稿" } else { "只读" };
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
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn Mcp() -> Element {
    let toast = use_toast();
    let mut tokens = use_signal(|| None::<Vec<McpTokenSummary>>);
    let mut failed = use_signal(|| false);
    let mut reload = use_signal(|| 0_u32);
    let mut creating = use_signal(|| false);
    let mut editing = use_signal(|| None::<McpTokenSummary>);
    let mut opened = use_signal(|| None::<OpenToken>);
    let mut prompt_token = use_signal(|| None::<String>);
    let mut copying = use_signal(|| false);
    let mut copied = use_signal(|| false);

    use_effect(move || {
        let _ = reload();
        spawn(async move {
            match crate::api::mcp_tokens::list_mcp_tokens().await {
                Ok(list) => {
                    tokens.set(Some(list));
                    failed.set(false);
                }
                Err(_) => failed.set(true),
            }
        });
    });

    let active: Vec<McpTokenSummary> = tokens()
        .unwrap_or_default()
        .into_iter()
        .filter(is_active)
        .collect();
    // 一句话接入默认用最新的有效令牌。
    let chosen = prompt_token()
        .filter(|id| active.iter().any(|t| &t.id == id))
        .or_else(|| active.first().map(|t| t.id.clone()));
    let options: Vec<(String, String)> = active
        .iter()
        .map(|t| (t.id.clone(), t.name.clone()))
        .collect();

    let chosen_for_copy = chosen.clone();
    let copy_prompt = move |_| {
        let Some(id) = chosen_for_copy.clone() else {
            return;
        };
        copying.set(true);
        spawn(async move {
            match copy_agent_prompt(id).await {
                Ok(()) => {
                    copied.set(true);
                    toast.success("已复制接入指令，粘贴给 Agent 即可");
                    crate::utils::time::sleep_ms(1600).await;
                    copied.set(false);
                }
                Err(e) => toast.error(e),
            }
            copying.set(false);
        });
    };

    rsx! {
        div { class: "admin-mcp",
            div { class: "admin-page-head",
                div {
                    h1 { "MCP 接入" }
                    p { "为 Codex、Claude Code、Cursor、Cline、Oh-My-Pi、OpenCode 等 AI 客户端签发访问令牌。" }
                }
                div { class: "admin-page-actions",
                    button {
                        class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                        r#type: "button",
                        onclick: move |_| creating.set(true),
                        Icon { name: IconName::Plus, size: 16 }
                        "新建令牌"
                    }
                }
            }

            section { class: "admin-panel mcp-oneline", aria_labelledby: "mcp-oneline-h",
                div { class: "mcp-oneline-head",
                    h2 { id: "mcp-oneline-h", "一句话接入" }
                    span { "复制时自动附上服务地址和所选令牌" }
                }
                p { class: "mcp-oneline-text",
                    "请为当前客户端接入 Yggdrasil MCP，保留已有配置，并验证连接与工具列表。"
                }
                div { class: "mcp-oneline-actions",
                    if options.is_empty() {
                        span { class: "mcp-muted", "还没有可用的令牌，先新建一个。" }
                    } else {
                        span { class: "mcp-muted", "使用令牌" }
                        crate::components::forms::FormSelect {
                            id: "mcp-prompt-token",
                            value: chosen.clone().unwrap_or_default(),
                            options,
                            trigger_class: crate::components::forms::FORM_SELECT_COMPACT_CLASS,
                            aria_label: "使用令牌",
                            onchange: move |id: String| prompt_token.set(Some(id)),
                        }
                    }
                    button {
                        class: "{button_class(ButtonVariant::Primary, ButtonSize::Sm)} mcp-copy-prompt",
                        r#type: "button",
                        "data-copied": copied().then_some("true"),
                        disabled: chosen.is_none() || copying(),
                        onclick: copy_prompt,
                        Icon { name: if copied() { IconName::Check } else { IconName::Copy }, size: 14 }
                        if copied() { "已复制" } else if copying() { "准备中…" } else { "复制给 Agent" }
                    }
                }
            }

            TokenTable {
                tokens: tokens(),
                failed: failed(),
                on_retry: move |_| reload += 1,
                on_open: move |t: OpenToken| opened.set(Some(t)),
                on_edit: move |t: McpTokenSummary| editing.set(Some(t)),
                on_changed: move |_| reload += 1,
            }
        }

        if creating() {
            CreateTokenDialog {
                on_close: move |_| creating.set(false),
                on_created: move |t: OpenToken| {
                    creating.set(false);
                    reload += 1;
                    opened.set(Some(t));
                },
            }
        }
        if let Some(token) = editing() {
            EditPermissionsDialog {
                key: "{token.id}",
                token,
                on_close: move |_| editing.set(None),
                on_saved: move |_| {
                    editing.set(None);
                    reload += 1;
                },
            }
        }
        if let Some(token) = opened() {
            TokenDialog { key: "{token.summary.id}", token, on_close: move |_| opened.set(None) }
        }
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
    fn notes_desc_summarizes_grant() {
        assert_eq!(notes_desc(&token(NoteGrant::default())), "未授权");
        let all = NoteGrant {
            read: true,
            write: false,
            notebook_ids: None,
        };
        assert_eq!(notes_desc(&token(all)), "只读 · 全部笔记本");
        let some = NoteGrant {
            read: true,
            write: true,
            notebook_ids: Some(vec![1, 2]),
        };
        assert_eq!(notes_desc(&token(some)), "读写草稿 · 2 个笔记本");
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
