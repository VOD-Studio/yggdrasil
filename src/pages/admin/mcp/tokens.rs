//! 已签发的令牌表格：名称、文章权限、笔记授权、有效期、最近使用与操作。
use super::{is_active, notes_desc, OpenToken};
use crate::api::mcp_tokens::{reveal_mcp_token, revoke_mcp_token};
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::{
    use_toast, ConfirmDialog, IconName, LoadError, Menu, MenuItem, StatusDot,
};
use crate::models::mcp_token::McpTokenSummary;
use crate::utils::error::failure_message;
use crate::utils::time::{to_local, ymd};
use leptos::prelude::*;
use leptos::task::spawn_local;

fn date(at: Option<chrono::DateTime<chrono::Utc>>, none: &str) -> String {
    at.map_or_else(|| none.to_string(), |d| ymd(&to_local(d), '-'))
}

#[component]
pub(super) fn TokenTable(
    #[prop(into)] tokens: Signal<Option<Vec<McpTokenSummary>>>,
    #[prop(into)] failed: Signal<bool>,
    #[prop(into)] on_retry: Callback<()>,
    #[prop(into)] on_open: Callback<OpenToken>,
    #[prop(into)] on_edit: Callback<McpTokenSummary>,
    #[prop(into)] on_changed: Callback<()>,
) -> impl IntoView {
    let toast = use_toast();
    let revoking = RwSignal::new(None::<McpTokenSummary>);
    let confirm = RwSignal::new(false);
    let busy = RwSignal::new(None::<String>);

    let open = move |token: McpTokenSummary| {
        busy.set(Some(token.id.clone()));
        spawn_local(async move {
            match reveal_mcp_token(token.id.clone()).await {
                Ok(Some(plaintext)) => {
                    let _ = on_open.try_run(OpenToken {
                        summary: token,
                        plaintext,
                        created: false,
                    });
                }
                Ok(None) => toast.error("这个令牌已失效，无法查看"),
                Err(e) => toast.error(failure_message("读取令牌失败", &e)),
            }
            let _ = busy.try_set(None);
        });
    };
    let revoke = move |_| {
        let Some(token) = revoking.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match revoke_mcp_token(token.id.clone()).await {
                Ok(()) => {
                    toast.success(format!("已吊销「{}」", token.name));
                    let _ = on_changed.try_run(());
                }
                Err(e) => toast.error(failure_message("吊销失败", &e)),
            }
        });
    };

    let row = move |token: McpTokenSummary| {
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
            MenuItem::new("revoke", "吊销")
                .icon(IconName::Trash)
                .danger()
                .separated(),
        ];
        if !active {
            items = items
                .into_iter()
                .map(|i| i.disabled("令牌已失效"))
                .collect();
        }
        let busy_id = token.id.clone();
        let t = token.clone();
        let sub = match state {
            Some(state) => view! { <StatusDot label=state/> }.into_any(),
            None => format!("创建于 {}", date(Some(token.created_at), "")).into_any(),
        };
        view! {
            <tr
                data-inactive=(!active).then_some("true")
                aria-busy=move || busy.with(|b| b.as_ref() == Some(&busy_id)).then_some("true")
            >
                <td>
                    <div class="mcp-token-name">{token.name.clone()}</div>
                    <div class="mcp-token-sub">{sub}</div>
                </td>
                <td>
                    <span class="mcp-scope-pill" data-scope=token.scope.as_str()>
                        {token.scope.as_str()}
                    </span>
                </td>
                <td class="mcp-col-notes">{notes_desc(&token)}</td>
                <td class="admin-table-muted">{date(token.expires_at, "永不过期")}</td>
                <td class="admin-table-muted mcp-col-used">{date(token.last_used_at, "从未使用")}</td>
                <td class="admin-table-actions">
                    <Menu
                        label="令牌操作"
                        items=items
                        on_select=move |id: &'static str| match id {
                            "open" => open(t.clone()),
                            "edit" => on_edit.run(t.clone()),
                            "revoke" => {
                                revoking.set(Some(t.clone()));
                                confirm.set(true);
                            }
                            _ => {}
                        }
                    />
                </td>
            </tr>
        }
    };

    view! {
        <section class="mcp-tokens" aria-labelledby="mcp-tokens-h">
            <h2 id="mcp-tokens-h" class="mcp-section-title">
                "已签发的令牌"
            </h2>
            {move || {
                if failed.get() {
                    return view! {
                        <LoadError
                            title="令牌列表加载失败"
                            on_retry=move |_| on_retry.run(())
                        />
                    }
                        .into_any();
                }
                match tokens.get() {
                    Some(list) if list.is_empty() => {
                        view! {
                            <div class="admin-panel">
                                <EmptyState
                                    icon=IconName::KeyRound
                                    title="还没有令牌"
                                    description="新建一个令牌，把它写进 AI 客户端的配置即可连接。"
                                />
                            </div>
                        }
                            .into_any()
                    }
                    Some(list) => {
                        view! {
                            <div class="admin-panel admin-table-wrap">
                                <table class="admin-table mcp-table">
                                    <thead>
                                        <tr>
                                            <th>"名称"</th>
                                            <th>"文章权限"</th>
                                            <th class="mcp-col-notes">"笔记授权"</th>
                                            <th>"有效期至"</th>
                                            <th class="mcp-col-used">"最近使用"</th>
                                            <th class="admin-table-actions">
                                                <span class="sr-only">"操作"</span>
                                            </th>
                                        </tr>
                                    </thead>
                                    <tbody>{list.into_iter().map(row).collect_view()}</tbody>
                                </table>
                            </div>
                        }
                            .into_any()
                    }
                    None => {
                        view! {
                            <div class="admin-panel" aria-hidden="true">
                                {(0..3)
                                    .map(|_| {
                                        view! {
                                            <div class="mcp-skeleton-row">
                                                <SkeletonBox class="h-4 w-40 rounded"/>
                                                <SkeletonBox class="h-4 w-16 rounded"/>
                                                <SkeletonBox class="h-4 w-24 rounded"/>
                                            </div>
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        }
                            .into_any()
                    }
                }
            }}
        </section>
        <ConfirmDialog
            visible=confirm
            title="吊销这个令牌？"
            description=move || {
                format!(
                    "使用「{}」的客户端会立刻无法连接，吊销后不能恢复。",
                    revoking.get().map(|t| t.name).unwrap_or_default(),
                )
            }
            confirm_label="吊销"
            icon=IconName::KeyRound
            on_confirm=revoke
        />
    }
}
