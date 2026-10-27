//! 安全分区：CSRF 可信来源、Cookie Secure、可信代理段数、并发会话上限。
//!
//! 保存后失效 moka 缓存，数秒内全链路生效（CSRF 校验、cookie 构造、真实 IP 提取、
//! 登录会话淘汰都走 runtime accessor）。

use leptos::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard, SwitchRow};
use super::SettingsSection;
use crate::api::error::ApiError;
use crate::api::settings::{get_security_settings, update_security_settings};
use crate::components::forms::{FormInput, FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::components::ui::{SegmentedControl, SegmentedItem, SegmentedKind, SegmentedSize};
use crate::models::settings::SecuritySettings;

async fn save(d: SecuritySettings) -> Result<SecuritySettings, ApiError> {
    update_security_settings(
        d.app_base_url,
        d.cookie_secure,
        d.trusted_proxy_count,
        d.max_sessions_per_user,
    )
    .await
}

/// 安全分区组件。
#[component]
pub fn SecuritySection() -> impl IntoView {
    let st = use_section(SettingsSection::Security, get_security_settings, save);
    let proxy_items = Signal::stored(
        (0..=3u32)
            .map(|n| SegmentedItem::new(n, n.to_string()))
            .collect::<Vec<_>>(),
    );
    let session_options = Signal::stored(vec![
        (1u32, "1（仅单设备）"),
        (3, "3"),
        (5, "5（默认）"),
        (10, "10"),
        (20, "20"),
        (50, "50"),
    ]);
    view! {
        <SettingsCard
            section=SettingsSection::Security
            description="CSRF 可信来源、Cookie 与真实 IP 提取。保存后即时生效。"
            ready=st.ready()
            load_failed=st.load_failed()
            failed=st.failed()
        >
            <SettingRow
                label="可信来源"
                html_for="sec-base-url"
                hint="即 APP_BASE_URL，须带 http:// 或 https://，写请求的 CSRF 校验以它为准。留空时只要求来源主机与请求 Host 一致，生产环境建议显式设置。"
            >
                <FormInput
                    id="sec-base-url"
                    class="field settings-mono"
                    r#type="url"
                    placeholder="https://你的站点"
                    value=st.field(|d| d.app_base_url.clone())
                    oninput=move |v: String| st.edit(|d| d.app_base_url = v)
                />
            </SettingRow>
            <SwitchRow
                label="Cookie Secure"
                hint="只在 HTTPS 下发送会话 Cookie。HTTPS 生产环境务必开启。"
                checked=st.field(|d| d.cookie_secure)
                ontoggle=move |()| st.edit(|d| d.cookie_secure = !d.cookie_secure)
            />
            <SettingRow
                label="可信代理段数"
                hint="即 TRUSTED_PROXY_COUNT：X-Forwarded-For 中真实客户端 IP 右侧的段数，不等于代理层数。直连填 0；单层反代覆盖 XFF 并设置 X-Real-IP 时填 1；CDN + 源站反代填 1 而非 2。设大会允许伪造 IP 绕过限流。"
            >
                <SegmentedControl
                    items=proxy_items
                    value=st.field(|d| d.trusted_proxy_count)
                    on_change=move |n: u32| st.edit(|d| d.trusted_proxy_count = n)
                    aria_label="可信代理段数"
                    kind=SegmentedKind::Radio
                    size=SegmentedSize::Sm
                    class="settings-segmented-square"
                />
            </SettingRow>
            <SettingRow
                label="并发会话上限"
                html_for="sec-sessions"
                hint="单个账号同时保持登录的设备数，超出时最早的会话自动失效。"
            >
                <FormSelect
                    id="sec-sessions"
                    value=st.field(|d| d.max_sessions_per_user)
                    options=session_options
                    trigger_class=FORM_SELECT_COMPACT_CLASS
                    onchange=move |v: u32| st.edit(|d| d.max_sessions_per_user = v)
                />
            </SettingRow>
        </SettingsCard>
    }
}
