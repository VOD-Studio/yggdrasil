//! 页面缓存分区：匿名公开页 HTML 缓存的单页上限。
//!
//! 写入数据库，下次启动生效（HTML 缓存在首次使用时读取）。

use leptos::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard};
use super::SettingsSection;
use crate::api::error::ApiError;
use crate::api::settings::{get_ssr_cache_settings, update_ssr_cache_settings};
use crate::components::forms::NumberInput;
use crate::models::settings::SsrCacheSettings;

async fn save(d: SsrCacheSettings) -> Result<SsrCacheSettings, ApiError> {
    update_ssr_cache_settings(d.max_page_bytes).await
}

/// 页面缓存分区组件。
#[component]
pub fn PageCacheSection() -> impl IntoView {
    let st = use_section(SettingsSection::PageCache, get_ssr_cache_settings, save);
    view! {
        <SettingsCard
            section=SettingsSection::PageCache
            description="匿名访客的公开页 HTML 缓存在内存里，总量 32 MiB。"
            ready=st.ready()
            load_failed=st.load_failed()
            failed=st.failed()
        >
            <SettingRow label="单页上限" html_for="ssr-cache-max-page">
                <NumberInput
                    id="ssr-cache-max-page"
                    value=st.field(|d| (d.max_page_bytes / 1024) as i64)
                    min=64
                    max=32_768
                    step=256
                    unit="KB"
                    hint="渲染后更大的页面不缓存，每次请求重新渲染。默认 2048 KB。"
                    onchange=move |v: i64| st.edit(|d| d.max_page_bytes = v as u64 * 1024)
                />
            </SettingRow>
        </SettingsCard>
    }
}
