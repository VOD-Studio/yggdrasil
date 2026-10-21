//! 令牌对话框：明文（默认遮罩）、各客户端接入配置与一句话接入指令。
use super::{notes_desc, OpenToken};
use crate::api::mcp_tokens::{get_mcp_client_configs, reveal_mcp_token, McpClientConfigs};
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconButton, IconName, ModalShell,
};
use dioxus::prelude::*;

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
        .map_err(|e| format!("读取令牌失败：{e}"))?
        .ok_or("这个令牌已失效，请换一个或新建")?;
    let configs = get_mcp_client_configs(token)
        .await
        .map_err(|e| format!("生成接入指令失败：{e}"))?;
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
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn TokenDialog(token: OpenToken, on_close: EventHandler<()>) -> Element {
    let toast = use_toast();
    let mut visible = use_signal(|| true);
    let mut closing = use_signal(|| false);
    let mut shown = use_signal(|| false);
    let mut client = use_signal(|| 0_usize);
    let mut configs = use_signal(|| None::<Result<McpClientConfigs, String>>);
    let plaintext = token.plaintext.clone();
    use_hook({
        let plaintext = plaintext.clone();
        move || {
            spawn(async move {
                configs.set(Some(
                    get_mcp_client_configs(plaintext)
                        .await
                        .map_err(|e| e.to_string()),
                ));
            });
        }
    });
    // 播完退出动画再卸载。
    use_effect(move || {
        if !visible() {
            spawn(async move {
                crate::utils::time::sleep_ms(crate::components::ui::EXIT_ANIM_MS).await;
                on_close.call(());
            });
        }
    });
    let summary = &token.summary;
    let expiry = summary.expires_at.map_or("永不过期".to_string(), |e| {
        crate::utils::time::to_local(e)
            .format("%Y-%m-%d 到期")
            .to_string()
    });
    let line = format!(
        "{} · {} · 笔记{} · {expiry}",
        summary.name,
        summary.scope.as_str(),
        notes_desc(summary)
    );
    let title = if token.created {
        "令牌已创建"
    } else {
        "接入配置"
    };
    let copy_flash = move |text: String, what: &'static str| {
        spawn(async move {
            if copy(&text).await {
                toast.success(format!("已复制{what}"));
            } else {
                toast.error("复制失败，请手动选中复制");
            }
        });
    };
    let shown_text = if shown() {
        plaintext.clone()
    } else {
        mask(&plaintext)
    };
    let loaded = configs();
    let ready = matches!(loaded, Some(Ok(_)));
    // 一句话指令跟随当前选中的客户端。
    let selected_prompt = match &loaded {
        Some(Ok(c)) => c
            .snippets
            .get(client().min(c.snippets.len().saturating_sub(1)))
            .map(|s| s.agent_prompt()),
        _ => None,
    };

    rsx! {
        ModalShell {
            visible,
            closing,
            title,
            overlay_padding: "p-4",
            panel_class: "mcp-dialog mcp-token-dialog",
            div { class: "mcp-dialog-head",
                if token.created {
                    span { class: "mcp-ok", aria_hidden: "true", Icon { name: IconName::Check, size: 18 } }
                }
                div {
                    h2 { "{title}" }
                    p { "{line}" }
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
                    label { class: "mcp-label", r#for: "mcp-plaintext", "令牌明文" }
                    div { class: "mcp-secret",
                        input {
                            id: "mcp-plaintext",
                            class: "field settings-mono",
                            readonly: true,
                            value: "{shown_text}",
                        }
                        IconButton {
                            icon: if shown() { IconName::EyeOff } else { IconName::Eye },
                            label: if shown() { "隐藏令牌" } else { "显示令牌" },
                            onclick: move |_| shown.toggle(),
                        }
                        button {
                            class: button_class(ButtonVariant::Primary, ButtonSize::Md),
                            r#type: "button",
                            onclick: {
                                let p = plaintext.clone();
                                move |_| copy_flash(p.clone(), "令牌")
                            },
                            "复制令牌"
                        }
                    }
                    p { class: "mcp-warn",
                        Icon { name: IconName::TriangleAlert, size: 14 }
                        span { "只写入本地客户端配置，不要提交到 Git 或贴进对话。之后可以在列表里重新查看。" }
                    }
                }
                div { class: "mcp-configs",
                    div { class: "mcp-configs-head",
                        h3 { "接入配置" }
                        span { "已自动填入服务地址和这个令牌" }
                    }
                    match loaded {
                        None => rsx! {
                            p { class: "mcp-muted", role: "status", "正在生成配置…" }
                        },
                        Some(Err(e)) => rsx! {
                            p { class: "mcp-error", role: "alert", "配置生成失败：{e}" }
                        },
                        Some(Ok(c)) => {
                            let current = client().min(c.snippets.len().saturating_sub(1));
                            let snippet = c.snippets.get(current).cloned();
                            rsx! {
                                div { class: "mcp-clients", role: "tablist", aria_label: "客户端",
                                    for (i, s) in c.snippets.iter().enumerate() {
                                        button {
                                            key: "{i}",
                                            r#type: "button",
                                            role: "tab",
                                            aria_selected: "{i == current}",
                                            onclick: move |_| client.set(i),
                                            "{split_title(&s.title).0}"
                                        }
                                    }
                                }
                                if let Some(s) = snippet {
                                    figure { class: "mcp-snippet", role: "tabpanel",
                                        figcaption {
                                            span { "{split_title(&s.title).1}" }
                                            button {
                                                class: button_class(ButtonVariant::Ghost, ButtonSize::Xs),
                                                r#type: "button",
                                                onclick: {
                                                    let text = s.content.clone();
                                                    move |_| copy_flash(text.clone(), "配置")
                                                },
                                                Icon { name: IconName::Copy, size: 13 }
                                                "复制配置"
                                            }
                                        }
                                        div { class: "md-content",
                                            // 与明文框一致：未点「显示」时配置里的令牌也遮罩，复制仍是完整配置。
                                            pre {
                                                code {
                                                    dangerous_inner_html: if shown() {
                                                        s.content_html.clone()
                                                    } else {
                                                        s.content_html.replace(&plaintext, &mask(&plaintext))
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
            div { class: "mcp-dialog-foot",
                button {
                    class: button_class(ButtonVariant::Ghost, ButtonSize::Md),
                    r#type: "button",
                    disabled: !ready,
                    onclick: move |_| {
                        if let Some(prompt) = selected_prompt.clone() {
                            copy_flash(prompt, "接入指令，粘贴给 Agent 即可");
                        }
                    },
                    "复制一句话接入指令"
                }
                button {
                    class: button_class(ButtonVariant::Secondary, ButtonSize::Md),
                    r#type: "button",
                    onclick: move |_| {
                        closing.set(true);
                        visible.set(false);
                    },
                    "完成"
                }
            }
        }
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
