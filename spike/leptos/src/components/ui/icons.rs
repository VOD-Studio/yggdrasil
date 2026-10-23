//! Lucide 图标（ISC 许可，见 `public/icons/LICENSE-lucide.txt`）。
//!
//! 全站统一 1.5 线宽、`currentColor` 描边，颜色随文字。图标数据由
//! `libs/shared/scripts/generate-icons.ts` 生成到 `icons_data.rs`。
use leptos::prelude::*;

#[path = "icons_data.rs"]
mod data;

pub use data::IconName;

/// 内联 SVG 图标。
///
/// - 默认装饰性（`aria-hidden`）；传 `label` 时作为独立图形朗读（`role="img"`）。
/// - `size` 为像素边长，默认 16；行内图标可加 `class="icon-inline"` 做 -0.125em 光学对齐。
#[component]
pub fn Icon(
    name: IconName,
    #[prop(default = 16)] size: u32,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    let body = name.body();
    let slug = name.slug();
    let class = format!("lucide lucide-{slug} shrink-0 {class}");
    let size = size.to_string();
    match label {
        Some(label) => view! {
            <svg
                class=class
                width=size.clone()
                height=size
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
                role="img"
                aria-label=label
                inner_html=body
            ></svg>
        }
        .into_any(),
        None => view! {
            <svg
                class=class
                width=size.clone()
                height=size
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
                focusable="false"
                inner_html=body
            ></svg>
        }
        .into_any(),
    }
}
