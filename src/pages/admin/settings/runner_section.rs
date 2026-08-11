//! 代码运行器配置分区（沙箱资源限制）。
//!
//! Tier B 可编辑+重启生效：RUNNER_CONFIG LazyLock 在首次访问时从启动配置构建。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::settings::{get_runner_settings, update_runner_settings};
#[cfg(target_arch = "wasm32")]
use crate::components::forms::{FormLabel, INPUT_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::components::ui::{LoadingButton, ADMIN_CARD_CLASS, CHECKBOX_CLASS};
#[cfg(target_arch = "wasm32")]
use crate::models::settings::RunnerSettings;

/// 代码运行器配置分区组件。
#[allow(non_snake_case)]
#[component]
pub fn RunnerSection(toast: Callback<(String, bool)>) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        let mut saved: Signal<RunnerSettings> = use_signal(RunnerSettings::default);
        let mut draft: Signal<RunnerSettings> = use_signal(RunnerSettings::default);
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
                    match get_runner_settings().await {
                        Ok(s) => { saved.set(s.clone()); draft.set(s); }
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
                                path { d: "M16 18l6-6-6-6 M8 6l-6 6 6 6" }
                            }
                        }
                        div {
                            h2 { class: "text-xl font-bold text-[var(--color-paper-primary)]", "代码运行器" }
                            p { class: "text-sm text-[var(--color-paper-secondary)] mt-0.5",
                                span { class: "text-amber-600 dark:text-amber-400 font-medium", "⚠ 修改后需重启服务生效。" }
                            }
                        }
                    }

                    label { class: "flex items-center gap-3 cursor-pointer max-w-xl",
                        input {
                            r#type: "checkbox",
                            class: "{CHECKBOX_CLASS}",
                            checked: draft().allow_network,
                            onchange: move |e: Event<FormData>| {
                                let mut d = draft(); d.allow_network = e.value() == "true"; draft.set(d); just_saved.set(false);
                            },
                        }
                        div { class: "flex flex-col",
                            span { class: "text-sm font-medium text-[var(--color-paper-primary)]", "允许容器联网" }
                            span { class: "text-xs text-[var(--color-paper-secondary)]", "⚠️ 启用后用户代码可访问网络，存在安全风险。" }
                        }
                    }

                    NumberField { id: "runner-concurrent".to_string(), label: "最大并发任务数".to_string(), value: draft().max_concurrent.to_string(),
                        oninput: move |v: String| { if let Ok(n) = v.parse::<u32>() { let mut d = draft(); d.max_concurrent = n; draft.set(d); just_saved.set(false); } }
                    }
                    NumberField { id: "runner-cpu".to_string(), label: "每任务最大 CPU 核数".to_string(), value: format!("{:.1}", draft().max_cpu_cores),
                        oninput: move |v: String| { if let Ok(n) = v.parse::<f64>() { let mut d = draft(); d.max_cpu_cores = n; draft.set(d); just_saved.set(false); } },
                        is_float: true,
                    }
                    NumberField { id: "runner-mem".to_string(), label: "每任务最大内存（MB）".to_string(), value: draft().max_memory_mb.to_string(),
                        oninput: move |v: String| { if let Ok(n) = v.parse::<u32>() { let mut d = draft(); d.max_memory_mb = n; draft.set(d); just_saved.set(false); } }
                    }
                    NumberField { id: "runner-timeout".to_string(), label: "每任务最大执行超时（秒）".to_string(), value: draft().max_timeout_secs.to_string(),
                        oninput: move |v: String| { if let Ok(n) = v.parse::<u32>() { let mut d = draft(); d.max_timeout_secs = n; draft.set(d); just_saved.set(false); } }
                    }
                    NumberField { id: "runner-output".to_string(), label: "每任务最大输出字节数".to_string(), value: draft().max_output_bytes.to_string(),
                        oninput: move |v: String| { if let Ok(n) = v.parse::<u64>() { let mut d = draft(); d.max_output_bytes = n; draft.set(d); just_saved.set(false); } }
                    }
                    NumberField { id: "runner-source".to_string(), label: "每任务最大源码字节数".to_string(), value: draft().max_source_bytes.to_string(),
                        oninput: move |v: String| { if let Ok(n) = v.parse::<u64>() { let mut d = draft(); d.max_source_bytes = n; draft.set(d); just_saved.set(false); } }
                    }
                    NumberField { id: "runner-queue".to_string(), label: "排队等待超时（秒）".to_string(), value: draft().queue_timeout_secs.to_string(),
                        oninput: move |v: String| { if let Ok(n) = v.parse::<u32>() { let mut d = draft(); d.queue_timeout_secs = n; draft.set(d); just_saved.set(false); } }
                    }
                    NumberField { id: "runner-ttl".to_string(), label: "历史 task 保留时长（秒）".to_string(), value: draft().task_ttl_secs.to_string(),
                        oninput: move |v: String| { if let Ok(n) = v.parse::<u32>() { let mut d = draft(); d.task_ttl_secs = n; draft.set(d); just_saved.set(false); } }
                    }

                    div { class: "flex flex-col gap-2 max-w-xl",
                        FormLabel { label: "语言白名单（逗号分隔，留空=全部）", html_for: Some("runner-langs".to_string()) }
                        input {
                            id: "runner-langs", r#type: "text",
                            class: "{INPUT_CLASS}",
                            value: draft().languages.as_deref().unwrap_or(""),
                            placeholder: "python,node,rust（留空=全部）",
                            oninput: move |e: Event<FormData>| {
                                let v = e.value();
                                let mut d = draft();
                                d.languages = if v.trim().is_empty() { None } else { Some(v) };
                                draft.set(d); just_saved.set(false);
                            },
                        }
                        p { class: "text-xs text-[var(--color-paper-secondary)]", "限制可用语言。留空表示全部注册语言可用。" }
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
                                    match update_runner_settings(d.allow_network, d.max_concurrent, d.max_cpu_cores, d.max_memory_mb, d.max_timeout_secs, d.max_output_bytes, d.max_source_bytes, d.queue_timeout_secs, d.task_ttl_secs, d.languages).await {
                                        Ok(s) => { saved.set(s.clone()); draft.set(s); just_saved.set(true); toast.call(("保存成功".to_string(), false)); }
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
        rsx! { div { class: "p-8 text-[var(--color-paper-secondary)]", "运行器配置（前端渲染）" } }
    }
}
#[cfg(target_arch = "wasm32")]
/// 通用数字输入字段（支持整数和浮点）。
#[allow(non_snake_case)]
#[component]
fn NumberField(
    id: String,
    label: String,
    value: String,
    oninput: Callback<String>,
    #[props(default = false)] is_float: bool,
) -> Element {
    rsx! {
        div { class: "flex flex-col gap-2 max-w-xl",
            FormLabel { label: "{label}", html_for: Some(id.clone()) }
            input {
                id: "{id}",
                r#type: "number",
                step: if is_float { "0.1" } else { "1" },
                class: "{INPUT_CLASS}",
                value: "{value}",
                oninput: move |e: Event<FormData>| oninput.call(e.value()),
            }
        }
    }
}
