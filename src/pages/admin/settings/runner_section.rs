//! 代码运行器分区：联网开关、并发与每任务资源上限、语言白名单。
//!
//! 写入数据库，下次启动生效（RUNNER_CONFIG 在首次访问时从启动配置构建）。

use leptos::prelude::*;

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
pub fn RunnerSection() -> impl IntoView {
    let st = use_section(SettingsSection::Runner, get_runner_settings, save);
    view! {
        <SettingsCard
            section=SettingsSection::Runner
            description="文章里可运行代码块的沙箱资源限制。"
            ready=st.ready()
            load_failed=st.load_failed()
            failed=st.failed()
        >
            <SwitchRow
                label="允许容器联网"
                hint="开启后读者运行的代码可以访问网络，有安全风险。"
                checked=st.field(|d| d.allow_network)
                ontoggle=move |()| st.edit(|d| d.allow_network = !d.allow_network)
            />
            <SettingRow label="最大并发任务" html_for="runner-concurrent">
                <NumberInput
                    id="runner-concurrent"
                    value=st.field(|d| i64::from(d.max_concurrent))
                    min=1
                    max=64
                    unit="个"
                    onchange=move |v: i64| st.edit(|d| d.max_concurrent = v as u32)
                />
            </SettingRow>
            <SettingRow label="CPU" html_for="runner-cpu" hint="每个任务可用的 CPU 核数，可以是小数。">
                <div class="settings-inline">
                    <FormInput
                        id="runner-cpu"
                        class="field settings-number"
                        r#type="number"
                        min="0.1"
                        step="0.1"
                        value=st.field(|d| format!("{:.1}", d.max_cpu_cores))
                        oninput=move |v: String| {
                            if let Ok(v) = v.parse::<f64>() {
                                st.edit(|d| d.max_cpu_cores = v);
                            }
                        }
                    />
                    <span class="settings-unit">"核"</span>
                </div>
            </SettingRow>
            <SettingRow label="内存" html_for="runner-mem">
                <NumberInput
                    id="runner-mem"
                    value=st.field(|d| i64::from(d.max_memory_mb))
                    min=16
                    max=65_536
                    step=64
                    unit="MB"
                    onchange=move |v: i64| st.edit(|d| d.max_memory_mb = v as u32)
                />
            </SettingRow>
            <SettingRow label="执行超时" html_for="runner-timeout">
                <NumberInput
                    id="runner-timeout"
                    value=st.field(|d| i64::from(d.max_timeout_secs))
                    min=1
                    max=600
                    unit="秒"
                    onchange=move |v: i64| st.edit(|d| d.max_timeout_secs = v as u32)
                />
            </SettingRow>
            <SettingRow label="排队超时" html_for="runner-queue">
                <NumberInput
                    id="runner-queue"
                    value=st.field(|d| i64::from(d.queue_timeout_secs))
                    min=1
                    max=600
                    unit="秒"
                    onchange=move |v: i64| st.edit(|d| d.queue_timeout_secs = v as u32)
                />
            </SettingRow>
            <SettingRow label="输出上限" html_for="runner-output">
                <NumberInput
                    id="runner-output"
                    value=st.field(|d| (d.max_output_bytes / 1024) as i64)
                    min=1
                    max=65_536
                    unit="KB"
                    onchange=move |v: i64| st.edit(|d| d.max_output_bytes = v as u64 * 1024)
                />
            </SettingRow>
            <SettingRow label="源码上限" html_for="runner-source">
                <NumberInput
                    id="runner-source"
                    value=st.field(|d| (d.max_source_bytes / 1024) as i64)
                    min=1
                    max=65_536
                    unit="KB"
                    onchange=move |v: i64| st.edit(|d| d.max_source_bytes = v as u64 * 1024)
                />
            </SettingRow>
            <SettingRow label="任务记录保留" html_for="runner-ttl">
                <NumberInput
                    id="runner-ttl"
                    value=st.field(|d| i64::from(d.task_ttl_secs))
                    min=10
                    max=86_400
                    step=60
                    unit="秒"
                    hint="运行结束后保留输出供页面读取的时长。"
                    onchange=move |v: i64| st.edit(|d| d.task_ttl_secs = v as u32)
                />
            </SettingRow>
            <SettingRow label="语言白名单" html_for="runner-langs" hint="逗号分隔，留空表示全部注册语言可用。">
                <FormInput
                    id="runner-langs"
                    class="field settings-mono"
                    placeholder="python, node, rust"
                    value=st.field(|d| d.languages.clone().unwrap_or_default())
                    oninput=move |v: String| st.edit(|d| d.languages = (!v.trim().is_empty()).then_some(v))
                />
            </SettingRow>
        </SettingsCard>
    }
}
