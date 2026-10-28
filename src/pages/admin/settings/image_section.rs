//! 图片处理分区：WebP 编码质量与方法、尺寸上限、尺寸缓存 TTL。
//!
//! 写入数据库，下次启动生效（编码器与解码上限在启动时读取）。

use leptos::prelude::*;

use super::kit::{use_section, SettingRow, SettingsCard};
use super::SettingsSection;
use crate::api::error::ApiError;
use crate::api::settings::{
    get_image_limit_settings, get_webp_settings, update_image_limit_settings, update_webp_settings,
};
use crate::components::forms::NumberInput;
use crate::components::ui::{SegmentedControl, SegmentedItem, SegmentedKind, SegmentedSize};
use crate::models::settings::{ImageLimitSettings, WebpSettings};

/// WebP 编码与尺寸上限一起编辑、一起保存。
#[derive(Clone, Debug, Default, PartialEq)]
struct ImageDraft {
    webp: WebpSettings,
    limit: ImageLimitSettings,
}

async fn load() -> Result<ImageDraft, ApiError> {
    Ok(ImageDraft {
        webp: get_webp_settings().await?,
        limit: get_image_limit_settings().await?,
    })
}

async fn save(d: ImageDraft) -> Result<ImageDraft, ApiError> {
    Ok(ImageDraft {
        webp: update_webp_settings(d.webp.quality, d.webp.method).await?,
        limit: update_image_limit_settings(
            d.limit.max_dimension,
            d.limit.max_pixels,
            d.limit.dimensions_cache_ttl_secs,
        )
        .await?,
    })
}

/// 图片处理分区组件。
#[component]
pub fn ImageSection() -> impl IntoView {
    let st = use_section(SettingsSection::Image, load, save);
    let quality = st.field(|d| d.webp.quality.round() as i64);
    let method_items = Signal::stored(
        (0..=6u32)
            .map(|n| SegmentedItem::new(n, n.to_string()))
            .collect::<Vec<_>>(),
    );
    view! {
        <SettingsCard
            section=SettingsSection::Image
            description="上传图片转码为 WebP 的质量与速度，以及解码尺寸上限。"
            ready=st.ready()
            load_failed=st.load_failed()
            failed=st.failed()
        >
            <SettingRow label="质量系数" html_for="webp-quality" hint="0 体积最小，100 质量最好。默认 85。">
                <div class="settings-range">
                    <input
                        id="webp-quality"
                        type="range"
                        min="0"
                        max="100"
                        step="1"
                        prop:value=move || quality.get().to_string()
                        on:input=move |e| {
                            if let Ok(v) = event_target_value(&e).parse::<f32>() {
                                st.edit(|d| d.webp.quality = v);
                            }
                        }
                    />
                    <output for="webp-quality">{move || quality.get()}</output>
                </div>
            </SettingRow>
            <SettingRow label="编码方法" hint="越大压缩率越高、越慢。默认 2。">
                <SegmentedControl
                    items=method_items
                    value=st.field(|d| d.webp.method)
                    on_change=move |n| st.edit(|d| d.webp.method = n)
                    aria_label="编码方法"
                    kind=SegmentedKind::Radio
                    size=SegmentedSize::Sm
                    class="settings-segmented-square"
                />
            </SettingRow>
            <SettingRow label="单边最大尺寸" html_for="img-dim">
                <NumberInput
                    id="img-dim"
                    value=st.field(|d| i64::from(d.limit.max_dimension))
                    min=512
                    max=65_535
                    step=512
                    unit="像素"
                    hint="上传与动态缩放的宽、高上限。默认 8192。"
                    onchange=move |v: i64| st.edit(|d| d.limit.max_dimension = v as u32)
                />
            </SettingRow>
            <SettingRow label="总像素上限" html_for="img-px">
                <NumberInput
                    id="img-px"
                    value=st.field(|d| (d.limit.max_pixels / 1_000_000) as i64)
                    min=1
                    max=1_000
                    unit="百万"
                    hint="决定单图解码内存（像素 × 4 + 1 MB）。默认 50 百万，约 7000 × 7000。"
                    onchange=move |v: i64| st.edit(|d| d.limit.max_pixels = v as u64 * 1_000_000)
                />
            </SettingRow>
            <SettingRow label="尺寸缓存" html_for="img-ttl">
                <NumberInput
                    id="img-ttl"
                    value=st.field(|d| (d.limit.dimensions_cache_ttl_secs / 3600) as i64)
                    min=1
                    max=8_760
                    unit="小时"
                    hint="图片宽高的缓存时长，用于预留图片位置。默认 24 小时。"
                    onchange=move |v: i64| st.edit(|d| d.limit.dimensions_cache_ttl_secs = v as u64 * 3600)
                />
            </SettingRow>
        </SettingsCard>
    }
}
