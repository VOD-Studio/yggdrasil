//! 图片缓存分区：`uploads/.cache/` 的容量与保留时长。
//!
//! 清理任务每小时 tick 时从 runtime accessor 读取最新值，保存后即时生效。

use leptos::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard};
use super::SettingsSection;
use crate::api::error::ApiError;
use crate::api::settings::{get_image_cache_settings, update_image_cache_settings};
use crate::components::forms::NumberInput;
use crate::models::settings::ImageCacheSettings;

async fn save(d: ImageCacheSettings) -> Result<ImageCacheSettings, ApiError> {
    update_image_cache_settings(d.disk_cache_max_mb, d.disk_cache_max_age_hours).await
}

/// 图片缓存分区组件。
#[component]
pub fn CacheSection() -> impl IntoView {
    let st = use_section(SettingsSection::Cache, get_image_cache_settings, save);
    view! {
        <SettingsCard
            section=SettingsSection::Cache
            description="缩略图与缩放图的磁盘缓存，清理任务每小时扫描一次。"
            ready=st.ready()
            load_failed=st.load_failed()
            failed=st.failed()
        >
            <SettingRow label="最大容量" html_for="cache-max-mb">
                <NumberInput
                    id="cache-max-mb"
                    value=st.field(|d| i64::from(d.disk_cache_max_mb))
                    min=1
                    max=1_048_576
                    step=128
                    unit="MB"
                    hint="超限时按修改时间删除最旧的缓存文件。默认 1024 MB。"
                    onchange=move |v: i64| st.edit(|d| d.disk_cache_max_mb = v as u32)
                />
            </SettingRow>
            <SettingRow label="最长保留" html_for="cache-max-age">
                <NumberInput
                    id="cache-max-age"
                    value=st.field(|d| i64::from(d.disk_cache_max_age_hours))
                    min=1
                    max=87_600
                    step=24
                    unit="小时"
                    hint="超期的缓存文件优先删除。默认 168 小时（7 天）。"
                    onchange=move |v: i64| st.edit(|d| d.disk_cache_max_age_hours = v as u32)
                />
            </SettingRow>
        </SettingsCard>
    }
}
