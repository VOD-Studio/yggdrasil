//! 站点分区：页脚 GitHub 链接、首页标语与站点简介。
//!
//! 写入后服务端失效 moka 缓存与全部公开页 SSR 缓存，前台下次访问立即生效。

use dioxus::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard};
use super::SettingsSection;
use crate::api::settings::{get_site_settings, update_site_settings};
use crate::models::settings::{
    DEFAULT_HOME_INTRO, DEFAULT_HOME_SUBTITLE, DEFAULT_HOME_TITLE, DEFAULT_SITE_DESCRIPTION,
};

/// 站点分区组件。
#[component]
pub fn SiteSection() -> Element {
    let st = use_section(
        SettingsSection::Site,
        get_site_settings,
        update_site_settings,
    );
    let mut draft = st.draft;
    let d = draft();
    rsx! {
        SettingsCard {
            section: SettingsSection::Site,
            description: "页脚链接、首页开场标语与站点简介，保存后前台立即生效。",
            ready: st.ready(),
            load_failed: (st.load_failed)(),
            failed: st.failed(),
            SettingRow {
                label: "GitHub 链接",
                html_for: "site-github-url".to_string(),
                hint: "页脚右侧的 GitHub 图标，留空则不展示。可省略 https:// 前缀，保存时自动补全。".to_string(),
                input {
                    id: "site-github-url",
                    class: "field settings-mono",
                    placeholder: "github.com/你的用户名",
                    value: "{d.github_url}",
                    oninput: move |e| draft.write().github_url = e.value(),
                }
            }
            SettingRow {
                label: "标语第一行",
                html_for: "site-home-title".to_string(),
                input {
                    id: "site-home-title",
                    class: "field",
                    maxlength: 40,
                    placeholder: DEFAULT_HOME_TITLE,
                    value: "{d.home_title}",
                    oninput: move |e| draft.write().home_title = e.value(),
                }
            }
            SettingRow {
                label: "标语第二行",
                html_for: "site-home-subtitle".to_string(),
                hint: "首页开场的两行标题，第二行为浅色。标题留空时使用默认文案；只填第一行则只显示一行。".to_string(),
                input {
                    id: "site-home-subtitle",
                    class: "field",
                    maxlength: 40,
                    placeholder: DEFAULT_HOME_SUBTITLE,
                    value: "{d.home_subtitle}",
                    oninput: move |e| draft.write().home_subtitle = e.value(),
                }
            }
            SettingRow {
                label: "首页简介",
                html_for: "site-home-intro".to_string(),
                hint: "最多 200 字，换行会合并为空格。".to_string(),
                input {
                    id: "site-home-intro",
                    class: "field",
                    maxlength: 200,
                    placeholder: DEFAULT_HOME_INTRO,
                    value: "{d.home_intro}",
                    oninput: move |e| draft.write().home_intro = e.value(),
                }
            }
            SettingRow {
                label: "站点简介",
                html_for: "site-description".to_string(),
                hint: "一句话介绍本站，显示在友链页的「交换友链」信息里。".to_string(),
                input {
                    id: "site-description",
                    class: "field",
                    maxlength: 200,
                    placeholder: DEFAULT_SITE_DESCRIPTION,
                    value: "{d.site_description}",
                    oninput: move |e| draft.write().site_description = e.value(),
                }
            }
        }
    }
}
