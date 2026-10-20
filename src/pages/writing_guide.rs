//! 写作技能的公开阅读页：三步接入（安装技能、连接站点、开始写作）、正文功能卡片与
//! 技能全文。复用文章渲染器及 MCP 配置生成器。
use dioxus::prelude::*;

use crate::api::writing_guide::{get_writing_guide, WritingGuideData, SKILL_URL};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::writing_guide_skeleton::WritingGuideSkeleton;
use crate::components::ui::{FilterTabs, Icon, IconName};
use crate::router::Route;

/// 客户端：名称、配置文件位置与官方文档。顺序与 `WritingGuideData::configs` 一致。
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

/// 正文能用的功能：标题、说明与最小写法。
const FEATURES: [(&str, &str, &str); 4] = [
    (
        "可运行代码",
        "围栏语言名后加 runnable，读者可以在页面里直接运行。",
        "```python runnable\nprint(sum([2, 4, 6]))\n```",
    ),
    (
        "Mermaid 图表",
        "流程图、时序图与关系图直接写成代码，跟随明暗主题。",
        "```mermaid\nflowchart LR\n  草稿 --> 审阅 --> 发布\n```",
    ),
    (
        "数学与化学公式",
        "行内与独立公式用 KaTeX 排版，化学式用 \\ce{}。",
        "$$E = mc^2$$\n\n$\\ce{2H2 + O2 -> 2H2O}$",
    ),
    (
        "脚注、表格与折叠",
        "补充说明放进脚注，长输出收进折叠内容。",
        "适用条件见脚注[^note]。\n\n[^note]: 输入在计算期间不变。",
    ),
];

#[component]
pub fn WritingGuide() -> Element {
    let response = use_server_future(get_writing_guide)?;
    let data = match response.read().as_ref() {
        Some(Ok(data)) => data.clone(),
        Some(Err(error)) => return Err(error.clone().into()),
        None => {
            return rsx! {
                DelayedSkeleton { WritingGuideSkeleton {} }
            };
        }
    };

    rsx! {
        document::Title { "写作指南 · Yggdrasil" }
        article { class: "writing-page",
            WritingHeader {}
            ol { class: "writing-steps page-enter", style: "--enter-index: 1",
                InstallStep { prompt: data.install_prompt.clone() }
                ConnectStep { data: data.clone() }
                li { class: "writing-step",
                    div { class: "writing-step-rail",
                        span { class: "writing-step-number is-last tabular-nums", "3" }
                    }
                    div { class: "writing-step-body",
                        h2 { class: "writing-step-title", "开始写作" }
                        p { class: "writing-step-description", "AI 只会保存草稿，发布仍由你在后台确认。可以这样开头：" }
                        blockquote { class: "writing-example",
                            "让 Agent 使用 yggdrasil-writing，写一篇带可运行示例的文章，先保存为草稿。"
                        }
                    }
                }
            }
            section { class: "writing-features", aria_labelledby: "writing-features-title",
                h2 { id: "writing-features-title", class: "writing-section-title", "正文能用的功能" }
                div { class: "writing-feature-grid",
                    for (title, description, code) in FEATURES {
                        div { key: "{title}", class: "writing-feature",
                            p { class: "writing-feature-title", "{title}" }
                            p { class: "writing-feature-description", "{description}" }
                            pre { class: "writing-feature-code", code { "{code}" } }
                        }
                    }
                }
            }
            SkillDocument { data }
        }
    }
}

/// 页头：返回关于、标题与一句说明。骨架屏复用。
#[component]
pub fn WritingHeader() -> Element {
    rsx! {
        Link { class: "writing-back", to: Route::About {},
            Icon { name: IconName::ArrowLeft, size: 14 }
            "关于"
        }
        header { class: "writing-header page-enter",
            h1 { class: "writing-title", "写作指南" }
            p { class: "writing-lead",
                "把这里的写作方式交给你的 AI：一份技能文件告诉它怎么排版，一条 MCP 连接让它能直接保存草稿。"
            }
        }
    }
}

/// 第 1 步：一句话让 Agent 安装技能到用户目录。
#[component]
fn InstallStep(prompt: String) -> Element {
    rsx! {
        li { class: "writing-step",
            div { class: "writing-step-rail",
                span { class: "writing-step-number tabular-nums", "1" }
                span { class: "writing-step-line", aria_hidden: "true" }
            }
            div { class: "writing-step-body",
                h2 { class: "writing-step-title", "安装写作技能" }
                p { class: "writing-step-description",
                    "把这句话发给你的 Agent。它会把 SKILL.md 安装到用户目录，之后在任何项目里都能用；Codex、Oh-My-Pi 与 OpenCode 都支持这个目录。"
                }
                div { class: "writing-install",
                    p { class: "writing-install-prompt", "{prompt}" }
                    div { class: "writing-install-actions",
                        CopyButton { text: prompt.clone(), label: "复制安装指令", primary: true }
                        a { class: "btn btn-ghost btn-sm", href: SKILL_URL, download: "SKILL.md",
                            Icon { name: IconName::Download, size: 15 }
                            "下载 SKILL.md"
                        }
                        a { class: "btn btn-ghost btn-sm", href: "#skill", "先读全文" }
                    }
                }
            }
        }
    }
}

