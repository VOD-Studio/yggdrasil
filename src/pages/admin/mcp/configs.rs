//! Client configuration and clipboard feedback.
use super::McpPageState;
use crate::api::mcp_tokens::{get_mcp_client_configs, McpClientConfigs, McpConfigSnippet};
use crate::components::forms::FormInput;
use crate::components::skeletons::atoms::SkeletonBox;
use dioxus::prelude::*;

/// 配置骨架屏的尺寸变化：(标题宽度 px, 代码块高度 px)。
///
/// 按真实配置片段的长短差异给出不同占位尺寸，让骨架屏更像即将出现的真实内容
/// 而非千篇一律的等高块。
#[cfg(target_arch = "wasm32")]
const CONFIG_SKELETON_SHAPES: &[(&str, &str)] = &[
    ("width: 320px;", "height: 152px;"), // Oh-My-Pi JSON
    ("width: 280px;", "height: 88px;"),  // Codex TOML
    ("width: 280px;", "height: 168px;"), // OpenCode JSON
    ("width: 260px;", "height: 168px;"), // Claude Code JSON
    ("width: 200px;", "height: 136px;"), // Cursor JSON
    ("width: 220px;", "height: 168px;"), // Cline JSON
    ("width: 160px;", "height: 104px;"), // 通用 JSON
    ("width: 140px;", "height: 56px;"),  // CLI 一行命令
];

/// 客户端配置卡片（需先在令牌列表点「用于配置」或手动粘贴）。
#[cfg(target_arch = "wasm32")]
#[component]
pub(super) fn ConfigCard() -> Element {
    let state: McpPageState = use_context();
    let mut manual_token = use_signal(String::new);
    let mut configs = use_signal(|| None::<McpClientConfigs>);
    let mut loading = use_signal(|| false);
    let mut request_gen = use_signal(|| 0_u64);
    let mut config_error = use_signal(|| None::<String>);

    let config_token = state.config_token;
    // 令牌列表与手动输入走同一个生成流程。
    use_effect(move || {
        if let Some(token) = config_token() {
            manual_token.set(token);
        }
    });

    use_effect(move || {
        let token = manual_token().trim().to_string();
        let generation = request_gen.peek().wrapping_add(1);
        request_gen.set(generation);
        configs.set(None);
        config_error.set(None);
        loading.set(!token.is_empty());
        if token.is_empty() {
            return;
        }
        spawn(async move {
            crate::utils::time::sleep_ms(180).await;
            if *request_gen.peek() != generation {
                return;
            }
            let result = get_mcp_client_configs(token.clone()).await;
            // 连续输入/切换令牌时，旧响应不得覆盖当前接入信息。
            if *request_gen.peek() != generation || manual_token.peek().trim() != token {
                return;
            }
            match result {
                Ok(c) => configs.set(Some(c)),
                Err(_) => config_error.set(Some("配置生成失败，请重新填入令牌后重试。".into())),
            }
            loading.set(false);
        });
    });

    rsx! {
        div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 p-6 sm:p-8 flex flex-col gap-6",
            div { class: "flex flex-col gap-1 border-b border-[var(--color-paper-border)]/60 pb-4",
                div { class: "flex items-center gap-2.5",
                    svg {
                        class: "w-5 h-5 text-[var(--color-paper-accent)]",
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        polyline { points: "4 17 10 11 4 5" }
                        line { x1: "12", y1: "19", x2: "20", y2: "19" }
                    }
                    h2 { class: "text-lg sm:text-xl font-bold text-[var(--color-paper-primary)]",
                        "客户端接入配置"
                    }
                }
                p { class: "text-sm text-[var(--color-paper-secondary)] mt-1",
                    "在上方令牌列表点击「用于配置」自动填入，或直接粘贴令牌明文（ygg_...）。"
                }
            }

            div { class: "flex flex-col gap-2",
                label { class: "text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)]",
                    "令牌明文 (Bearer Token)"
                }
                div { class: "relative",
                    FormInput {
                        r#type: "text",
                        placeholder: "ygg_xxxxxxxxxxxxxxxxxxxx",
                        value: manual_token(),
                        oninput: move |v: String| {
                            configs.set(None);
                            loading.set(!v.trim().is_empty());
                            manual_token.set(v);
                        },
                    }
                }
            }

            AgentSetupCard {
                key: "{request_gen()}",
                prompt: configs().map(|c| c.agent_prompt),
                loading: loading(),
            }

            if let Some(message) = config_error() {
                p { role: "alert", class: "text-sm text-red-500", "{message}" }
            }

            if loading() {
                div { class: "flex flex-col gap-4",
                    for i in 0..CONFIG_SKELETON_SHAPES.len() {
                        div { key: "{i}", class: "flex flex-col gap-2",
                            div { class: "flex items-center justify-between",
                                SkeletonBox {
                                    class: "h-4 rounded",
                                    style: CONFIG_SKELETON_SHAPES[i].0,
                                }
                                SkeletonBox { class: "h-7 w-14 rounded-full" }
                            }
                            SkeletonBox {
                                class: "rounded-lg",
                                style: CONFIG_SKELETON_SHAPES[i].1,
                            }
                        }
                    }
                }
            } else if let Some(c) = configs() {
                div { class: "grid grid-cols-1 gap-5",
                    for (i, snippet) in c.snippets.iter().enumerate() {
                        div {
                            key: "{snippet.title}",
                            class: "animate-row-enter",
                            style: "animation-delay: {i * 40}ms",
                            ConfigSnippet { snippet: snippet.clone() }
                        }
                    }
                }
            } else {
                p { class: "text-sm text-[var(--color-paper-tertiary)] py-8 text-center",
                    "填入令牌明文后此处自动生成针对各客户端的接入配置代码。"
                }
            }
        }
    }
}

