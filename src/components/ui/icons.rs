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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_drawable_body() {
        for icon in IconName::ALL {
            let body = icon.body();
            assert!(body.starts_with('<'), "{} 没有图形", icon.slug());
            assert!(!body.contains("<script"), "{} 含脚本", icon.slug());
            assert!(!body.contains("stroke-width"), "{} 自带线宽", icon.slug());
        }
    }

    #[test]
    fn slugs_are_unique() {
        let mut slugs: Vec<_> = IconName::ALL.iter().map(|i| i.slug()).collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), IconName::ALL.len());
    }

    #[test]
    fn decorative_icons_are_hidden_from_assistive_tech() {
        let html = Owner::new().with(|| view! { <Icon name=IconName::Search/> }.to_html());
        assert!(html.contains("aria-hidden=\"true\""), "{html}");
        assert!(html.contains("stroke-width=\"1.5\""), "{html}");
        assert!(html.contains("lucide-search"), "{html}");
        assert!(html.contains("<circle"), "{html}");
    }

    #[test]
    fn labelled_icons_are_announced() {
        let html =
            Owner::new().with(|| view! { <Icon name=IconName::X size=20 label="关闭"/> }.to_html());
        assert!(html.contains("role=\"img\""), "{html}");
        assert!(html.contains("aria-label=\"关闭\""), "{html}");
        assert!(html.contains("width=\"20\""), "{html}");
        assert!(!html.contains("aria-hidden"), "{html}");
    }
}
