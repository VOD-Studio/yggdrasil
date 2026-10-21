//! 素材清理分区：无引用图片的自动清理开关与保留天数。
//!
//! 后台任务每天读取 DB 值执行：没有文章、笔记、评论或头像引用的素材，超过保留天数
//! 后物理删除（文件、DB 行与派生缓存）。

use dioxus::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard, SwitchRow};
use super::SettingsSection;
use crate::api::settings::{get_asset_purge_settings, update_asset_purge_settings};
use crate::components::forms::NumberInput;
use crate::models::settings::AssetPurgeSettings;

async fn save(d: AssetPurgeSettings) -> Result<AssetPurgeSettings, ServerFnError> {
    update_asset_purge_settings(d.auto_purge_enabled, d.retention_days).await
}

/// 素材清理分区组件。
#[component]
pub fn AssetPurgeSection() -> Element {
    let st = use_section(SettingsSection::AssetPurge, get_asset_purge_settings, save);
    let mut draft = st.draft;
    let d = draft();
    rsx! {
        SettingsCard {
            section: SettingsSection::AssetPurge,
            description: "没有任何引用的图片超过保留天数后自动删除，每天执行一次。",
            ready: st.ready(),
            load_failed: (st.load_failed)(),
            failed: st.failed(),
            SwitchRow {
                label: "启用自动清理",
                hint: "关闭后未引用图片一直保留，需要在素材页手动清理。评论区允许匿名传图，建议保持开启。",
                checked: d.auto_purge_enabled,
                ontoggle: move |_| draft.with_mut(|d| d.auto_purge_enabled = !d.auto_purge_enabled),
            }
            SettingRow {
                label: "保留天数",
                html_for: "asset-purge-days".to_string(),
                NumberInput {
                    id: "asset-purge-days",
                    value: d.retention_days as i64,
                    min: 1,
                    max: 365,
                    unit: "天",
                    hint: "上传满这么多天仍未被引用才会删除，以免误删还没保存的草稿里的图片。".to_string(),
                    onchange: move |v: i64| draft.write().retention_days = v as i32,
                }
            }
        }
    }
}
