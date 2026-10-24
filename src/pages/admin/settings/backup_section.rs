//! 自动备份分区：调度开关、执行时间、保留份数、是否包含 uploads。
//!
//! 备份操作（创建、恢复、删除）在「系统」页。执行时间按浏览器本地时间编辑，
//! 服务端以 UTC 存储；写入后唤醒调度任务立即重排。

use leptos::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard, SwitchRow};
use super::SettingsSection;
use crate::api::settings::{get_backup_settings, update_backup_settings};
use crate::components::forms::{NumberInput, TimePicker};
use crate::models::settings::BackupSettings;
use crate::utils::time::{local_hhmm_to_utc, utc_hhmm_to_local};

/// 草稿里的执行时间是本地时间。
fn to_local(mut s: BackupSettings) -> BackupSettings {
    s.time_utc = utc_hhmm_to_local(&s.time_utc);
    s
}

async fn load() -> Result<BackupSettings, ServerFnError> {
    get_backup_settings().await.map(|v| to_local(v.settings))
}

async fn save(d: BackupSettings) -> Result<BackupSettings, ServerFnError> {
    update_backup_settings(
        d.auto_enabled,
        local_hhmm_to_utc(&d.time_utc),
        d.retention_count,
        d.include_uploads,
    )
    .await
    .map(|v| to_local(v.settings))
}

/// 自动备份分区组件。
#[component]
pub fn BackupSection() -> impl IntoView {
    let st = use_section(SettingsSection::Backup, load, save);
    view! {
        <SettingsCard
            section=SettingsSection::Backup
            description="每天定时备份数据库。创建、恢复与删除备份在「系统」页。"
            ready=st.ready()
            load_failed=st.load_failed()
            failed=st.failed()
        >
            <SwitchRow
                label="启用定时备份"
                hint="保存后调度任务立即按新时间重排。"
                checked=st.field(|d| d.auto_enabled)
                ontoggle=move |()| st.edit(|d| d.auto_enabled = !d.auto_enabled)
            />
            <SettingRow label="执行时间" html_for="backup-time" hint="按本地时间显示与编辑，服务端以 UTC 保存。">
                <TimePicker
                    id="backup-time"
                    value=st.field(|d| d.time_utc.clone())
                    onchange=move |v: String| st.edit(|d| d.time_utc = v)
                />
            </SettingRow>
            <SettingRow label="保留份数" html_for="backup-retention">
                <NumberInput
                    id="backup-retention"
                    value=st.field(|d| i64::from(d.retention_count))
                    min=1
                    max=365
                    unit="份"
                    hint="超出后最旧的自动备份连同 uploads 包一起删除；手动备份不会自动删除。"
                    onchange=move |v: i64| st.edit(|d| d.retention_count = v as i32)
                />
            </SettingRow>
            <SwitchRow
                label="包含 uploads"
                hint="同时打包上传的素材（tar.gz，跳过可重建的缓存）。"
                checked=st.field(|d| d.include_uploads)
                ontoggle=move |()| st.edit(|d| d.include_uploads = !d.include_uploads)
            />
        </SettingsCard>
    }
}
