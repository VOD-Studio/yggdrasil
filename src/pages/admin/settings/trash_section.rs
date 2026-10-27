//! 回收站分区：已删除文章的自动清理开关与保留天数。
//!
//! 调度任务每小时读取 DB 值执行清理。

use leptos::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard, SwitchRow};
use super::SettingsSection;
use crate::api::error::ApiError;
use crate::api::settings::{get_trash_settings, update_trash_settings};
use crate::components::forms::NumberInput;
use crate::models::settings::TrashSettings;

async fn save(d: TrashSettings) -> Result<TrashSettings, ApiError> {
    update_trash_settings(d.auto_purge_enabled, d.retention_days).await
}

/// 回收站分区组件。
#[component]
pub fn TrashSection() -> impl IntoView {
    let st = use_section(SettingsSection::Trash, get_trash_settings, save);
    view! {
        <SettingsCard
            section=SettingsSection::Trash
            description="已删除的文章超过保留天数后，由后台任务彻底删除。"
            ready=st.ready()
            load_failed=st.load_failed()
            failed=st.failed()
        >
            <SwitchRow
                label="启用自动清理"
                hint="关闭后需要在回收站手动彻底删除。"
                checked=st.field(|d| d.auto_purge_enabled)
                ontoggle=move |()| st.edit(|d| d.auto_purge_enabled = !d.auto_purge_enabled)
            />
            <SettingRow label="保留天数" html_for="trash-days">
                <NumberInput
                    id="trash-days"
                    value=st.field(|d| i64::from(d.retention_days))
                    min=1
                    max=365
                    unit="天"
                    hint="回收站列表据此显示每篇的剩余天数。"
                    onchange=move |v: i64| st.edit(|d| d.retention_days = v as i32)
                />
            </SettingRow>
        </SettingsCard>
    }
}
