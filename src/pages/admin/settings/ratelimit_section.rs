//! 限流配置分区（各接口速率限制）。
//!
//! Tier B 可编辑+重启生效：面板修改后需重启服务生效（限流桶 LazyLock 在
//! 首次请求时从启动配置构建）。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::settings::{get_rate_limit_settings, update_rate_limit_settings};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::FormLabel;
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{LoadingButton, ADMIN_CARD_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::models::settings::RateLimitSettings;

/// 限流配置分区组件。
#[allow(non_snake_case)]
#[component]
pub fn RateLimitSection(toast: Callback<(String, bool)>) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let mut saved: Signal<RateLimitSettings> = use_signal(RateLimitSettings::default);
        let mut draft: Signal<RateLimitSettings> = use_signal(RateLimitSettings::default);
        let mut loading = use_signal(|| true);
        let mut saving = use_signal(|| false);
        let mut just_saved = use_signal(|| false);

        use_effect(move || {
            #[cfg(target_arch = "wasm32")]
            {
                let mut saved = saved;
                let mut draft = draft;
                let mut loading = loading;
                spawn(async move {
                    match get_rate_limit_settings().await {
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
                                path { d: "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2z M12 6v6l4 2" }
                            }
                        }
                        div {
                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]", "接口限流" }
                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                span { class: "text-amber-600 dark:text-amber-400 font-medium", "⚠ 修改后需重启服务生效。" }
                            }
                        }
                    }

                    // 各限流桶的 per_sec / burst 输入
                    LimiterField { label: "严格限流（登录/注册）", per_sec: draft().strict_per_sec, burst: draft().strict_burst,
                        on_per_sec: move |v| { let mut d = draft(); d.strict_per_sec = v; draft.set(d); just_saved.set(false); },
                        on_burst: move |v| { let mut d = draft(); d.strict_burst = v; draft.set(d); just_saved.set(false); },
                    }
                    LimiterField { label: "上传限流（图片上传）", per_sec: draft().upload_per_sec, burst: draft().upload_burst,
                        on_per_sec: move |v| { let mut d = draft(); d.upload_per_sec = v; draft.set(d); just_saved.set(false); },
                        on_burst: move |v| { let mut d = draft(); d.upload_burst = v; draft.set(d); just_saved.set(false); },
                    }
                    LimiterField { label: "图片访问限流（GET /uploads/*）", per_sec: draft().image_per_sec, burst: draft().image_burst,
                        on_per_sec: move |v| { let mut d = draft(); d.image_per_sec = v; draft.set(d); just_saved.set(false); },
                        on_burst: move |v| { let mut d = draft(); d.image_burst = v; draft.set(d); just_saved.set(false); },
                    }
                    LimiterField { label: "评论限流（创建评论）", per_sec: draft().comment_per_sec, burst: draft().comment_burst,
                        on_per_sec: move |v| { let mut d = draft(); d.comment_per_sec = v; draft.set(d); just_saved.set(false); },
                        on_burst: move |v| { let mut d = draft(); d.comment_burst = v; draft.set(d); just_saved.set(false); },
                    }
                    LimiterField { label: "代码执行限流", per_sec: draft().code_exec_per_sec, burst: draft().code_exec_burst,
                        on_per_sec: move |v| { let mut d = draft(); d.code_exec_per_sec = v; draft.set(d); just_saved.set(false); },
                        on_burst: move |v| { let mut d = draft(); d.code_exec_burst = v; draft.set(d); just_saved.set(false); },
                    }
                    LimiterField { label: "unknown 桶（无法识别 IP）", per_sec: draft().unknown_per_sec, burst: draft().unknown_burst,
                        on_per_sec: move |v| { let mut d = draft(); d.unknown_per_sec = v; draft.set(d); just_saved.set(false); },
                        on_burst: move |v| { let mut d = draft(); d.unknown_burst = v; draft.set(d); just_saved.set(false); },
                    }

                    // 代码执行日限额 + GC 间隔
                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "代码执行日限额（次/天）", html_for: Some("rl-code-daily".to_string()) }
                        input {
                            id: "rl-code-daily", r#type: "number", min: "1",
                            class: "w-full px-4 py-2 border border-[var(--color-paper-border)] rounded-2xl bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] focus:outline-none focus:border-[var(--color-paper-accent)] focus:ring-1 focus:ring-[var(--color-paper-accent)]/30 transition-colors",
                            value: "{draft().code_exec_daily}",
                            oninput: move |e: Event<FormData>| {
                                let v = e.value();
                                if let Ok(n) = v.parse::<u32>() { let mut d = draft(); d.code_exec_daily = n; draft.set(d); just_saved.set(false); }
                            },
                        }
                    }
                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "限流桶 GC 间隔（秒）", html_for: Some("rl-gc".to_string()) }
                        input {
                            id: "rl-gc", r#type: "number", min: "1",
                            class: "w-full px-4 py-2 border border-[var(--color-paper-border)] rounded-2xl bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] focus:outline-none focus:border-[var(--color-paper-accent)] focus:ring-1 focus:ring-[var(--color-paper-accent)]/30 transition-colors",
                            value: "{draft().gc_interval_secs}",
                            oninput: move |e: Event<FormData>| {
                                let v = e.value();
                                if let Ok(n) = v.parse::<u32>() { let mut d = draft(); d.gc_interval_secs = n; draft.set(d); just_saved.set(false); }
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]", "周期性回收已冷却的 IP 键，防止键空间无限膨胀。" }
                    }

                    div { class: "flex items-center justify-between gap-4 pt-1",
                        if just_saved() {
                            span { class: "inline-flex items-center gap-1.5 text-xs text-[var(--color-paper-accent)]",
                                svg { class: "w-3.5 h-3.5", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.5",
                                    path { stroke_linecap: "round", stroke_linejoin: "round", d: "M5 13l4 4L19 7" }
                                }
                                "已保存（重启后生效）"
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
                                    match update_rate_limit_settings(d.strict_per_sec, d.strict_burst, d.upload_per_sec, d.upload_burst, d.image_per_sec, d.image_burst, d.comment_per_sec, d.comment_burst, d.code_exec_per_sec, d.code_exec_burst, d.code_exec_daily, d.unknown_per_sec, d.unknown_burst, d.gc_interval_secs).await {
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
        rsx! { div { class: "p-8 text-[var(--color-paper-secondary)]", "限流配置（前端渲染）" } }
    }
}
#[cfg(target_arch = "wasm32")]
/// 单个限流桶的 per_sec + burst 双输入行。
#[allow(non_snake_case)]
#[component]
fn LimiterField(
    label: String,
    per_sec: u32,
    burst: u32,
    on_per_sec: Callback<u32>,
    on_burst: Callback<u32>,
) -> Element {
    rsx! {
        div { class: "flex flex-col gap-2 max-w-xl",
            FormLabel { label: "{label}", html_for: None }
            div { class: "flex gap-3",
                div { class: "flex-1",
                    input {
                        r#type: "number", min: "1",
                        class: "w-full px-4 py-2 border border-[var(--color-paper-border)] rounded-2xl bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] focus:outline-none focus:border-[var(--color-paper-accent)] focus:ring-1 focus:ring-[var(--color-paper-accent)]/30 transition-colors",
                        value: "{per_sec}",
                        oninput: move |e: Event<FormData>| {
                            let v = e.value();
                            if let Ok(n) = v.parse::<u32>() { on_per_sec.call(n); }
                        },
                    }
                    p { class: "text-xs text-[var(--color-paper-tertiary)] mt-1", "每秒请求数" }
                }
                div { class: "flex-1",
                    input {
                        r#type: "number", min: "1",
                        class: "w-full px-4 py-2 border border-[var(--color-paper-border)] rounded-2xl bg-[var(--color-paper-entry)] text-[var(--color-paper-primary)] focus:outline-none focus:border-[var(--color-paper-accent)] focus:ring-1 focus:ring-[var(--color-paper-accent)]/30 transition-colors",
                        value: "{burst}",
                        oninput: move |e: Event<FormData>| {
                            let v = e.value();
                            if let Ok(n) = v.parse::<u32>() { on_burst.call(n); }
                        },
                    }
                    p { class: "text-xs text-[var(--color-paper-tertiary)] mt-1", "突发上限" }
                }
            }
        }
    }
}
