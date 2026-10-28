//! 接口限流分区：各限流桶（含 MCP 令牌桶）的每秒请求数与突发上限、日限额与回收间隔。
//!
//! 写入数据库，下次启动生效（限流桶在首次请求时从启动配置构建）。

use leptos::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard};
use super::SettingsSection;
use crate::api::error::ApiError;
use crate::api::settings::{get_rate_limit_settings, update_rate_limit_settings};
use crate::components::forms::NumberInput;
use crate::models::settings::RateLimitSettings;

async fn save(d: RateLimitSettings) -> Result<RateLimitSettings, ApiError> {
    update_rate_limit_settings(
        d.strict_per_sec,
        d.strict_burst,
        d.upload_per_sec,
        d.upload_burst,
        d.image_per_sec,
        d.image_burst,
        d.comment_per_sec,
        d.comment_burst,
        d.comment_upload_per_sec,
        d.comment_upload_burst,
        d.comment_upload_daily,
        d.code_exec_per_sec,
        d.code_exec_burst,
        d.code_exec_daily,
        d.unknown_per_sec,
        d.unknown_burst,
        d.mcp_per_sec,
        d.mcp_burst,
        d.mcp_upload_per_sec,
        d.mcp_upload_burst,
        d.gc_interval_secs,
    )
    .await
}

/// 一个限流桶：每秒请求数与突发上限两个字段。
type Bucket = (
    &'static str,
    &'static str,
    fn(&RateLimitSettings) -> (u32, u32),
    fn(&mut RateLimitSettings, u32, u32),
);

const BUCKETS: [Bucket; 9] = [
    (
        "登录与注册",
        "strict",
        |s| (s.strict_per_sec, s.strict_burst),
        |s, a, b| {
            s.strict_per_sec = a;
            s.strict_burst = b;
        },
    ),
    (
        "图片上传",
        "upload",
        |s| (s.upload_per_sec, s.upload_burst),
        |s, a, b| {
            s.upload_per_sec = a;
            s.upload_burst = b;
        },
    ),
    (
        "图片访问",
        "image",
        |s| (s.image_per_sec, s.image_burst),
        |s, a, b| {
            s.image_per_sec = a;
            s.image_burst = b;
        },
    ),
    (
        "发表评论",
        "comment",
        |s| (s.comment_per_sec, s.comment_burst),
        |s, a, b| {
            s.comment_per_sec = a;
            s.comment_burst = b;
        },
    ),
    (
        "评论传图",
        "comment-upload",
        |s| (s.comment_upload_per_sec, s.comment_upload_burst),
        |s, a, b| {
            s.comment_upload_per_sec = a;
            s.comment_upload_burst = b;
        },
    ),
    (
        "运行代码",
        "code-exec",
        |s| (s.code_exec_per_sec, s.code_exec_burst),
        |s, a, b| {
            s.code_exec_per_sec = a;
            s.code_exec_burst = b;
        },
    ),
    (
        "无法识别 IP",
        "unknown",
        |s| (s.unknown_per_sec, s.unknown_burst),
        |s, a, b| {
            s.unknown_per_sec = a;
            s.unknown_burst = b;
        },
    ),
    (
        "MCP 请求",
        "mcp",
        |s| (s.mcp_per_sec, s.mcp_burst),
        |s, a, b| {
            s.mcp_per_sec = a;
            s.mcp_burst = b;
        },
    ),
    (
        "MCP 传图",
        "mcp-upload",
        |s| (s.mcp_upload_per_sec, s.mcp_upload_burst),
        |s, a, b| {
            s.mcp_upload_per_sec = a;
            s.mcp_upload_burst = b;
        },
    ),
];

/// 接口限流分区组件。
#[component]
pub fn RateLimitSection() -> impl IntoView {
    let st = use_section(SettingsSection::RateLimit, get_rate_limit_settings, save);
    view! {
        <SettingsCard
            section=SettingsSection::RateLimit
            description="按 IP 计的令牌桶（MCP 两项按令牌计）：每秒补充的请求数与可以一次用掉的突发上限。"
            ready=st.ready()
            load_failed=st.load_failed()
            failed=st.failed()
        >
            {BUCKETS
                .into_iter()
                .map(|(label, key, get, set)| {
                    view! {
                        <SettingRow label=label>
                            <div class="settings-pair">
                                <NumberInput
                                    id=format!("rl-{key}-rate")
                                    value=st.field(move |d| i64::from(get(d).0))
                                    min=1
                                    max=10_000
                                    unit="次/秒"
                                    onchange=move |v: i64| {
                                        st.edit(|s| {
                                            let (_, b) = get(s);
                                            set(s, v as u32, b);
                                        })
                                    }
                                />
                                <NumberInput
                                    id=format!("rl-{key}-burst")
                                    value=st.field(move |d| i64::from(get(d).1))
                                    min=1
                                    max=10_000
                                    unit="突发"
                                    onchange=move |v: i64| {
                                        st.edit(|s| {
                                            let (a, _) = get(s);
                                            set(s, a, v as u32);
                                        })
                                    }
                                />
                            </div>
                        </SettingRow>
                    }
                })
                .collect_view()}
            <SettingRow label="评论传图日限额" html_for="rl-comment-upload-daily">
                <NumberInput
                    id="rl-comment-upload-daily"
                    value=st.field(|d| i64::from(d.comment_upload_daily))
                    min=1
                    max=100_000
                    unit="次/天"
                    hint="评论区允许匿名传图，日限额是防止磁盘被占满的关键防线。"
                    onchange=move |v: i64| st.edit(|d| d.comment_upload_daily = v as u32)
                />
            </SettingRow>
            <SettingRow label="运行代码日限额" html_for="rl-code-exec-daily">
                <NumberInput
                    id="rl-code-exec-daily"
                    value=st.field(|d| i64::from(d.code_exec_daily))
                    min=1
                    max=100_000
                    unit="次/天"
                    onchange=move |v: i64| st.edit(|d| d.code_exec_daily = v as u32)
                />
            </SettingRow>
            <SettingRow label="回收间隔" html_for="rl-gc">
                <NumberInput
                    id="rl-gc"
                    value=st.field(|d| i64::from(d.gc_interval_secs))
                    min=10
                    max=86_400
                    step=60
                    unit="秒"
                    hint="定期清掉不活跃 IP 的限流桶，释放内存。"
                    onchange=move |v: i64| st.edit(|d| d.gc_interval_secs = v as u32)
                />
            </SettingRow>
        </SettingsCard>
    }
}
