//! 写作技能的公开阅读页；复用文章渲染器及 MCP 配置生成器。
use dioxus::prelude::*;

use crate::api::writing_guide::{get_writing_guide, SKILL_SOURCE, SKILL_URL};
use crate::router::Route;

const CLIENTS: [(&str, &str, &str); 3] = [
    (
        "Codex",
        "~/.codex/config.toml",
        "https://developers.openai.com/codex/mcp",
    ),
    (
        "OMP",
        "~/.omp/agent/mcp.json",
        "https://github.com/can1357/oh-my-pi/blob/main/docs/mcp-config.md",
    ),
    (
        "OpenCode",
        "~/.config/opencode/opencode.json",
        "https://opencode.ai/docs/mcp-servers/",
    ),
];

#[component]
pub fn WritingGuide() -> Element {
    let response = use_server_future(get_writing_guide)?;
    let mut selected = use_signal(|| 0usize);
    let data = match response.read().as_ref() {
        Some(Ok(data)) => data.clone(),
        Some(Err(error)) => return Err(error.clone().into()),
        None => return rsx! { p { role: "status", "正在载入写作指南…" } },
    };
    let current = selected();
    let (client, config_path, docs_url) = CLIENTS[current];

    rsx! {
        document::Title { "Agent 写作指南 · Yggdrasil" }
        article { class: "writing-page",
            Link { class: "writing-back", to: Route::About {}, "← 关于这里" }
            header { class: "writing-hero",
                div {
                    p { class: "writing-eyebrow", span {} "A FIELD GUIDE FOR AGENTS" }
                    h1 { "让灵感生根，" br {} em { "让 Agent 懂得书写。" } }
                    p { class: "writing-lead", "一份写作技能，连接你的 AI 与这棵世界树。" br {} "从一段能运行的代码，到一张讲清思路的图。" }
                    div { class: "writing-actions",
                        CopyButton { text: SKILL_SOURCE.to_string(), label: "复制 Skill 源码", primary: true }
                        a { class: "writing-download", href: SKILL_URL, download: "SKILL.md", "下载 SKILL.md ↓" }
                    }
                }
                div { class: "writing-emblem", aria_hidden: "true",
                    span { class: "writing-emblem-orbit" }
                    span { class: "writing-emblem-star", "✳" }
                    span { class: "writing-emblem-code", "{{ }}" }
                    span { class: "writing-emblem-caption", "WORDS → WORLDS" }
                }
            }
            div { class: "writing-capabilities", aria_label: "支持的写作功能",
                for (symbol, label) in [("⌘", "可运行代码"), ("⑂", "Mermaid 图表"), ("∑", "数学与化学公式"), ("↗", "MCP 写作") ] {
                    span { span { aria_hidden: "true", "{symbol}" } "{label}" }
                }
            }

            section { id: "connect", class: "writing-connect", aria_labelledby: "connect-title",
                div { class: "writing-section-heading",
                    div { p { class: "writing-eyebrow", "01 / GET CONNECTED" } h2 { id: "connect-title", "给你的 Agent，一份写作地图" } }
                    a { href: "#skill", class: "writing-text-link", "先读技能全文 ↓" }
                }
                section { class: "writing-install", aria_labelledby: "install-title",
                    div { class: "writing-install-heading",
                        h3 { id: "install-title", span { aria_hidden: "true", "↓" } "一句话，让 Agent 安装" }
                        span { class: "writing-install-badge", "用户全局" }
                    }
                    p { class: "writing-install-description", "复制后发送给你的 Agent，一次安装，在不同项目中使用。" }
                    p { class: "writing-install-prompt", "{data.install_prompt}" }
                    div { class: "writing-install-footer",
                        span { "安装到用户目录，跨项目可用" }
                        CopyButton { text: data.install_prompt.clone(), label: "复制安装指令", primary: true }
                    }
                }
                div { class: "writing-steps",
                    div { span { "01" } h3 { "全局安装" } p { "将上方指令发给 Agent，或手动保存到" } code { "~/.agents/skills/yggdrasil-writing/SKILL.md" } p { "Codex、OMP 与 OpenCode 均支持此用户级目录。" } }
                    div { span { "02" } h3 { "连接站点" } p { "将下方配置合并到客户端配置文件，并替换令牌占位符。" } Link { to: Route::Mcp {}, class: "writing-text-link", "管理 MCP 令牌 ↗" } }
                    div { span { "03" } h3 { "开始写作" } p { "让 Agent 使用 yggdrasil-writing，写一篇带可运行示例的文章，先保存为草稿。" } }
                }
                div { class: "writing-config",
                    div { class: "writing-config-tabs", role: "group", aria_label: "选择 Agent 客户端",
                        for (index, (name, _, _)) in CLIENTS.iter().enumerate() {
                            button {
                                r#type: "button",
                                aria_pressed: "{current == index}",
                                class: if current == index { "is-selected" } else { "" },
                                onclick: move |_| selected.set(index),
                                "{name}"
                            }
                        }
                        span { "MCP / STREAMABLE HTTP" }
                    }
                    div { class: "writing-config-meta",
                        code { "{config_path}" }
                        CopyButton { key: "{client}", text: data.configs[current].clone(), label: "复制配置" }
                    }
                    pre { class: "writing-config-code", aria_label: "{client} MCP 配置", code { "{data.configs[current]}" } }
                    div { class: "writing-config-footer",
                        p { "将 YOUR_YGGDRASIL_TOKEN 替换为站点签发的令牌。" }
                        a { href: docs_url, target: "_blank", rel: "noopener noreferrer", "{client} 官方文档 ↗" }
                    }
                }
                p { class: "writing-permissions", "技能可以公开阅读；通过 MCP 保存文章需要站点管理员签发的 write 令牌，执行代码验证需要 admin 权限。已有配置请合并，保留其他服务。" }
                details { class: "writing-sources",
                    summary { "技能安装参考" }
                    a { href: "https://developers.openai.com/codex/skills", target: "_blank", rel: "noopener noreferrer", "Codex Skills ↗" }
                    a { href: "https://github.com/can1357/oh-my-pi/blob/main/docs/skills.md", target: "_blank", rel: "noopener noreferrer", "OMP Skills ↗" }
                    a { href: "https://opencode.ai/docs/skills/", target: "_blank", rel: "noopener noreferrer", "OpenCode Skills ↗" }
                }
            }

            section { id: "skill", class: "writing-document", aria_labelledby: "skill-title",
                div { class: "writing-section-heading",
                    div { p { class: "writing-eyebrow", "02 / THE SKILL" } h2 { id: "skill-title", "Yggdrasil 文章写作" } }
                    span { class: "writing-file-label", "SKILL.md" }
                }
                details { class: "writing-toc",
                    summary { "本页技能目录" span { aria_hidden: "true", "+" } }
                    nav { aria_label: "技能目录", dangerous_inner_html: data.toc_html }
                }
                div { class: "md-content writing-content", dangerous_inner_html: data.html }
                footer { class: "writing-document-footer",
                    p { "准备好了，就让下一篇文章在这里生长。" }
                    CopyButton { text: SKILL_SOURCE.to_string(), label: "复制 Skill 源码", primary: true }
                }
            }
        }
    }
}

