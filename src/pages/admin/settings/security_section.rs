//! 安全配置分区（CSRF 可信源、cookie Secure、代理层数、并发会话上限）。
//!
//! 即时生效层：面板保存后失效 moka 缓存，数秒内全链路生效（CSRF 校验、
//! cookie 构造、真实 IP 提取、登录会话淘汰全部走 runtime accessor）。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::settings::{get_security_settings, update_security_settings};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::{FormInput, FormLabel, FormSelect};
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{Checkbox, LoadingButton, ADMIN_CARD_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::models::settings::SecuritySettings;

/// 安全配置分区组件。
#[allow(non_snake_case)]
#[component]
pub fn SecuritySection(toast: Callback<(String, bool)>) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let mut saved: Signal<SecuritySettings> = use_signal(SecuritySettings::default);
        let mut draft: Signal<SecuritySettings> = use_signal(SecuritySettings::default);
        let loading = use_signal(|| true);
        let mut saving = use_signal(|| false);
        let mut just_saved = use_signal(|| false);

        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            {
                let mut saved = saved;
                let mut draft = draft;
                let mut loading = loading;
                spawn(async move {
                    match get_security_settings().await {
                        Ok(s) => {
                            saved.set(s.clone());
                            draft.set(s);
                        }
                        Err(e) => toast.call((format!("加载失败：{e}"), true)),
                    }
                    loading.set(false);
                });
            }
        });

        let dirty = use_memo(move || draft() != saved());

        rsx! {
            div { class: "space-y-6",
                div { class: "{ADMIN_CARD_CLASS} p-6 md:p-8 flex flex-col gap-6",
                    div { class: "flex items-center gap-3",
                        span { class: "inline-flex items-center justify-center w-10 h-10 rounded-full bg-[var(--color-paper-theme)] text-[var(--color-paper-primary)] border border-[var(--color-paper-border)]",
                            svg { class: "w-5 h-5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                                path { d: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" }
                            }
                        }
                        div {
                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]", "安全配置" }
                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                "CSRF 可信来源、Cookie 安全、真实 IP 提取与会话管理。保存后即时生效。"
                            }
                        }
                    }

                    // CSRF 可信源
                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "CSRF 可信来源 (APP_BASE_URL)", html_for: Some("sec-base-url".to_string()) }
                        FormInput {
                            id: Some("sec-base-url".to_string()),
                            r#type: "url",
                            placeholder: "https://your-domain.example",
                            value: draft().app_base_url,
                            oninput: move |v: String| {
                                let mut d = draft();
                                d.app_base_url = v;
                                draft.set(d);
                                just_saved.set(false);
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]",
                            "写请求（POST/PUT/PATCH/DELETE）的 CSRF 校验可信来源。留空回退到 Host 头推导，生产环境强烈建议显式设置。"
                        }
                    }

                    // Cookie Secure
                    label { class: "flex items-center gap-3 cursor-pointer max-w-xl",
                        Checkbox {
                            checked: draft().cookie_secure,
                            onchange: move |checked: bool| {
                                let mut d = draft();
                                d.cookie_secure = checked;
                                draft.set(d);
                                just_saved.set(false);
                            },
                        }
                        div { class: "flex flex-col",
                            span { class: "text-sm font-medium text-[var(--color-paper-primary)]", "Cookie Secure 标志" }
                            span { class: "text-xs text-[var(--color-paper-secondary)]", "启用后浏览器仅在 HTTPS 下发送会话 Cookie。HTTP 生产环境必开。" }
                        }
                    }

                    // 代理层数
                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "反向代理层数 (TRUSTED_PROXY_COUNT)", html_for: Some("sec-proxy".to_string()) }
                        FormSelect {
                            id: Some("sec-proxy".to_string()),
                            value: draft().trusted_proxy_count as i32,
                            options: vec![
                                (0, "0（直接对外服务）"),
                                (1, "1（一层代理，如 nginx/Caddy）"),
                                (2, "2（两层代理）"),
                                (3, "3"),
                                (4, "4"),
                                (5, "5"),
                            ],
                            onchange: move |v: i32| {
                                let mut d = draft();
                                d.trusted_proxy_count = v.max(0) as u32;
                                draft.set(d);
                                just_saved.set(false);
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]",
                            "用于从 X-Forwarded-For 提取真实客户端 IP。设错会允许 IP 伪造（绕过限流）或限流对错对象。"
                        }
                    }

                    // 并发会话上限
                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "单用户最大并发会话数", html_for: Some("sec-sessions".to_string()) }
                        FormSelect {
                            id: Some("sec-sessions".to_string()),
                            value: draft().max_sessions_per_user as i32,
                            options: vec![
                                (1, "1（仅单设备登录）"),
                                (3, "3"),
                                (5, "5（默认）"),
                                (10, "10"),
                                (20, "20"),
                                (50, "50"),
                            ],
                            onchange: move |v: i32| {
                                let mut d = draft();
                                d.max_sessions_per_user = v.max(1) as u32;
                                draft.set(d);
                                just_saved.set(false);
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]",
                            "超出上限时按最旧优先淘汰——新设备登录会让最老的会话自动失效。"
                        }
                    }

                    // 操作行
                    div { class: "flex items-center justify-between gap-4 pt-1",
                        if just_saved() {
                            span { class: "inline-flex items-center gap-1.5 text-xs text-[var(--color-paper-accent)]",
                                svg { class: "w-3.5 h-3.5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.5",
                                    path { stroke_linecap: "round", stroke_linejoin: "round", d: "M5 13l4 4L19 7" }
                                }
                                "已保存"
                            }
                        } else if dirty() {
                            span { class: "text-xs text-[var(--color-paper-secondary)]", "有未保存的更改" }
                        } else {
                            span { class: "text-xs text-transparent select-none", "·" }
                        }
                        LoadingButton {
                            label: "保存设置".to_string(),
                            loading: saving(),
                            disabled: loading() || just_saved() || !dirty(),
                            onclick: move |_| {
                                let d = draft();
                                saving.set(true);
                                spawn(async move {
                                    match update_security_settings(
                                        d.app_base_url, d.cookie_secure, d.trusted_proxy_count, d.max_sessions_per_user,
                                    ).await {
                                        Ok(s) => {
                                            saved.set(s.clone());
                                            draft.set(s);
                                            just_saved.set(true);
                                            toast.call(("保存成功".to_string(), false));
                                        }
                                        Err(e) => toast.call((format!("保存失败：{e}"), true)),
                                    }
                                    saving.set(false);
                                });
                            },
                        }
                    }
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        rsx! { div { class: "p-8 text-[var(--color-paper-secondary)]", "安全配置（前端渲染）" } }
    }
}
