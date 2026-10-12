//! Token list and row actions.
use super::McpPageState;
use crate::api::mcp_tokens::{list_mcp_tokens, reveal_mcp_token, revoke_mcp_token};
use crate::components::skeletons::{atoms::SkeletonBox, delayed_skeleton::DelayedSkeleton};
use crate::models::mcp_token::McpTokenSummary;
use dioxus::prelude::*;

/// 令牌列表卡片 + 撤销/重查操作。
#[cfg(target_arch = "wasm32")]
#[component]
pub(super) fn TokenList() -> Element {
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
    let edit_token = token.clone();

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
                if token.notes.read || token.notes.write {
                    p {class:"text-xs text-paper-accent mt-1",if token.notes.write {"笔记读写"} else {"知识库只读"}
                        if let Some(ids)=&token.notes.notebook_ids {" · {ids.len()} 个笔记本"} else {" · 全部笔记本"}
                    }
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
                            class: "inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-xs font-medium text-[var(--color-paper-primary)] hover:bg-[var(--color-paper-theme)] transition-colors cursor-pointer",
                            onclick: move |_| state.editing.set(Some(edit_token.clone())),
                            "编辑权限"
                        }
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