/// 自动识别客户端的一句话接入入口。
#[cfg(target_arch = "wasm32")]
#[component]
fn AgentSetupCard(prompt: Option<String>, loading: bool) -> Element {
    let disabled = loading || prompt.is_none();
    let prompt = prompt.unwrap_or_default();
    rsx! {
        section { class: "mcp-agent-card", aria_label: "一句话接入 MCP",
            div { class: "mcp-agent-heading",
                span { class: "mcp-agent-emblem", aria_hidden: "true",
                    McpCopyIcon { copied: false, agent: true }
                }
                div { class: "min-w-0",
                    div { class: "flex flex-wrap items-center gap-2.5",
                        h3 { class: "text-base sm:text-lg font-bold tracking-tight", "一句话，让 Agent 帮你接入" }
                        span { class: "mcp-agent-badge", "自动识别客户端" }
                    }
                    p { class: "text-sm text-paper-secondary mt-1.5 leading-relaxed",
                        "复制指令，粘贴到 Agent 对话，它会帮你添加配置并检查连接。"
                    }
                }
            }
            div { class: "mcp-agent-prompt",
                span { class: "mcp-agent-quote", aria_hidden: "true", "“" }
                p { "请为当前客户端接入 Yggdrasil MCP，保留已有配置，并验证连接与工具列表。" }
                span { class: "mcp-agent-prompt-caption", "复制时自动附上服务地址和当前令牌" }
            }
            div { class: "mcp-agent-footer",
                ol { class: "mcp-agent-steps", aria_label: "接入步骤",
                    for (number, label) in [("1", "复制指令"), ("2", "粘贴给 Agent"), ("3", "完成接入")] {
                        li {
                            span { "{number}" }
                            "{label}"
                        }
                    }
                }
                McpCopyButton {
                    text: prompt,
                    agent: true,
                    prominent: true,
                    disabled,
                    loading,
                }
            }
            if disabled && !loading {
                p { class: "text-xs text-paper-secondary", "先选择或填入上方令牌，即可复制完整接入指令。" }
            }
        }
    }
}

