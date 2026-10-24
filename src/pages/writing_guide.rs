//! 写作技能的公开阅读页：三步接入（安装技能、连接站点、开始写作）、正文功能卡片与
//! 技能全文。复用文章渲染器及 MCP 配置生成器。
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::writing_guide::{get_writing_guide, WritingGuideData, SKILL_URL};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::writing_guide_skeleton::WritingGuideSkeleton;
use crate::components::ui::{Icon, IconName, SegmentedControl, SegmentedItem};
use crate::pages::not_found::{NotFound, PageLoadFailed};
use crate::routes::Route;
use crate::utils::error::is_not_found;

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

/// 正文支持的功能：标题、说明与最小写法。
const FEATURES: [(&str, &str, &str); 4] = [
    (
        "可运行代码",
        "围栏语言名后加 runnable，阅读页提供运行按钮。",
        "```python runnable\nprint(sum([2, 4, 6]))\n```",
    ),
    (
        "Mermaid 图表",
        "流程图、时序图与关系图以代码形式书写，跟随明暗主题。",
        "```mermaid\nflowchart LR\n  草稿 --> 审阅 --> 发布\n```",
    ),
    (
        "数学与化学公式",
        "行内与独立公式使用 KaTeX 排版，化学式使用 \\ce{}。",
        "$$E = mc^2$$\n\n$\\ce{2H2 + O2 -> 2H2O}$",
    ),
    (
        "脚注、表格与折叠",
        "支持脚注、表格、任务列表与折叠内容。",
        "适用条件见脚注[^note]。\n\n[^note]: 输入在计算期间不变。",
    ),
];

/// 写作指南路由，对应 `/about/writing`。
pub struct WritingGuideRoute {
    guide: Resource<Result<WritingGuideData, ServerFnError>>,
}

#[lazy_route]
impl LazyRoute for WritingGuideRoute {
    fn data() -> Self {
        Self {
            guide: Resource::new(|| (), |_| get_writing_guide()),
        }
    }

    fn view(this: Self) -> AnyView {
        let guide = this.guide;
        view! {
            <Suspense fallback=|| {
                view! {
                    <DelayedSkeleton>
                        <WritingGuideSkeleton/>
                    </DelayedSkeleton>
                }
            }>
                {move || Suspend::new(async move {
                    match guide.await {
                        Ok(data) => view! { <WritingGuide data=data/> }.into_any(),
                        Err(err) if is_not_found(&err) => view! { <NotFound/> }.into_any(),
                        Err(_) => view! { <PageLoadFailed/> }.into_any(),
                    }
                })}
            </Suspense>
        }
        .into_any()
    }
}

#[component]
fn WritingGuide(data: WritingGuideData) -> impl IntoView {
    let features = FEATURES
        .iter()
        .map(|(title, description, code)| {
            view! {
                <div class="writing-feature">
                    <p class="writing-feature-title">{*title}</p>
                    <p class="writing-feature-description">{*description}</p>
                    <pre class="writing-feature-code">
                        <code>{*code}</code>
                    </pre>
                </div>
            }
        })
        .collect_view();

    view! {
        <Title text="写作指南 · Yggdrasil"/>
        <article class="writing-page">
            <WritingHeader/>
            <ol class="writing-steps page-enter" style="--enter-index: 1">
                <InstallStep prompt=data.install_prompt.clone()/>
                <ConnectStep data=data.clone()/>
                <li class="writing-step">
                    <div class="writing-step-rail">
                        <span class="writing-step-number is-last tabular-nums">"3"</span>
                    </div>
                    <div class="writing-step-body">
                        <h2 class="writing-step-title">"开始写作"</h2>
                        <p class="writing-step-description">
                            "AI 只会保存草稿，发布仍由你在后台确认。可以这样开头："
                        </p>
                        <blockquote class="writing-example">
                            "让 Agent 使用 yggdrasil-writing，写一篇文章，先保存为草稿。"
                        </blockquote>
                    </div>
                </li>
            </ol>
            <section class="writing-features" aria-labelledby="writing-features-title">
                <h2 id="writing-features-title" class="writing-section-title">
                    "正文支持的功能"
                </h2>
                <div class="writing-feature-grid">{features}</div>
            </section>
            <SkillDocument data=data/>
        </article>
    }
}

