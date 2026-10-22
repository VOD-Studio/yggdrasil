//! 代码运行器分区：联网开关、并发与每任务资源上限、语言白名单。
//!
//! 写入数据库，下次启动生效（RUNNER_CONFIG 在首次访问时从启动配置构建）。

use dioxus::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard, SwitchRow};
use super::SettingsSection;
use crate::api::settings::{get_runner_settings, update_runner_settings};
use crate::components::forms::{FormInput, NumberInput};
use crate::models::settings::RunnerSettings;

async fn save(d: RunnerSettings) -> Result<RunnerSettings, ServerFnError> {
    update_runner_settings(
        d.allow_network,
        d.max_concurrent,
        d.max_cpu_cores,
        d.max_memory_mb,
        d.max_timeout_secs,
        d.max_output_bytes,
        d.max_source_bytes,
        d.queue_timeout_secs,
        d.task_ttl_secs,
        d.languages,
    )
    .await
}

/// 代码运行器分区组件。
#[component]
pub fn RunnerSection() -> Element {
    let st = use_section(SettingsSection::Runner, get_runner_settings, save);
    let mut draft = st.draft;
    let d = draft();
    rsx! {
        SettingsCard {
            section: SettingsSection::Runner,
            description: "文章里可运行代码块的沙箱资源限制。",
            ready: st.ready(),
            load_failed: (st.load_failed)(),
            failed: st.failed(),
            SwitchRow {
                label: "允许容器联网",
                hint: "开启后读者运行的代码可以访问网络，有安全风险。",
                checked: d.allow_network,
                ontoggle: move |_| draft.with_mut(|d| d.allow_network = !d.allow_network),
            }
            SettingRow {
                label: "最大并发任务",
                html_for: "runner-concurrent".to_string(),
                NumberInput {
                    id: "runner-concurrent",
                    value: d.max_concurrent as i64,
                    min: 1,
                    max: 64,
                    unit: "个",
                    onchange: move |v: i64| draft.write().max_concurrent = v as u32,
                }
            }
            SettingRow {
                label: "CPU",
                html_for: "runner-cpu".to_string(),
                hint: "每个任务可用的 CPU 核数，可以是小数。".to_string(),
                div { class: "settings-inline",
                    FormInput {
                        id: Some("runner-cpu".to_string()),
                        class: "field settings-number",
                        r#type: "number",
                        min: "0.1",
                        step: "0.1",
                        value: format!("{:.1}", d.max_cpu_cores),
                        oninput: move |v: String| {
                            if let Ok(v) = v.parse::<f64>() {
                                draft.write().max_cpu_cores = v;
                            }
                        },
                    }
                    span { class: "settings-unit", "核" }
                }
            }
            SettingRow {
                label: "内存",
                html_for: "runner-mem".to_string(),
                NumberInput {
                    id: "runner-mem",
                    value: d.max_memory_mb as i64,
                    min: 16,
                    max: 65_536,
                    step: 64,
                    unit: "MB",
                    onchange: move |v: i64| draft.write().max_memory_mb = v as u32,
                }
            }
            SettingRow {
                label: "执行超时",
                html_for: "runner-timeout".to_string(),
                NumberInput {
                    id: "runner-timeout",
                    value: d.max_timeout_secs as i64,
                    min: 1,
                    max: 600,
                    unit: "秒",
                    onchange: move |v: i64| draft.write().max_timeout_secs = v as u32,
                }
            }
            SettingRow {
                label: "排队超时",
                html_for: "runner-queue".to_string(),
                NumberInput {
                    id: "runner-queue",
                    value: d.queue_timeout_secs as i64,
                    min: 1,
                    max: 600,
                    unit: "秒",
                    onchange: move |v: i64| draft.write().queue_timeout_secs = v as u32,
                }
            }
            SettingRow {
                label: "输出上限",
                html_for: "runner-output".to_string(),
                NumberInput {
                    id: "runner-output",
                    value: (d.max_output_bytes / 1024) as i64,
                    min: 1,
                    max: 65_536,
                    unit: "KB",
                    onchange: move |v: i64| draft.write().max_output_bytes = v as u64 * 1024,
                }
            }
            SettingRow {
                label: "源码上限",
                html_for: "runner-source".to_string(),
                NumberInput {
                    id: "runner-source",
                    value: (d.max_source_bytes / 1024) as i64,
                    min: 1,
                    max: 65_536,
                    unit: "KB",
                    onchange: move |v: i64| draft.write().max_source_bytes = v as u64 * 1024,
                }
            }
            SettingRow {
                label: "任务记录保留",
                html_for: "runner-ttl".to_string(),
                NumberInput {
                    id: "runner-ttl",
                    value: d.task_ttl_secs as i64,
                    min: 10,
                    max: 86_400,
                    step: 60,
                    unit: "秒",
                    hint: "运行结束后保留输出供页面读取的时长。".to_string(),
                    onchange: move |v: i64| draft.write().task_ttl_secs = v as u32,
                }
            }
            SettingRow {
                label: "语言白名单",
                html_for: "runner-langs".to_string(),
                hint: "逗号分隔，留空表示全部注册语言可用。".to_string(),
                FormInput {
                    id: Some("runner-langs".to_string()),
                    class: "field settings-mono",
                    placeholder: "python, node, rust",
                    value: d.languages.clone().unwrap_or_default(),
                    oninput: move |v: String| {
                        draft.write().languages = (!v.trim().is_empty()).then_some(v);
                    },
                }
            }
        }
    }
}
