//! 令牌对话框：明文（默认遮罩）、各客户端接入配置与一句话接入指令。
use super::token_editor::use_close_after_exit;
use super::{notes_desc, OpenToken};
use crate::api::mcp_tokens::{get_mcp_client_configs, reveal_mcp_token, McpClientConfigs};
use crate::components::forms::FormInput;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconButton, IconName, ModalShell,
};
use crate::utils::error::failure_message;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::sync::Arc;

/// 写入剪贴板；非浏览器目标或被拒绝时返回 `false`。
pub(super) async fn copy(text: &str) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        crate::utils::js::copy_text(text).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = text;
        false
    }
}

/// 一句话接入：取回令牌明文、生成指令并复制。
pub(super) async fn copy_agent_prompt(id: String) -> Result<(), String> {
    let token = reveal_mcp_token(id)
        .await
        .map_err(|e| failure_message("读取令牌失败", &e))?
        .ok_or("这个令牌已失效，请换一个或新建")?;
    let configs = get_mcp_client_configs(token)
        .await
        .map_err(|e| failure_message("生成接入指令失败", &e))?;
    if copy(&configs.agent_prompt).await {
        Ok(())
    } else {
        Err("浏览器拒绝了剪贴板写入，请在令牌的接入配置里手动复制".into())
    }
}

/// 配置标题「Claude Code（.mcp.json / ~/.claude.json）」拆成客户端名与路径。
fn split_title(title: &str) -> (String, String) {
    match title.split_once('（') {
        Some((name, rest)) => (
            name.trim().to_string(),
            rest.trim_end_matches('）').trim().to_string(),
        ),
        None => (title.to_string(), String::new()),
    }
}

/// 令牌明文遮罩：保留前缀与末 4 位。
fn mask(token: &str) -> String {
    let tail: String = token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("ygg_{}{tail}", "•".repeat(20))
}

