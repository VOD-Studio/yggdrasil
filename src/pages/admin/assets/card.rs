//! 素材网格的一格：缩略图、尺寸、文件名、大小与引用概况。点击在详情栏查看，
//! 未引用的素材右上角可勾选，供批量删除。
use crate::components::ui::{Icon, IconName};
use crate::models::asset::{AssetDto, AssetRef};
use crate::utils::format_bytes;
use leptos::prelude::*;

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

/// 网格中的一格。
///
/// `asset` 随列表刷新更新（`For` 按 id 复用格子，数据变化不重建 DOM，保住键盘焦点）；
/// `active` / `checked` 由页面按详情栏与勾选集派生。
#[component]
pub(super) fn AssetTile(
    #[prop(into)] asset: Signal<AssetDto>,
    #[prop(into)] active: Signal<bool>,
    #[prop(into)] checked: Signal<bool>,
    #[prop(into)] on_open: Callback<String>,
    #[prop(into)] on_check: Callback<(String, bool)>,
) -> impl IntoView {
    // id 与 path 在同一素材的生命周期内不变。
    let (id, thumb) = asset.with_untracked(|d| {
        (
            d.asset.id.clone(),
            format!("/uploads/{}?thumb=300x300", d.asset.path),
        )
    });
    let check_id = id.clone();
    let filename = move || asset.with(|d| d.asset.filename.clone());
    let orphan = Memo::new(move |_| asset.with(|d| d.ref_count == 0));
    let dims = Memo::new(move |_| {
        asset.with(|d| (d.asset.width > 0).then(|| format!("{}×{}", d.asset.width, d.asset.height)))
    });
    let no_alt = Memo::new(move |_| {
        asset.with(|d| d.asset.alt.as_deref().is_none_or(|t| t.trim().is_empty()))
    });
    let meta = move || {
        asset.with(|d| format!("{} · {}", format_bytes(d.asset.size_bytes), ref_summary(d)))
    };
    view! {
        <li
            class="asset-tile"
            data-active=move || active.get().then_some("true")
            data-checked=move || checked.get().then_some("true")
        >
            <button
                class="asset-tile-open"
                type="button"
                aria-pressed=move || active.get().to_string()
                aria-label=move || format!("查看 {}", filename())
                on:click=move |_| on_open.run(id.clone())
            >
                <span class="asset-tile-thumb">
                    <img src=thumb alt="" loading="lazy" decoding="async"/>
                    {move || dims.get().map(|d| view! { <span class="asset-tile-dims">{d}</span> })}
                    {move || orphan.get().then(|| view! { <span class="asset-tile-badge">"未引用"</span> })}
                    {move || {
                        no_alt.get()
                            .then(|| {
                                view! {
                                    <span
                                        class="asset-tile-noalt"
                                        title="未填写替代文本"
                                        aria-hidden="true"
                                    ></span>
                                }
                            })
                    }}
                </span>
                <span class="asset-tile-text">
                    <span class="asset-tile-name">{filename}</span>
                    <span class="asset-tile-meta">{meta}</span>
                </span>
            </button>
            {move || {
                let check_id = check_id.clone();
                orphan.get()
                    .then(|| {
                        view! {
                            <label
                                class="asset-tile-check"
                                title=move || if checked.get() { "取消选择" } else { "选择" }
                            >
                                <input
                                    type="checkbox"
                                    prop:checked=move || checked.get()
                                    aria-label=move || format!("选择 {}", filename())
                                    on:change=move |e| {
                                        on_check.run((check_id.clone(), event_target_checked(&e)))
                                    }
                                />
                                <Icon name=IconName::Check size=12/>
                            </label>
                        }
                    })
            }}
        </li>
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
