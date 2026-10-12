//! 管理后台「MCP 服务器」页面。
//!
//! 管理员在此签发/查看/编辑权限/撤销为 AI 客户端（Codex / Claude Code / Cursor / Cline 等）准备的
//! bearer 令牌，并复制对应的客户端配置片段。功能分三块：
//! - 令牌列表：名称 / 作用域 / 创建时间 / 过期 / 最近使用 / 状态 + 撤销 / 重查按钮。
//! - 新建令牌表单：名称 + 作用域下拉 + 有效期下拉；提交后一次性弹窗展示明文。
//! - 客户端配置：选中令牌后展示各客户端可复制的配置片段 + CLI 命令。
//!
//! 仅 WASM 前端交互，数据经 Dioxus server functions（`src/api/mcp_tokens.rs`）加载。
//! 配置片段由服务端生成（`src/mcp/config.rs`，server-only 模块），经 `get_mcp_client_configs`
//! server fn 返回——`APP_BASE_URL` 等环境变量只在服务端可读。
//!
//! 跨子组件状态（刷新触发、一次性明文弹窗、配置令牌）经一个共享 context 传递，
//! 避免列表与表单组件各自维护互相不可见的信号。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::models::mcp_token::McpTokenSummary;

/// 跨子组件共享的页面状态：刷新代际、一次性明文弹窗、配置用令牌、操作提示。
///
/// `PartialEq` 由组件宏生成的 Props 结构体要求（`TokenRow` 以此为 prop）。
/// `Signal<T>` 实现了 `PartialEq`（比较当前值），故派生可行。
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, PartialEq)]
struct McpPageState {
    /// 递增以触发令牌列表重新加载（创建/编辑/撤销后 +1）。
    reload_gen: Signal<u32>,
    /// 一次性明文弹窗：Some 时展示。
    created_plaintext: Signal<Option<String>>,
    /// 重查明文弹窗：(id, plaintext)。
    revealed: Signal<Option<(String, String)>>,
    /// 配置卡片使用的令牌明文。
    config_token: Signal<Option<String>>,
    /// 全局操作提示：(消息, 是否错误)。
    toast: Signal<Option<(String, bool)>>,
    editing: Signal<Option<McpTokenSummary>>,
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
            editing: use_signal(|| None),
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

                if let Some(token) = (state.editing)() {
                    EditTokenPermissionsModal { key: "{token.id}", token }
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
                    "为 AI 客户端（Codex / Claude Code / Cursor / Cline / Oh-My-Pi / OpenCode）签发访问令牌并获取接入配置"
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

#[cfg(target_arch = "wasm32")]
mod configs;
#[cfg(target_arch = "wasm32")]
mod plaintext;
#[cfg(target_arch = "wasm32")]
mod token_editor;
#[cfg(target_arch = "wasm32")]
mod tokens;
#[cfg(target_arch = "wasm32")]
use configs::ConfigCard;
#[cfg(target_arch = "wasm32")]
use plaintext::PlaintextModal;
#[cfg(target_arch = "wasm32")]
use token_editor::{CreateTokenCard, EditTokenPermissionsModal};
#[cfg(target_arch = "wasm32")]
use tokens::TokenList;