/// 仅在剪贴板写入成功后显示成功；失败时保持原文可选中或下载。
#[component]
fn CopyButton(text: String, label: &'static str, #[props(default)] primary: bool) -> Element {
    #[allow(unused_mut)]
    let mut result = use_signal(|| None::<bool>);
    rsx! {
        span { class: "writing-copy-wrap",
            button {
                r#type: "button",
                class: if primary { "writing-copy writing-copy-primary" } else { "writing-copy" },
                onclick: move |_| {
                    let text = text.clone();
                    async move {
                        #[cfg(target_arch = "wasm32")]
                        {
                            use wasm_bindgen::JsCast;
                            let clipboard = web_sys::window().and_then(|window| {
                                js_sys::Reflect::get(window.navigator().as_ref(), &"clipboard".into())
                                    .ok()?
                                    .dyn_into::<web_sys::Clipboard>()
                                    .ok()
                            });
                            let success = if let Some(clipboard) = clipboard {
                                wasm_bindgen_futures::JsFuture::from(clipboard.write_text(&text)).await.is_ok()
                            } else { false };
                            result.set(Some(success));
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        let _ = text;
                    }
                },
                span { aria_hidden: "true", if result() == Some(true) { "✓" } else { "⧉" } }
                "{label}"
            }
            span { class: "writing-copy-status", role: "status", aria_live: "polite",
                match result() {
                    Some(true) => "已复制",
                    Some(false) => "复制失败，请下载或手动复制",
                    None => "",
                }
            }
        }
    }
}
