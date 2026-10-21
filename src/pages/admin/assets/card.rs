//! 素材网格的一格：缩略图、尺寸、文件名、大小与引用概况。点击在详情栏查看，
//! 未引用的素材右上角可勾选，供批量删除。
use crate::components::ui::{Icon, IconName};
use crate::models::asset::{AssetDto, AssetRef};
use crate::utils::format_bytes;
use dioxus::prelude::*;

/// 引用概况：第一处引用的类型，多处时加「等 N 处」。
pub(super) fn ref_summary(asset: &AssetDto) -> String {
    let Some(first) = asset.refs.first() else {
        return "未引用".into();
    };
    let kind = ref_kind(first);
    if asset.ref_count > 1 {
        format!("{kind}等 {} 处", asset.ref_count)
    } else {
        kind.into()
    }
}

/// 引用来源的类型名。
pub(super) fn ref_kind(r: &AssetRef) -> &'static str {
    match r {
        AssetRef::Post { .. } => "文章",
        AssetRef::Note { .. } => "笔记",
        AssetRef::Comment { .. } => "评论",
        AssetRef::UserAvatar { .. } => "头像",
        AssetRef::FriendAvatar { .. } => "友链",
    }
}

#[component]
pub(super) fn AssetTile(
    asset: AssetDto,
    active: bool,
    checked: bool,
    on_open: EventHandler<String>,
    on_check: EventHandler<(String, bool)>,
) -> Element {
    let a = &asset.asset;
    let orphan = asset.ref_count == 0;
    let thumb = format!("/uploads/{}?thumb=300x300", a.path);
    let summary = ref_summary(&asset);
    let id = a.id.clone();
    let check_id = a.id.clone();
    rsx! {
        li {
            class: "asset-tile",
            "data-active": active.then_some("true"),
            "data-checked": checked.then_some("true"),
            button {
                class: "asset-tile-open",
                r#type: "button",
                aria_pressed: "{active}",
                aria_label: "查看 {a.filename}",
                onclick: move |_| on_open.call(id.clone()),
                span { class: "asset-tile-thumb",
                    img {
                        src: "{thumb}",
                        alt: "",
                        loading: "lazy",
                        decoding: "async",
                    }
                    if a.width > 0 {
                        span { class: "asset-tile-dims", "{a.width}×{a.height}" }
                    }
                    if orphan {
                        span { class: "asset-tile-badge", "未引用" }
                    }
                    if a.alt.as_deref().is_none_or(|t| t.trim().is_empty()) {
                        span { class: "asset-tile-noalt", title: "未填写替代文本", aria_hidden: "true" }
                    }
                }
                span { class: "asset-tile-text",
                    span { class: "asset-tile-name", "{a.filename}" }
                    span { class: "asset-tile-meta", "{format_bytes(a.size_bytes)} · {summary}" }
                }
            }
            if orphan {
                label { class: "asset-tile-check", title: if checked { "取消选择" } else { "选择" },
                    input {
                        r#type: "checkbox",
                        checked,
                        aria_label: "选择 {a.filename}",
                        onchange: move |e| on_check.call((check_id.clone(), e.checked())),
                    }
                    Icon { name: IconName::Check, size: 12 }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::asset::{Asset, AssetRefPostStatus};

    fn asset(refs: Vec<AssetRef>) -> AssetDto {
        AssetDto {
            asset: Asset {
                id: "a".into(),
                path: "p.webp".into(),
                filename: "p.webp".into(),
                mime: "image/webp".into(),
                size_bytes: 1,
                width: 1,
                height: 1,
                alt: None,
                created_at: chrono::Utc::now(),
            },
            ref_count: refs.len() as i64,
            refs,
        }
    }

    #[test]
    fn ref_summary_names_the_first_kind_and_counts_the_rest() {
        assert_eq!(ref_summary(&asset(vec![])), "未引用");
        let post = AssetRef::Post {
            post_id: 1,
            title: "t".into(),
            slug: "t".into(),
            status: AssetRefPostStatus::Published,
        };
        let avatar = AssetRef::UserAvatar {
            user_id: 1,
            label: "xfy".into(),
        };
        assert_eq!(ref_summary(&asset(vec![post.clone()])), "文章");
        assert_eq!(ref_summary(&asset(vec![post, avatar])), "文章等 2 处");
    }
}