/// 第 2 步：选择客户端，复制合并到配置文件的 MCP 配置。
#[component]
fn ConnectStep(data: WritingGuideData) -> Element {
    let mut selected = use_signal(|| 0usize);
    let current = selected();
    let (client, config_path, docs_url) = CLIENTS[current];
    let tabs: Vec<(&'static str, &'static str)> = CLIENTS
        .iter()
        .enumerate()
        .map(|(i, (name, _, _))| (["0", "1", "2"][i], *name))
        .collect();
    rsx! {
        li { class: "writing-step",
            div { class: "writing-step-rail",
                span { class: "writing-step-number tabular-nums", "2" }
                span { class: "writing-step-line", aria_hidden: "true" }
            }
            div { class: "writing-step-body writing-connect",
                div { class: "writing-step-head",
                    h2 { class: "writing-step-title", "连接站点" }
                    Link { class: "writing-link", to: Route::Mcp {},
                        "管理 MCP 令牌"
                        Icon { name: IconName::ArrowUpRight, size: 14 }
                    }
                }
                p { class: "writing-step-description",
                    "选择客户端，把配置合并到它的配置文件，再把令牌占位符换成后台签发的令牌。"
                }
                figure { class: "writing-config",
                    div { class: "writing-config-bar",
                        FilterTabs {
                            items: tabs,
                            active_value: current.to_string(),
                            on_change: move |value: String| selected.set(value.parse().unwrap_or(0)),
                            aria_label: "Agent 客户端",
                            class: "writing-config-tabs",
                        }
                        div { class: "writing-config-meta",
                            code { "{config_path}" }
                            CopyButton { key: "{client}", text: data.configs[current].clone(), label: "复制配置" }
                        }
                    }
                    pre { class: "writing-config-code", aria_label: "{client} MCP 配置", code { "{data.configs[current]}" } }
                    figcaption { class: "writing-config-foot",
                        span { "将 YOUR_YGGDRASIL_TOKEN 替换为站点签发的令牌；已有配置请合并，保留其他服务。" }
                        a { href: docs_url, target: "_blank", rel: "noopener noreferrer",
                            "{client} 文档"
                            Icon { name: IconName::ArrowUpRight, size: 13 }
                        }
                    }
                }
                p { class: "writing-permissions",
                    "技能可以公开阅读；通过 MCP 保存文章需要 write 令牌，执行代码验证需要 admin 权限。"
                }
            }
        }
    }
}

/// 技能全文：左侧目录，右侧正文默认收起，展开后读全文。
#[component]
fn SkillDocument(data: WritingGuideData) -> Element {
    let mut expanded = use_signal(|| false);
    rsx! {
        section { id: "skill", class: "writing-skill", aria_labelledby: "skill-title",
            div { class: "writing-skill-head",
                h2 { id: "skill-title", class: "writing-section-title", "技能全文" }
                div { class: "writing-actions",
                    span { class: "writing-file-label", "SKILL.md" }
                    CopyButton { text: data.source.clone(), label: "复制全文" }
                }
            }
            div { class: "writing-skill-layout",
                // 目录指向收起区里的小节：按下链接时先展开全文。锚点点击被全局处理在
                // 捕获阶段拦下，收不到 click，所以听 pointerdown 与 Enter。
                nav {
                    class: "writing-toc",
                    aria_label: "技能目录",
                    onpointerdown: move |_| expanded.set(true),
                    onkeydown: move |event| {
                        if event.key() == Key::Enter {
                            expanded.set(true);
                        }
                    },
                    dangerous_inner_html: data.toc_html,
                }
                div { class: "writing-skill-body",
                    div {
                        id: "skill-content",
                        class: "md-content post-content writing-content",
                        "data-expanded": "{expanded}",
                        dangerous_inner_html: data.html,
                    }
                    if !expanded() {
                        div { class: "writing-expand",
                            button {
                                r#type: "button",
                                class: "btn btn-secondary",
                                aria_controls: "skill-content",
                                aria_expanded: "false",
                                onclick: move |_| expanded.set(true),
                                "展开全文"
                            }
                        }
                    }
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
    let icon = if result() == Some(true) {
        IconName::Check
    } else {
        IconName::Copy
    };
    rsx! {
        span { class: "writing-copy-wrap",
            button {
                r#type: "button",
                class: if primary { "btn btn-primary btn-sm" } else { "btn btn-ghost btn-sm" },
                onclick: move |_| {
                    let text = text.clone();
                    async move {
                        #[cfg(target_arch = "wasm32")]
                        result.set(Some(crate::utils::js::copy_text(&text).await));
                        #[cfg(not(target_arch = "wasm32"))]
                        let _ = text;
                    }
                },
                Icon { name: icon, size: 15 }
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