/// 页头：返回关于、标题与一句说明。骨架屏复用。
#[component]
pub fn WritingHeader() -> impl IntoView {
    view! {
        <a class="writing-back" href=Route::About.to_string()>
            <Icon name=IconName::ArrowLeft size=14/>
            "关于"
        </a>
        <header class="writing-header page-enter">
            <h1 class="writing-title">"写作指南"</h1>
            <p class="writing-lead">
                "把这里的写作方式交给你的 AI：一份技能文件告诉它怎么排版，一条 MCP 连接让它能直接保存草稿。"
            </p>
        </header>
    }
}

/// 第 1 步：一句话让 Agent 安装技能到用户目录。
#[component]
fn InstallStep(prompt: String) -> impl IntoView {
    view! {
        <li class="writing-step">
            <div class="writing-step-rail">
                <span class="writing-step-number tabular-nums">"1"</span>
                <span class="writing-step-line" aria-hidden="true"></span>
            </div>
            <div class="writing-step-body">
                <h2 class="writing-step-title">"安装写作技能"</h2>
                <p class="writing-step-description">
                    "把这句话发给你的 Agent。它会把 SKILL.md 安装到用户目录，之后在任何项目里都能用；Codex、Oh-My-Pi 与 OpenCode 都支持这个目录。"
                </p>
                <div class="writing-install">
                    <p class="writing-install-prompt">{prompt.clone()}</p>
                    <div class="writing-install-actions">
                        <CopyButton text=prompt label="复制安装指令" primary=true/>
                        <a
                            class="btn btn-ghost btn-sm"
                            href=SKILL_URL
                            download="SKILL.md"
                            rel="external"
                        >
                            <Icon name=IconName::Download size=15/>
                            "下载 SKILL.md"
                        </a>
                        <a class="btn btn-ghost btn-sm" href="#skill">
                            "先读全文"
                        </a>
                    </div>
                </div>
            </div>
        </li>
    }
}

/// 第 2 步：选择客户端，复制合并到配置文件的 MCP 配置。
#[component]
fn ConnectStep(data: WritingGuideData) -> impl IntoView {
    let selected = RwSignal::new(0usize);
    let tabs: Vec<SegmentedItem<String>> = CLIENTS
        .iter()
        .enumerate()
        .map(|(i, (name, _, _))| SegmentedItem::new(i.to_string(), *name))
        .collect();
    let configs = data.configs;
    let copy_configs = configs.clone();
    view! {
        <li class="writing-step">
            <div class="writing-step-rail">
                <span class="writing-step-number tabular-nums">"2"</span>
                <span class="writing-step-line" aria-hidden="true"></span>
            </div>
            <div class="writing-step-body writing-connect">
                <div class="writing-step-head">
                    <h2 class="writing-step-title">"连接站点"</h2>
                    <McpTokenLink/>
                </div>
                <p class="writing-step-description">
                    "选择客户端，把配置合并到它的配置文件，再把令牌占位符换成后台签发的令牌。"
                </p>
                <figure class="writing-config">
                    <div class="writing-config-bar">
                        <SegmentedControl
                            items=tabs
                            value=Signal::derive(move || selected.get().to_string())
                            on_change=move |value: String| selected.set(value.parse().unwrap_or(0))
                            aria_label="Agent 客户端"
                            class="writing-config-tabs"
                        />
                        <div class="writing-config-meta">
                            <code>{move || CLIENTS[selected.get()].1}</code>
                            // 按客户端重建，复制状态随之清空。
                            {move || {
                                let current = selected.get();
                                view! {
                                    <CopyButton text=copy_configs[current].clone() label="复制配置"/>
                                }
                            }}
                        </div>
                    </div>
                    <pre
                        class="writing-config-code"
                        aria-label=move || format!("{} MCP 配置", CLIENTS[selected.get()].0)
                    >
                        <code>{move || configs[selected.get()].clone()}</code>
                    </pre>
                    <figcaption class="writing-config-foot">
                        <span>
                            "将 YOUR_YGGDRASIL_TOKEN 替换为站点签发的令牌；已有配置请合并，保留其他服务。"
                        </span>
                        <a
                            href=move || CLIENTS[selected.get()].2
                            target="_blank"
                            rel="noopener noreferrer"
                        >
                            {move || format!("{} 文档", CLIENTS[selected.get()].0)}
                            <Icon name=IconName::ArrowUpRight size=13/>
                        </a>
                    </figcaption>
                </figure>
                <p class="writing-permissions">
                    "技能可以公开阅读；通过 MCP 保存文章需要 write 令牌，执行代码验证需要 admin 权限。"
                </p>
            </div>
        </li>
    }
}

