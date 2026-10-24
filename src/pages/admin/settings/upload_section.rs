//! 上传分区：素材上传弹窗的并发数。

use leptos::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard};
use super::SettingsSection;
use crate::api::settings::{get_upload_settings, update_upload_settings};
use crate::components::ui::{SegmentedControl, SegmentedItem, SegmentedKind, SegmentedSize};
use crate::models::settings::UploadSettings;

async fn save(d: UploadSettings) -> Result<UploadSettings, ServerFnError> {
    update_upload_settings(d.concurrency).await
}

/// 上传分区组件。
#[component]
pub fn UploadSection() -> impl IntoView {
    let st = use_section(SettingsSection::Upload, get_upload_settings, save);
    let items = Signal::stored(
        (1..=8)
            .map(|n| SegmentedItem::new(n, n.to_string()))
            .collect::<Vec<_>>(),
    );
    view! {
        <SettingsCard
            section=SettingsSection::Upload
            description="素材上传弹窗同时上传的张数。"
            ready=st.ready()
            load_failed=st.load_failed()
            failed=st.failed()
        >
            <SettingRow
                label="并发数"
                hint="弹窗按并发数自动拉开张间间隔，聚合速率始终与上传限流对齐，不会触发 429。"
            >
                <SegmentedControl
                    items=items
                    value=st.field(|d| d.concurrency)
                    on_change=move |n| st.edit(|d| d.concurrency = n)
                    aria_label="上传并发数"
                    kind=SegmentedKind::Radio
                    size=SegmentedSize::Sm
                    class="settings-segmented-square"
                />
            </SettingRow>
        </SettingsCard>
    }
}