/// 所有配置复制共用真实的成功/失败反馈，避免剪贴板拒绝时提示成功。
#[cfg(target_arch = "wasm32")]
#[component]
pub(super) fn McpCopyButton(
    text: String,
    #[props(default = "复制配置")] label: &'static str,
    #[props(default)] agent: bool,
    #[props(default)] prominent: bool,
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
) -> Element {
    let mut copied = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| false);
    let button_label = if copied() {
        if agent {
            "已复制指令"
        } else {
            "已复制"
        }
    } else if busy() || loading {
        "准备中…"
    } else if agent {
        "复制给 Agent"
    } else {
        label
    };
    rsx! {
        div { class: "mcp-copy-control",
            button {
                r#type: "button",
                class: if prominent { "mcp-copy-button mcp-copy-button-primary" } else { "mcp-copy-button" },
                "data-copied": copied().to_string(),
                disabled: disabled || busy() || copied(),
                onclick: move |_| {
                    if disabled || busy() || copied() { return; }
                    let text = text.clone();
                    busy.set(true);
                    error.set(false);
                    spawn(async move {
                        match copy_clipboard_wasm(&text).await {
                            Ok(()) => copied.set(true),
                            Err(()) => error.set(true),
                        }
                        busy.set(false);
                        if copied() {
                            crate::utils::time::sleep_ms(2000).await;
                            copied.set(false);
                        }
                    });
                },
                McpCopyIcon { copied: copied(), agent }
                span { "{button_label}" }
                if prominent && !copied() {
                    span { class: "mcp-copy-arrow", aria_hidden: "true", "↗" }
                }
            }
            span { class: "sr-only", role: "status", aria_live: "polite",
                if copied() { if agent { "指令已复制，请粘贴给 Agent。" } else { "配置已复制。" } }
            }
            if error() {
                p { role: "alert", class: "mcp-copy-error", "复制失败，请允许浏览器访问剪贴板后重试。" }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
fn McpCopyIcon(copied: bool, agent: bool) -> Element {
    rsx! {
        svg {
            class: "mcp-copy-icon", xmlns: "http://www.w3.org/2000/svg", view_box: "0 0 24 24",
            fill: "none", stroke: "currentColor", stroke_width: "1.7", stroke_linecap: "round",
            stroke_linejoin: "round", "aria-hidden": "true",
            if copied {
                path { class: "mcp-copy-check", d: "m5 12 4 4L19 6" }
            } else if agent {
                path { d: "m12 3 2.6 6.4L21 12l-6.4 2.6L12 21l-2.6-6.4L3 12l6.4-2.6L12 3Z" }
                path { d: "M20 2v4m-2-2h4" }
            } else {
                rect { x: "8", y: "8", width: "12", height: "12", rx: "3" }
                path { d: "M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" }
            }
        }
    }
}

/// 单个配置片段卡片（标题 + 代码块 + 配置/Agent 指令复制按钮）。
#[cfg(target_arch = "wasm32")]
#[component]
fn ConfigSnippet(snippet: McpConfigSnippet) -> Element {
    let agent_prompt = snippet.agent_prompt();
    rsx! {
        div { class: "bg-[var(--color-paper-theme)]/70 rounded-2xl p-5 border border-[var(--color-paper-border)]/60 flex flex-col gap-3 shadow-2xs",
            div { class: "flex flex-col sm:flex-row sm:items-start justify-between gap-3",
                span { class: "text-sm font-semibold text-[var(--color-paper-primary)]",
                    "{snippet.title}"
                }
                div { class: "flex flex-wrap items-start gap-2 shrink-0",
                    McpCopyButton { text: snippet.content.clone() }
                    // 通用 entry / CLI 由上方自动接入入口处理，避免误判为配置文件。
                    if !snippet.title.starts_with("通用") && snippet.title != "Claude Code CLI" {
                        McpCopyButton { text: agent_prompt, agent: true }
                    }
                }
            }
            div { class: "md-content",
                pre { class: "bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] rounded-xl p-3.5 text-xs overflow-x-auto font-mono border border-[var(--color-paper-border)]/50",
                    code { dangerous_inner_html: "{snippet.content_html}" }
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn copy_clipboard_wasm(text: &str) -> Result<(), ()> {
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or(())?;
    if !window.is_secure_context() {
        return Err(());
    }
    // web-sys 的 Navigator::clipboard() 直接返回 Clipboard（非 Option）。
    let clipboard = window.navigator().clipboard();
    JsFuture::from(clipboard.write_text(text))
        .await
        .map(|_| ())
        .map_err(|_| ())
}