/// 技能全文：左侧目录，右侧正文默认收起，展开后读全文。
#[component]
fn SkillDocument(data: WritingGuideData) -> impl IntoView {
    let expanded = RwSignal::new(false);
    view! {
        <section id="skill" class="writing-skill" aria-labelledby="skill-title">
            <div class="writing-skill-head">
                <h2 id="skill-title" class="writing-section-title">
                    "技能全文"
                </h2>
                <div class="writing-actions">
                    <span class="writing-file-label">"SKILL.md"</span>
                    <CopyButton text=data.source.clone() label="复制全文"/>
                </div>
            </div>
            <div class="writing-skill-layout">
                // 目录指向收起区里的小节：按下链接时先展开全文。锚点点击会被全局处理拦下，
                // 不依赖 click，所以听 pointerdown 与 Enter。
                <nav
                    class="writing-toc"
                    aria-label="技能目录"
                    on:pointerdown=move |_| expanded.set(true)
                    on:keydown=move |event| {
                        if event.key() == "Enter" {
                            expanded.set(true);
                        }
                    }
                    inner_html=data.toc_html
                ></nav>
                <div class="writing-skill-body">
                    <div
                        id="skill-content"
                        class="md-content post-content writing-content"
                        data-expanded=move || expanded.get().to_string()
                        inner_html=data.html
                    ></div>
                    {move || {
                        (!expanded.get())
                            .then(|| {
                                view! {
                                    <div class="writing-expand">
                                        <button
                                            type="button"
                                            class="btn btn-secondary"
                                            aria-controls="skill-content"
                                            aria-expanded="false"
                                            on:click=move |_| expanded.set(true)
                                        >
                                            "展开全文"
                                        </button>
                                    </div>
                                }
                            })
                    }}
                </div>
            </div>
        </section>
    }
}

/// 仅在剪贴板写入成功后显示成功；失败时保持原文可选中或下载。
#[component]
fn CopyButton(
    #[prop(into)] text: String,
    label: &'static str,
    #[prop(optional)] primary: bool,
) -> impl IntoView {
    let result = RwSignal::new(None::<bool>);
    let on_click = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let text = text.clone();
            leptos::task::spawn_local(async move {
                let ok = crate::utils::js::copy_text(&text).await;
                let _ = result.try_set(Some(ok));
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = &text;
    };
    view! {
        <span class="writing-copy-wrap">
            <button
                type="button"
                class=if primary { "btn btn-primary btn-sm" } else { "btn btn-ghost btn-sm" }
                on:click=on_click
            >
                {move || {
                    let icon = if result.get() == Some(true) { IconName::Check } else { IconName::Copy };
                    view! { <Icon name=icon size=15/> }
                }}
                {label}
            </button>
            <span class="writing-copy-status" role="status" aria-live="polite">
                {move || match result.get() {
                    Some(true) => "已复制",
                    Some(false) => "复制失败，请下载或手动复制",
                    None => "",
                }}
            </span>
        </span>
    }
}

/// 「管理 MCP 令牌」入口：后台 MCP 页是站内路由，未登录由后台外壳跳转登录。
#[component]
fn McpTokenLink() -> impl IntoView {
    view! {
        <a href=Route::Mcp.to_string() class="writing-link">
            "管理 MCP 令牌"
            <Icon name=IconName::ArrowUpRight size=14/>
        </a>
    }
}