/// 令牌对话框。刚创建时标题为「令牌已创建」，从列表打开时为「接入配置」。
#[component]
pub(super) fn TokenDialog(token: OpenToken, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let toast = use_toast();
    let visible = RwSignal::new(true);
    let closing = RwSignal::new(false);
    let shown = RwSignal::new(false);
    let client = RwSignal::new(0_usize);
    let configs = RwSignal::new(None::<Result<Arc<McpClientConfigs>, String>>);
    let plaintext = StoredValue::new(token.plaintext.clone());
    {
        let plaintext = token.plaintext.clone();
        spawn_local(async move {
            let result = get_mcp_client_configs(plaintext)
                .await
                .map(Arc::new)
                .map_err(|e| failure_message("配置生成失败", &e));
            let _ = configs.try_set(Some(result));
        });
    }
    // 播完退出动画再卸载。
    use_close_after_exit(visible, on_close);

    let summary = &token.summary;
    let expiry = summary.expires_at.map_or("永不过期".to_string(), |e| {
        format!(
            "{} 到期",
            crate::utils::time::ymd(&crate::utils::time::to_local(e), '-')
        )
    });
    let line = format!(
        "{} · {} · 笔记{} · {expiry}",
        summary.name,
        summary.scope.as_str(),
        notes_desc(summary)
    );
    let created = token.created;
    let title = if created {
        "令牌已创建"
    } else {
        "接入配置"
    };
    let copy_flash = move |text: String, what: &'static str| {
        spawn_local(async move {
            if copy(&text).await {
                toast.success(format!("已复制{what}"));
            } else {
                toast.error("复制失败，请手动选中复制");
            }
        });
    };
    let close = move || {
        closing.set(true);
        visible.set(false);
    };
    let shown_text = Signal::derive(move || {
        plaintext.with_value(|p| if shown.get() { p.clone() } else { mask(p) })
    });
    let ready = move || configs.with(|c| matches!(c, Some(Ok(_))));
    // 一句话指令跟随当前选中的客户端。
    let selected_prompt = move || {
        configs.with(|c| match c {
            Some(Ok(c)) => c
                .snippets
                .get(client.get().min(c.snippets.len().saturating_sub(1)))
                .map(|s| s.agent_prompt()),
            _ => None,
        })
    };

    view! {
        <ModalShell
            visible=visible
            closing=closing
            title=title
            overlay_padding="p-4"
            panel_class="mcp-dialog mcp-token-dialog"
        >
            <div class="mcp-dialog-head">
                {created
                    .then(|| {
                        view! {
                            <span class="mcp-ok" aria-hidden="true">
                                <Icon name=IconName::Check size=18/>
                            </span>
                        }
                    })}
                <div>
                    <h2>{title}</h2>
                    <p>{line.clone()}</p>
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
                    <label class="mcp-label" for="mcp-plaintext">
                        "令牌明文"
                    </label>
                    <div class="mcp-secret">
                        <FormInput
                            id="mcp-plaintext".to_string()
                            class="field settings-mono"
                            readonly=true
                            value=shown_text
                        />
                        <IconButton
                            icon=Signal::derive(move || {
                                if shown.get() { IconName::EyeOff } else { IconName::Eye }
                            })
                            label=Signal::derive(move || {
                                if shown.get() { "隐藏令牌" } else { "显示令牌" }.to_string()
                            })
                            onclick=move |_| shown.update(|s| *s = !*s)
                        />
                        <button
                            class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                            type="button"
                            on:click=move |_| copy_flash(plaintext.get_value(), "令牌")
                        >
                            "复制令牌"
                        </button>
                    </div>
                    <p class="mcp-warn">
                        <Icon name=IconName::TriangleAlert size=14/>
                        <span>
                            "只写入本地客户端配置，不要提交到 Git 或贴进对话。之后可以在列表里重新查看。"
                        </span>
                    </p>
                </div>
                <div class="mcp-configs">
                    <div class="mcp-configs-head">
                        <h3>"接入配置"</h3>
                        <span>"已自动填入服务地址和这个令牌"</span>
                    </div>
                    {move || match configs.get() {
                        None => {
                            view! { <p class="mcp-muted" role="status">"正在生成配置…"</p> }
                                .into_any()
                        }
                        Some(Err(e)) => view! { <p class="mcp-error" role="alert">{e}</p> }.into_any(),
                        Some(Ok(c)) => {
                            let count = c.snippets.len();
                            let current = move || client.get().min(count.saturating_sub(1));
                            let tabs = c
                                .snippets
                                .iter()
                                .enumerate()
                                .map(|(i, s)| {
                                    view! {
                                        <button
                                            type="button"
                                            role="tab"
                                            aria-selected=move || (i == current()).to_string()
                                            on:click=move |_| client.set(i)
                                        >
                                            {split_title(&s.title).0}
                                        </button>
                                    }
                                })
                                .collect_view();
                            view! {
                                <div class="mcp-clients" role="tablist" aria-label="客户端">
                                    {tabs}
                                </div>
                                {move || {
                                    c.snippets
                                        .get(current())
                                        .cloned()
                                        .map(|s| {
                                            let text = s.content.clone();
                                            let html = s.content_html.clone();
                                            view! {
                                                <figure class="mcp-snippet" role="tabpanel">
                                                    <figcaption>
                                                        <span>{split_title(&s.title).1}</span>
                                                        <button
                                                            class=button_class(
                                                                ButtonVariant::Ghost,
                                                                ButtonSize::Xs,
                                                            )
                                                            type="button"
                                                            on:click=move |_| copy_flash(text.clone(), "配置")
                                                        >
                                                            <Icon name=IconName::Copy size=13/>
                                                            "复制配置"
                                                        </button>
                                                    </figcaption>
                                                    <div class="md-content">
                                                        // 与明文框一致：未点「显示」时配置里的令牌也遮罩，复制仍是完整配置。
                                                        <pre>
                                                            <code inner_html=move || {
                                                                if shown.get() {
                                                                    html.clone()
                                                                } else {
                                                                    plaintext
                                                                        .with_value(|p| html.replace(p.as_str(), &mask(p)))
                                                                }
                                                            }></code>
                                                        </pre>
                                                    </div>
                                                </figure>
                                            }
                                        })
                                }}
                            }
                                .into_any()
                        }
                    }}
                </div>
            </div>
            <div class="mcp-dialog-foot">
                <button
                    class=button_class(ButtonVariant::Ghost, ButtonSize::Md)
                    type="button"
                    disabled=move || !ready()
                    on:click=move |_| {
                        if let Some(prompt) = selected_prompt() {
                            copy_flash(prompt, "接入指令，粘贴给 Agent 即可");
                        }
                    }
                >
                    "复制一句话接入指令"
                </button>
                <button
                    class=button_class(ButtonVariant::Secondary, ButtonSize::Md)
                    type="button"
                    on:click=move |_| close()
                >
                    "完成"
                </button>
            </div>
        </ModalShell>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_title_separates_client_and_path() {
        assert_eq!(
            split_title("Claude Code（.mcp.json / ~/.claude.json）"),
            ("Claude Code".into(), ".mcp.json / ~/.claude.json".into())
        );
        assert_eq!(
            split_title("Claude Code CLI"),
            ("Claude Code CLI".into(), String::new())
        );
    }

    #[test]
    fn mask_keeps_only_the_last_four_characters() {
        let masked = mask("ygg_0123456789abcdef");
        assert!(masked.starts_with("ygg_•"));
        assert!(masked.ends_with("cdef"));
        assert!(!masked.contains("0123"));
    }
}
