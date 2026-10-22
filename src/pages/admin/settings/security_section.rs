//! 安全分区：CSRF 可信来源、Cookie Secure、可信代理段数、并发会话上限。
//!
//! 保存后失效 moka 缓存，数秒内全链路生效（CSRF 校验、cookie 构造、真实 IP 提取、
//! 登录会话淘汰都走 runtime accessor）。

use dioxus::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard, SwitchRow};
use super::SettingsSection;
use crate::api::settings::{get_security_settings, update_security_settings};
use crate::components::forms::{FormSelect, FORM_SELECT_COMPACT_CLASS};
use crate::models::settings::SecuritySettings;

async fn save(d: SecuritySettings) -> Result<SecuritySettings, ServerFnError> {
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
pub fn SecuritySection() -> Element {
    let st = use_section(SettingsSection::Security, get_security_settings, save);
    let mut draft = st.draft;
    let d = draft();
    rsx! {
        SettingsCard {
            section: SettingsSection::Security,
            description: "CSRF 可信来源、Cookie 与真实 IP 提取。保存后即时生效。",
            ready: st.ready(),
            load_failed: (st.load_failed)(),
            failed: st.failed(),
            SettingRow {
                label: "可信来源",
                html_for: "sec-base-url".to_string(),
                hint: "即 APP_BASE_URL，写请求的 CSRF 校验以它为准。留空时从 Host 头推导，生产环境建议显式设置。".to_string(),
                input {
                    id: "sec-base-url",
                    class: "field settings-mono",
                    r#type: "url",
                    placeholder: "https://你的站点",
                    value: "{d.app_base_url}",
                    oninput: move |e| draft.write().app_base_url = e.value(),
                }
            }
            SwitchRow {
                label: "Cookie Secure",
                hint: "只在 HTTPS 下发送会话 Cookie。HTTPS 生产环境务必开启。",
                checked: d.cookie_secure,
                ontoggle: move |_| draft.with_mut(|d| d.cookie_secure = !d.cookie_secure),
            }
            SettingRow {
                label: "可信代理段数",
                hint: "即 TRUSTED_PROXY_COUNT：X-Forwarded-For 中真实客户端 IP 右侧的段数，不等于代理层数。直连填 0；单层反代覆盖 XFF 并设置 X-Real-IP 时填 1；CDN + 源站反代填 1 而非 2。设大会允许伪造 IP 绕过限流。".to_string(),
                div { class: "segmented settings-segmented", role: "radiogroup", aria_label: "可信代理段数",
                    for n in 0..=3u32 {
                        button {
                            key: "{n}",
                            class: "segmented-item",
                            r#type: "button",
                            role: "radio",
                            aria_checked: "{d.trusted_proxy_count == n}",
                            onclick: move |_| draft.write().trusted_proxy_count = n,
                            "{n}"
                        }
                    }
                }
            }
            SettingRow {
                label: "并发会话上限",
                html_for: "sec-sessions".to_string(),
                hint: "单个账号同时保持登录的设备数，超出时最早的会话自动失效。".to_string(),
                FormSelect {
                    id: "sec-sessions",
                    value: d.max_sessions_per_user,
                    options: vec![(1, "1（仅单设备）"), (3, "3"), (5, "5（默认）"), (10, "10"), (20, "20"), (50, "50")],
                    trigger_class: FORM_SELECT_COMPACT_CLASS,
                    onchange: move |v: u32| draft.write().max_sessions_per_user = v,
                }
            }
        }
    }
}
