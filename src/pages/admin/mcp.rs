//! 管理后台「MCP 服务器」页面。
//!
//! 管理员在此签发/查看/撤销为 AI 客户端（Claude Code / Cursor / Cline）准备的
//! bearer 令牌，并复制对应的客户端配置片段。功能分三块：
//! - 令牌列表：名称 / 作用域 / 创建时间 / 过期 / 最近使用 / 状态 + 撤销 / 重查按钮。
//! - 新建令牌表单：名称 + 作用域下拉 + 有效期下拉；提交后一次性弹窗展示明文。
//! - 客户端配置：选中令牌后展示 4 种可复制的配置片段 + CLI 一行命令。
//!
//! 仅 WASM 前端交互，数据经 Dioxus server functions（`src/api/mcp_tokens.rs`）加载。
//! 配置片段由服务端生成（`src/mcp/config.rs`，server-only 模块），经 `get_mcp_client_configs`
//! server fn 返回——`APP_BASE_URL` 等环境变量只在服务端可读。
//!
//! 跨子组件状态（刷新触发、一次性明文弹窗、配置令牌）经一个共享 context 传递，
//! 避免列表与表单组件各自维护互相不可见的信号。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::mcp_tokens::{
    create_mcp_token, get_mcp_client_configs, list_mcp_tokens, reveal_mcp_token, revoke_mcp_token,
    McpClientConfigs, McpConfigSnippet, TokenLifetime,
};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::{FormInput, FormSelect};
#[cfg(target_arch = "wasm32")]
use crate::components::skeletons::atoms::SkeletonBox;
#[cfg(target_arch = "wasm32")]
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{BTN_PRIMARY, BTN_PRIMARY_SM};
#[cfg(target_arch = "wasm32")]
use crate::models::mcp_token::{McpTokenSummary, TokenScope};

/// 作用域可选项（下拉菜单）。
#[cfg(target_arch = "wasm32")]
const SCOPE_OPTIONS: &[(TokenScope, &str)] = &[
    (TokenScope::Read, "read — 仅查询已发布文章"),
    (TokenScope::Write, "write — read + 文章/评论/标签/媒体 CRUD"),
    (TokenScope::Admin, "admin — write + 站点设置 / 代码运行器"),
];

/// 有效期可选项（下拉菜单）。
#[cfg(target_arch = "wasm32")]
const LIFETIME_OPTIONS: &[(TokenLifetime, &str)] = &[
    (TokenLifetime::Days1, "1 天"),
    (TokenLifetime::Days7, "7 天"),
    (TokenLifetime::Days30, "30 天"),
    (TokenLifetime::Days90, "90 天"),
    (TokenLifetime::Never, "永不过期"),
];

/// 配置骨架屏的尺寸变化：(标题宽度 px, 代码块高度 px)。
///
/// 按 7 个真实配置片段的长短差异给出不同占位尺寸，让骨架屏更像即将出现的真实内容
/// 而非千篇一律的等高块。
#[cfg(target_arch = "wasm32")]
const CONFIG_SKELETON_SHAPES: &[(&str, &str)] = &[
    ("width: 320px;", "height: 152px;"), // Oh-My-Pi JSON
    ("width: 280px;", "height: 168px;"), // OpenCode JSON
    ("width: 260px;", "height: 168px;"), // Claude Code JSON
    ("width: 200px;", "height: 136px;"), // Cursor JSON
    ("width: 220px;", "height: 168px;"), // Cline JSON
    ("width: 160px;", "height: 104px;"), // 通用 JSON
    ("width: 140px;", "height: 56px;"),  // CLI 一行命令
];
#[cfg(target_arch = "wasm32")]
const BTN_COPIED_SM: &str =
    "inline-flex items-center justify-center px-4 py-1.5 text-sm font-medium \
     text-green-700 dark:text-green-300 bg-green-100 dark:bg-green-900/30 \
     rounded-full transition-all cursor-default";

