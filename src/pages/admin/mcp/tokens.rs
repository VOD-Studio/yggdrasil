//! 已签发的令牌表格：名称、文章权限、笔记授权、有效期、最近使用与操作。
use super::{is_active, notes_desc, OpenToken};
use crate::api::mcp_tokens::{reveal_mcp_token, revoke_mcp_token};
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::{use_toast, ConfirmDialog, IconName, LoadError, Menu, MenuItem};
use crate::models::mcp_token::McpTokenSummary;
use crate::utils::error::failure_message;
use crate::utils::time::to_local;
use dioxus::prelude::*;

fn date(at: Option<chrono::DateTime<chrono::Utc>>, none: &str) -> String {
    at.map_or_else(
        || none.to_string(),
        |d| to_local(d).format("%Y-%m-%d").to_string(),
    )
}

#[component]
pub(super) fn TokenTable(
    tokens: Option<Vec<McpTokenSummary>>,
    failed: bool,
    on_retry: EventHandler<()>,
    on_open: EventHandler<OpenToken>,
    on_edit: EventHandler<McpTokenSummary>,
    on_changed: EventHandler<()>,
) -> Element {
    let toast = use_toast();
    let mut revoking = use_signal(|| None::<McpTokenSummary>);
    let mut confirm = use_signal(|| false);
    let mut busy = use_signal(|| None::<String>);

    let mut open = move |token: McpTokenSummary| {
        busy.set(Some(token.id.clone()));
        spawn(async move {
            match reveal_mcp_token(token.id.clone()).await {
                Ok(Some(plaintext)) => on_open.call(OpenToken {
                    summary: token,
                    plaintext,
                    created: false,
                }),
                Ok(None) => toast.error("这个令牌已失效，无法查看"),
                Err(e) => toast.error(failure_message("读取令牌失败", &e)),
            }
            busy.set(None);
        });
    };
    let revoke = move |_| {
        let Some(token) = revoking() else {
            return;
        };
        spawn(async move {
            match revoke_mcp_token(token.id.clone()).await {
                Ok(()) => {
                    toast.success(format!("已吊销「{}」", token.name));
                    on_changed.call(());
                }
                Err(e) => toast.error(failure_message("吊销失败", &e)),
            }
        });
    };

    rsx! {
        section { class: "mcp-tokens", aria_labelledby: "mcp-tokens-h",
            h2 { id: "mcp-tokens-h", class: "mcp-section-title", "已签发的令牌" }
            if failed {
                LoadError { title: "令牌列表加载失败", on_retry: move |_| on_retry.call(()) }
            } else if let Some(list) = tokens {
                if list.is_empty() {
                    div { class: "admin-panel",
                        EmptyState {
                            icon: IconName::KeyRound,
                            title: "还没有令牌",
                            description: "新建一个令牌，把它写进 AI 客户端的配置即可连接。",
                        }
                    }
                } else {
                    div { class: "admin-panel admin-table-wrap",
                        table { class: "admin-table mcp-table",
                            thead {
                                tr {
                                    th { "名称" }
                                    th { "文章权限" }
                                    th { class: "mcp-col-notes", "笔记授权" }
                                    th { "有效期至" }
                                    th { class: "mcp-col-used", "最近使用" }
                                    th { class: "admin-table-actions", span { class: "sr-only", "操作" } }
                                }
                            }
                            tbody {
                                for token in list.into_iter() {
                                    {
                                        let active = is_active(&token);
                                        let state = if token.revoked_at.is_some() {
                                            Some("已吊销")
                                        } else if !active {
                                            Some("已过期")
                                        } else {
                                            None
                                        };
                                        let mut items = vec![
                                            MenuItem::new("open", "接入配置").icon(IconName::Plug),
                                            MenuItem::new("edit", "编辑权限").icon(IconName::Shield),
                                            MenuItem::new("revoke", "吊销").icon(IconName::Trash).danger().separated(),
                                        ];
                                        if !active {
                                            items = items
                                                .into_iter()
                                                .map(|i| i.disabled("令牌已失效"))
                                                .collect();
                                        }
                                        let t = token.clone();
                                        rsx! {
                                            tr {
                                                key: "{token.id}",
                                                "data-inactive": (!active).then_some("true"),
                                                "aria-busy": (busy() == Some(token.id.clone())).then_some("true"),
                                                td {
                                                    div { class: "mcp-token-name", "{token.name}" }
                                                    div { class: "mcp-token-sub",
                                                        if let Some(state) = state {
                                                            span { class: "status-dot", "data-tone": "muted", "{state}" }
                                                        } else {
                                                            "创建于 {date(Some(token.created_at), \"\")}"
                                                        }
                                                    }
                                                }
                                                td {
                                                    span { class: "mcp-scope-pill", "data-scope": token.scope.as_str(), "{token.scope.as_str()}" }
                                                }
                                                td { class: "mcp-col-notes", "{notes_desc(&token)}" }
                                                td { class: "admin-table-muted", "{date(token.expires_at, \"永不过期\")}" }
                                                td { class: "admin-table-muted mcp-col-used", "{date(token.last_used_at, \"从未使用\")}" }
                                                td { class: "admin-table-actions",
                                                    Menu {
                                                        label: "令牌操作",
                                                        items,
                                                        on_select: move |id: &'static str| match id {
                                                            "open" => open(t.clone()),
                                                            "edit" => on_edit.call(t.clone()),
                                                            "revoke" => {
                                                                revoking.set(Some(t.clone()));
                                                                confirm.set(true);
                                                            }
                                                            _ => {}
                                                        },
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "admin-panel", aria_hidden: "true",
                    for i in 0..3 {
                        div { key: "{i}", class: "mcp-skeleton-row",
                            SkeletonBox { class: "h-4 w-40 rounded" }
                            SkeletonBox { class: "h-4 w-16 rounded" }
                            SkeletonBox { class: "h-4 w-24 rounded" }
                        }
                    }
                }
            }
        }
        ConfirmDialog {
            visible: confirm,
            title: "吊销这个令牌？",
            description: format!(
                "使用「{}」的客户端会立刻无法连接，吊销后不能恢复。",
                revoking().map(|t| t.name).unwrap_or_default()
            ),
            confirm_label: "吊销",
            icon: IconName::KeyRound,
            on_confirm: revoke,
        }
    }
}
