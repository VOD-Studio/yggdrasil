//! 上传分区：素材上传弹窗的并发数。

use dioxus::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard};
use super::SettingsSection;
use crate::api::settings::{get_upload_settings, update_upload_settings};
use crate::models::settings::UploadSettings;

async fn save(d: UploadSettings) -> Result<UploadSettings, ServerFnError> {
    update_upload_settings(d.concurrency).await
}

/// 上传分区组件。
#[component]
pub fn UploadSection() -> Element {
    let st = use_section(SettingsSection::Upload, get_upload_settings, save);
    let mut draft = st.draft;
    let current = draft().concurrency;
    rsx! {
        SettingsCard {
            section: SettingsSection::Upload,
            description: "素材上传弹窗同时上传的张数。",
            ready: st.ready(),
            load_failed: (st.load_failed)(),
            failed: st.failed(),
            SettingRow {
                label: "并发数",
                hint: "弹窗按并发数自动拉开张间间隔，聚合速率始终与上传限流对齐，不会触发 429。".to_string(),
                div { class: "segmented settings-segmented", role: "radiogroup", aria_label: "上传并发数",
                    for n in 1..=8 {
                        button {
                            key: "{n}",
                            class: "segmented-item",
                            r#type: "button",
                            role: "radio",
                            aria_checked: "{current == n}",
                            onclick: move |_| draft.write().concurrency = n,
                            "{n}"
                        }
                    }
                }
            }
        }
    }
}