/// 跨子组件共享的页面状态：刷新代际、一次性明文弹窗、配置用令牌、操作提示。
///
/// `PartialEq` 由组件宏生成的 Props 结构体要求（`TokenRow` 以此为 prop）。
/// `Signal<T>` 实现了 `PartialEq`（比较当前值），故派生可行。
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, PartialEq)]
struct McpPageState {
    /// 递增以触发令牌列表重新加载（创建/撤销后 +1）。
    reload_gen: Signal<u32>,
    /// 一次性明文弹窗：Some 时展示。
    created_plaintext: Signal<Option<String>>,
    /// 重查明文弹窗：(id, plaintext)。
    revealed: Signal<Option<(String, String)>>,
    /// 配置卡片使用的令牌明文。
    config_token: Signal<Option<String>>,
    /// 全局操作提示：(消息, 是否错误)。
    toast: Signal<Option<(String, bool)>>,
}

/// 管理后台 MCP 令牌管理页面。
#[component]
#[allow(clippy::needless_pass_by_value)]
pub fn Mcp() -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let state = McpPageState {
            reload_gen: use_signal(|| 0),
            created_plaintext: use_signal(|| None),
            revealed: use_signal(|| None),
            config_token: use_signal(|| None),
            toast: use_signal(|| None),
        };
        use_context_provider(|| state);

        rsx! {
            // 将弹窗放在进场动画容器之外，避免 transform 祖先改变 fixed 的定位参考系。
            div { class: "w-full",
                div { class: "animate-page-enter w-full max-w-7xl mx-auto space-y-8",
                    div {
                        class: "animate-row-enter",
                        style: "animation-delay: 0ms",
                        PageHeader {}
                    }
                    div {
                        class: "animate-row-enter",
                        style: "animation-delay: 60ms",
                        TokenList {}
                    }
                    // FormSelect 面板为绝对定位；提高本区块层级，避免被后续 ConfigCard 覆盖。
                    div {
                        class: "animate-row-enter relative z-10",
                        style: "animation-delay: 120ms",
                        CreateTokenCard {}
                    }
                    div {
                        class: "animate-row-enter",
                        style: "animation-delay: 180ms",
                        ConfigCard {}
                    }
                }

                // 明文弹窗不放在 TokenList 的动画树内，确保 fixed 相对整个视口居中。
                if let Some(plaintext) = (state.created_plaintext)() {
                    PlaintextModal {
                        title: "令牌已创建（请立即复制，可稍后重新查看）".to_string(),
                        plaintext: plaintext.clone(),
                        on_use_config: {
                            let mut ct = state.config_token;
                            let mut cp = state.created_plaintext;
                            let mut toast = state.toast;
                            move |_| {
                                ct.set(Some(plaintext.clone()));
                                cp.set(None);
                                toast.set(Some(("已选为配置令牌".to_string(), false)));
                            }
                        },
                        on_close: {
                            let mut cp = state.created_plaintext;
                            move |_| cp.set(None)
                        },
                    }
                }
                if let Some((_, plaintext)) = (state.revealed)() {
                    PlaintextModal {
                        title: "令牌明文".to_string(),
                        plaintext: plaintext.clone(),
                        on_use_config: {
                            let mut ct = state.config_token;
                            let mut rev = state.revealed;
                            let mut toast = state.toast;
                            move |_| {
                                ct.set(Some(plaintext.clone()));
                                rev.set(None);
                                toast.set(Some(("已选为配置令牌".to_string(), false)));
                            }
                        },
                        on_close: {
                            let mut rev = state.revealed;
                            move |_| rev.set(None)
                        },
                    }
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // server 构建下页面无前端交互；路由实际只在 WASM 渲染。
        rsx! {
            p { class: "text-paper-secondary", "此页面仅在浏览器中可用。" }
            // server 构建下页面无前端交互；路由实际只在 WASM 渲染。
        }
    }
}

#[component]
fn PageHeader() -> Element {
    rsx! {
        div { class: "flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-[var(--color-paper-border)]/70",
            div {
                h1 { class: "text-3xl sm:text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                    "MCP 服务器"
                }
                p { class: "text-sm text-[var(--color-paper-secondary)] mt-1.5",
                    "为 AI 客户端（Claude Code / Cursor / Cline / Oh-My-Pi）签发访问令牌并获取接入配置"
                }
            }
            div { class: "flex items-center gap-2.5",
                div { class: "inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-full text-xs font-mono bg-[var(--color-paper-entry)] text-[var(--color-paper-secondary)] border border-[var(--color-paper-border)]/70 shadow-2xs",
                    span { class: "w-1.5 h-1.5 rounded-full bg-[var(--color-paper-accent)]" }
                    span { "Endpoint: /mcp" }
                }
            }
        }
    }
}

/// 全局操作提示条（读取共享 context 的 toast）。
#[cfg(target_arch = "wasm32")]
#[component]
fn Toast() -> Element {
    let state: McpPageState = use_context();
    let toast = state.toast;
    if let Some((msg, is_err)) = toast() {
        let cls = if is_err {
            "bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300"
        } else {
            "bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300"
        };
        rsx! {
            div { class: "text-sm rounded-lg px-3 py-2 {cls}", "{msg}" }
        }
    } else {
        rsx! {}
    }
}

/// 令牌列表卡片 + 撤销/重查操作。
#[cfg(target_arch = "wasm32")]
#[component]
fn TokenList() -> Element {
    let mut state: McpPageState = use_context();
    let mut tokens = use_signal(Vec::<McpTokenSummary>::new);
    let mut loaded_gen = use_signal(|| None::<u32>);
    let mut loading = use_signal(|| true);

    let reload_gen = state.reload_gen;
    let mut toast = state.toast;

    // reload_gen 变化时重新加载。
    use_effect(move || {
        let g = reload_gen();
        if loaded_gen() != Some(g) {
            loaded_gen.set(Some(g));
            spawn(async move {
                match list_mcp_tokens().await {
                    Ok(list) => tokens.set(list),
                    Err(e) => toast.set(Some((format!("加载失败：{e}"), true))),
                }
                loading.set(false);
            });
        }
    });

    rsx! {
        div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 p-6 sm:p-8 flex flex-col gap-6",
            div { class: "flex items-center justify-between border-b border-[var(--color-paper-border)]/60 pb-4",
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
                        path { d: "M21 2l-2 2m-1.5 1.5L14 9l-1.5-1.5L11 9l-1.5-1.5L8 9 3 14a5 5 0 0 0 7 7l5-5 1.5 1.5L18 15l-1.5-1.5L18 12l1.5-1.5 1.5 1.5z" }
                    }
                    h2 { class: "text-lg sm:text-xl font-bold text-[var(--color-paper-primary)]",
                        "已签发令牌 ({tokens().len()})"
                    }
                }
                button {
                    class: "inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-medium text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                    onclick: move |_| {
                        let g = reload_gen();
                        state.reload_gen.set(g + 1);
                    },
                    svg {
                        class: "w-3.5 h-3.5",
                        xmlns: "http://www.w3.org/2000/svg",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M21.5 2v6h-6M21.34 15.57a10 10 0 1 1-.57-8.38l5.67-5.67" }
                    }
                    "刷新列表"
                }
            }

            // 表格区
            if loading() && tokens().is_empty() {
                DelayedSkeleton {
                    div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 overflow-hidden",
                        table { class: "w-full text-sm",
                            thead {
                                tr { class: "bg-[var(--color-paper-entry)]/80 border-b border-[var(--color-paper-border)]/70",
                                    th { class: "px-5 py-3.5",
                                        SkeletonBox { class: "h-3 w-12" }
                                    }
                                    th { class: "px-4 py-3.5",
                                        SkeletonBox { class: "h-3 w-10" }
                                    }
                                    th { class: "px-4 py-3.5",
                                        SkeletonBox { class: "h-3 w-14" }
                                    }
                                    th { class: "px-4 py-3.5",
                                        SkeletonBox { class: "h-3 w-14" }
                                    }
                                    th { class: "px-4 py-3.5",
                                        SkeletonBox { class: "h-3 w-16" }
                                    }
                                    th { class: "px-4 py-3.5",
                                        SkeletonBox { class: "h-3 w-10" }
                                    }
                                    th { class: "px-5 py-3.5",
                                        SkeletonBox { class: "h-3 w-16 ml-auto" }
                                    }
                                }
                            }
                            tbody {
                                for _ in 0..3 {
                                    tr { class: "border-b border-[var(--color-paper-border)]/60 last:border-b-0",
                                        td { class: "px-5 py-3.5",
                                            SkeletonBox { class: "h-4 w-28" }
                                        }
                                        td { class: "px-4 py-3.5",
                                            SkeletonBox { class: "h-4 w-12" }
                                        }
                                        td { class: "px-4 py-3.5",
                                            SkeletonBox { class: "h-4 w-16" }
                                        }
                                        td { class: "px-4 py-3.5",
                                            SkeletonBox { class: "h-4 w-16" }
                                        }
                                        td { class: "px-4 py-3.5",
                                            SkeletonBox { class: "h-4 w-20" }
                                        }
                                        td { class: "px-4 py-3.5",
                                            SkeletonBox { class: "h-5 w-10 rounded-full" }
                                        }
                                        td { class: "px-5 py-3.5",
                                            SkeletonBox { class: "h-6 w-36 ml-auto rounded" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else if tokens().is_empty() {
                p { class: "text-[var(--color-paper-secondary)] text-sm py-8 text-center",
                    "暂无已签发的 MCP 访问令牌。在下方新建一个。"
                }
            } else {
                div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 overflow-hidden",
                    div { class: "overflow-x-auto",
                        table { class: "w-full text-sm",
                            thead {
                                tr { class: "bg-[var(--color-paper-entry)]/80 border-b border-[var(--color-paper-border)]/70 text-left text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)] select-none",
                                    th { class: "px-5 py-3.5 font-semibold", "令牌名称" }
                                    th { class: "px-4 py-3.5 font-semibold whitespace-nowrap", "权限作用域" }
                                    th { class: "px-4 py-3.5 font-semibold whitespace-nowrap",
                                        "创建日期"
                                    }
                                    th { class: "px-4 py-3.5 font-semibold whitespace-nowrap",
                                        "有效期至"
                                    }
                                    th { class: "px-4 py-3.5 font-semibold whitespace-nowrap",
                                        "最近活跃"
                                    }
                                    th { class: "px-4 py-3.5 font-semibold whitespace-nowrap text-center",
                                        "状态"
                                    }
                                    th { class: "px-5 py-3.5 font-semibold text-right whitespace-nowrap", "操作" }
                                }
                            }
                            tbody {
                                for (i, t) in tokens().iter().enumerate() {
                                    TokenRow {
                                        key: "{t.id}",
                                        token: t.clone(),
                                        state,
                                        stagger_index: i as u32,
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

/// 单行令牌：展示元数据 + 重查/用于配置/撤销按钮。
#[cfg(target_arch = "wasm32")]
#[component]
fn TokenRow(token: McpTokenSummary, state: McpPageState, stagger_index: u32) -> Element {
    let is_revoked = token.revoked_at.is_some();
    let is_expired = token
        .expires_at
        .map(|e| e < chrono::Utc::now())
        .unwrap_or(false);
    let active = !is_revoked && !is_expired;
    let created = token.created_at.format("%Y-%m-%d").to_string();
    let expires = token
        .expires_at
        .map(|e| e.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "永不过期".to_string());
    let last_used = token
        .last_used_at
        .map(|e| e.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "—".to_string());
    let id_reveal = token.id.clone();
    let id_config = token.id.clone();
    let id_revoke = token.id.clone();

    // 提取信号到局部，避免 `state.field()` 被解析为方法调用。
    let mut revealed = state.revealed;
    let mut toast = state.toast;
    let mut config_token = state.config_token;
    let reload_gen = state.reload_gen;

    rsx! {
        tr {
            class: "animate-row-enter border-b border-[var(--color-paper-border)]/60 last:border-b-0 hover:bg-[var(--color-paper-accent-soft)]/30 transition-colors duration-150 group",
            style: "animation-delay: {stagger_index * 40}ms",
            // 令牌名称
            td { class: "px-5 py-3.5 font-semibold text-[var(--color-paper-primary)]",
                "{token.name}"
            }
            // 作用域胶囊
            td { class: "px-4 py-3.5 whitespace-nowrap",
                span { class: "inline-flex items-center px-2 py-0.5 rounded-md text-xs font-mono font-medium bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]/60",
                    "{token.scope.as_str()}"
                }
            }
            // 创建日期
            td { class: "px-4 py-3.5 text-xs font-mono text-[var(--color-paper-secondary)] whitespace-nowrap",
                "{created}"
            }
            // 过期时间
            td { class: "px-4 py-3.5 text-xs font-mono text-[var(--color-paper-secondary)] whitespace-nowrap",
                "{expires}"
            }
            // 最近使用
            td { class: "px-4 py-3.5 text-xs font-mono text-[var(--color-paper-secondary)] whitespace-nowrap",
                "{last_used}"
            }
            // 状态胶囊
            td { class: "px-4 py-3.5 text-center whitespace-nowrap",
                if active {
                    span { class: "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20",
                        span { class: "w-1.5 h-1.5 rounded-full bg-emerald-500" }
                        "有效"
                    }
                } else if is_expired {
                    span { class: "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20",
                        span { class: "w-1.5 h-1.5 rounded-full bg-amber-500" }
                        "已过期"
                    }
                } else {
                    span { class: "inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-gray-500/10 text-gray-600 dark:text-gray-400 border border-gray-500/20",
                        span { class: "w-1.5 h-1.5 rounded-full bg-gray-400" }
                        "已撤销"
                    }
                }
            }
            // 操作列
            td { class: "px-5 py-3.5 text-right whitespace-nowrap",
                if active {
                    div { class: "flex justify-end items-center gap-1.5",
                        button {
                            class: "inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium text-emerald-600 dark:text-emerald-400 hover:bg-emerald-50 dark:hover:bg-emerald-900/20 transition-colors cursor-pointer",
                            onclick: move |_| {
                                let id = id_reveal.clone();
                                spawn(async move {
                                    match reveal_mcp_token(id.clone()).await {
                                        Ok(Some(p)) => revealed.set(Some((id, p))),
                                        Ok(None) => toast.set(Some(("无法解密该令牌".to_string(), true))),
                                        Err(e) => toast.set(Some((format!("重查失败：{e}"), true))),
                                    }
                                });
                            },
                            svg {
                                class: "w-3.5 h-3.5",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" }
                                circle { cx: "12", cy: "12", r: "3" }
                            }
                            "重新查看"
                        }
                        button {
                            class: "inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium text-[var(--color-paper-primary)] hover:bg-[var(--color-paper-theme)] transition-colors cursor-pointer",
                            onclick: move |_| {
                                let id = id_config.clone();
                                spawn(async move {
                                    match reveal_mcp_token(id).await {
                                        Ok(Some(p)) => {
                                            config_token.set(Some(p));
                                            toast.set(Some(("已选为配置令牌".to_string(), false)));
                                        }
                                        Ok(None) => toast.set(Some(("无法解密该令牌".to_string(), true))),
                                        Err(e) => toast.set(Some((format!("重查失败：{e}"), true))),
                                    }
                                });
                            },
                            svg {
                                class: "w-3.5 h-3.5 text-[var(--color-paper-accent)]",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M12 3v3m0 12v3M3 12h3m12 0h3M5.6 5.6l2.1 2.1m8.6 8.6l2.1 2.1M5.6 18.4l2.1-2.1m8.6-8.6l2.1-2.1" }
                            }
                            "用于配置"
                        }
                        button {
                            class: "inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium text-red-500 hover:text-red-700 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors cursor-pointer",
                            onclick: move |_| {
                                let id = id_revoke.clone();
                                spawn(async move {
                                    match revoke_mcp_token(id).await {
                                        Ok(()) => {
                                            toast.set(Some(("已撤销".to_string(), false)));
                                            let g = reload_gen();
                                            state.reload_gen.set(g + 1);
                                        }
                                        Err(e) => toast.set(Some((format!("撤销失败：{e}"), true))),
                                    }
                                });
                            },
                            svg {
                                class: "w-3.5 h-3.5",
                                xmlns: "http://www.w3.org/2000/svg",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                circle { cx: "12", cy: "12", r: "10" }
                                line { x1: "4.93", y1: "4.93", x2: "19.07", y2: "19.07" }
                            }
                            "撤销"
                        }
                    }
                } else {
                    span { class: "text-xs text-[var(--color-paper-tertiary)]", "—" }
                }
            }
        }
    }
}

/// 新建令牌表单卡片。成功后把明文写入共享 context 的一次性弹窗。
#[cfg(target_arch = "wasm32")]
#[component]
fn CreateTokenCard() -> Element {
    let mut state: McpPageState = use_context();
    let mut name = use_signal(String::new);
    let mut scope = use_signal(|| TokenScope::Read);
    let mut lifetime = use_signal(|| TokenLifetime::Days30);
    let mut busy = use_signal(|| false);

    let mut created_plaintext = state.created_plaintext;
    let reload_gen = state.reload_gen;
    let mut toast = state.toast;

    rsx! {
        div { class: "bg-[var(--color-paper-entry)]/40 rounded-2xl shadow-xs border border-[var(--color-paper-border)]/70 p-6 sm:p-8 flex flex-col gap-6",
            div { class: "flex items-center gap-2.5 border-b border-[var(--color-paper-border)]/60 pb-4",
                svg {
                    class: "w-5 h-5 text-[var(--color-paper-accent)]",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    line { x1: "12", y1: "5", x2: "12", y2: "19" }
                    line { x1: "5", y1: "12", x2: "19", y2: "12" }
                }
                h2 { class: "text-lg sm:text-xl font-bold text-[var(--color-paper-primary)]",
                    "新建访问令牌"
                }
            }

            div { class: "grid grid-cols-1 md:grid-cols-3 gap-5",
                // 名称
                div { class: "flex flex-col gap-2",
                    label { class: "text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)]",
                        "令牌名称 *"
                    }
                    FormInput {
                        r#type: "text",
                        placeholder: "如 claude-code-macbook",
                        value: name(),
                        oninput: move |v: String| name.set(v),
                    }
                }
                // 作用域
                div { class: "flex flex-col gap-2",
                    label { class: "text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)]",
                        "权限作用域"
                    }
                    FormSelect {
                        value: scope(),
                        options: SCOPE_OPTIONS.to_vec(),
                        onchange: move |s| scope.set(s),
                    }
                }
                // 有效期
                div { class: "flex flex-col gap-2",
                    label { class: "text-xs font-semibold uppercase tracking-wider text-[var(--color-paper-secondary)]",
                        "有效期"
                    }
                    FormSelect {
                        value: lifetime(),
                        options: LIFETIME_OPTIONS.to_vec(),
                        onchange: move |l| lifetime.set(l),
                    }
                }
            }

            div { class: "pt-1",
                button {
                    class: "{BTN_PRIMARY} inline-flex items-center gap-1.5",
                    disabled: "{busy() || name().trim().is_empty()}",
                    onclick: move |_| {
                        if busy() {
                            return;
                        }
                        let n = name().trim().to_string();
                        if n.is_empty() {
                            return;
                        }
                        let sc = scope();
                        let lt = lifetime();
                        busy.set(true);
                        spawn(async move {
                            match create_mcp_token(n, sc, lt).await {
                                Ok(resp) => {
                                    created_plaintext.set(Some(resp.plaintext));
                                    name.set(String::new());
                                    let g = reload_gen();
                                    state.reload_gen.set(g + 1);
                                }
                                Err(e) => {
                                    toast.set(Some((format!("创建失败：{e}"), true)));
                                }
                            }
                            busy.set(false);
                        });
                    },
                    if busy() {
                        "创建中…"
                    } else {
                        "创建访问令牌"
                    }
                }
            }
        }
    }
}

/// 客户端配置卡片：展示 4 种配置片段（需先在令牌列表点「用于配置」或手动粘贴）。
#[cfg(target_arch = "wasm32")]
#[component]
fn ConfigCard() -> Element {
    let state: McpPageState = use_context();
    let mut manual_token = use_signal(String::new);
    let mut configs = use_signal(|| None::<McpClientConfigs>);
    let mut loading = use_signal(|| false);

    let config_token = state.config_token;
    let mut toast = state.toast;

    // 当 config_token 变化时，请求服务端生成配置。
    use_effect(move || {
        let token = config_token();
        let Some(t) = token else {
            return;
        };
        loading.set(true);
        manual_token.set(t.clone());
        spawn(async move {
            match get_mcp_client_configs(t).await {
                Ok(c) => configs.set(Some(c)),
                Err(e) => toast.set(Some((format!("配置生成失败：{e}"), true))),
            }
            loading.set(false);
        });
    });

    // 手动输入时也触发生成。
    let mut on_manual_input = move |val: String| {
        manual_token.set(val.clone());
        if val.trim().is_empty() {
            configs.set(None);
            return;
        }
        if loading() {
            return;
        }
        loading.set(true);
        spawn(async move {
            match get_mcp_client_configs(val).await {
                Ok(c) => configs.set(Some(c)),
                Err(e) => toast.set(Some((format!("配置生成失败：{e}"), true))),
            }
            loading.set(false);
        });
    };

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
                        oninput: move |v: String| on_manual_input(v),
                    }
                }
            }

            if loading() {
                div { class: "flex flex-col gap-4",
                    for i in 0..7 {
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

/// 单个配置片段卡片（标题 + 代码块 + 复制按钮）。
#[cfg(target_arch = "wasm32")]
#[component]
fn ConfigSnippet(snippet: McpConfigSnippet) -> Element {
    let state: McpPageState = use_context();
    let mut toast = state.toast;
    let mut copied = use_signal(|| false);
    let title = snippet.title.clone();
    let copied_now = copied();
    let btn_class = if copied_now {
        BTN_COPIED_SM
    } else {
        BTN_PRIMARY_SM
    };
    let btn_label = if copied_now { "已复制" } else { "复制" };
    rsx! {
        div { class: "bg-[var(--color-paper-theme)]/70 rounded-2xl p-5 border border-[var(--color-paper-border)]/60 flex flex-col gap-3 shadow-2xs",
            div { class: "flex items-center justify-between",
                span { class: "text-sm font-semibold text-[var(--color-paper-primary)]",
                    "{snippet.title}"
                }
                button {
                    class: "{btn_class}",
                    onclick: move |_| {
                        let cc = snippet.content.clone();
                        let tt = title.clone();
                        copied.set(true);
                        spawn(async move {
                            copy_clipboard_wasm(&cc).await;
                            toast.set(Some((format!("已复制：{tt}"), false)));
                            crate::utils::time::sleep_ms(1500).await;
                            copied.set(false);
                        });
                    },
                    "{btn_label}"
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

/// 明文展示弹窗。
#[cfg(target_arch = "wasm32")]
#[component]
fn PlaintextModal(
    title: String,
    plaintext: String,
    on_use_config: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let mut copied = use_signal(|| false);
    let copied_now = copied();
    let btn_class = if copied_now {
        BTN_COPIED_SM
    } else {
        BTN_PRIMARY_SM
    };
    let btn_label = if copied_now { "已复制" } else { "复制" };
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4 animate-modal-overlay-enter",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] rounded-3xl p-7 max-w-xl w-full flex flex-col gap-5 border border-[var(--color-paper-border)] shadow-2xl animate-modal-panel-enter",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between border-b border-[var(--color-paper-border)]/60 pb-3",
                    h3 { class: "text-base sm:text-lg font-bold text-[var(--color-paper-primary)]",
                        "{title}"
                    }
                    button {
                        class: "text-[var(--color-paper-tertiary)] hover:text-[var(--color-paper-primary)] p-1 rounded-lg hover:bg-[var(--color-paper-entry)] transition-colors cursor-pointer",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }
                pre { class: "bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]/60 rounded-2xl p-4 text-xs font-mono break-all select-all leading-relaxed",
                    code { "{plaintext}" }
                }
                div { class: "flex flex-wrap gap-2.5 justify-end pt-1",
                    button {
                        class: "{btn_class}",
                        onclick: move |_| {
                            let p = plaintext.clone();
                            copied.set(true);
                            spawn(async move {
                                copy_clipboard_wasm(&p).await;
                                crate::utils::time::sleep_ms(1500).await;
                                copied.set(false);
                            });
                        },
                        "{btn_label}"
                    }
                    button {
                        class: "{BTN_PRIMARY_SM}",
                        onclick: move |_| on_use_config.call(()),
                        "用于配置"
                    }
                    button {
                        class: "px-4 py-1.5 text-sm font-medium text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] hover:bg-[var(--color-paper-entry)] rounded-full transition-colors cursor-pointer",
                        onclick: move |_| on_close.call(()),
                        "关闭"
                    }
                }
            }
        }
    }
}

/// WASM 端剪贴板写入（navigator.clipboard.writeText）。
///
/// 失败静默忽略（非关键路径；用户可手动选中复制）。
#[cfg(target_arch = "wasm32")]
async fn copy_clipboard_wasm(text: &str) {
    use wasm_bindgen_futures::JsFuture;

    let Some(window) = web_sys::window() else {
        return;
    };
    // web-sys 的 Navigator::clipboard() 直接返回 Clipboard（非 Option）。
    let clipboard = window.navigator().clipboard();
    // write_text 返回 Promise<void>；忽略 reject（如非 HTTPS / 无焦点）。
    let _ = JsFuture::from(clipboard.write_text(text)).await;
}
