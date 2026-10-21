//! 素材详情栏：大图（点击进灯箱）、文件信息、替代文本、引用来源、复制链接与删除。
//! 被引用的素材禁止删除并写明引用数。
use super::card::ref_kind;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, IconButton, IconName,
};
use crate::models::asset::{AssetDto, AssetRef, AssetRefPostStatus};
use crate::router::Route;
use crate::utils::format_bytes;
use dioxus::prelude::*;

/// 引用来源的一行。已发布文章与评论新标签打开前台页（规避后台到前台跨布局导航的
/// 回收问题，也不打断后台上下文）；草稿、回收站、笔记进入后台编辑页；头像只展示。
fn ref_row(r: &AssetRef) -> Element {
    let kind = ref_kind(r);
    match r {
        AssetRef::Post {
            post_id,
            title,
            slug,
            status,
        } => {
            let note = status.note();
            if *status == AssetRefPostStatus::Published {
                rsx! {
                    a { class: "asset-ref", href: "/post/{slug}", target: "_blank", rel: "noopener",
                        span { class: "asset-ref-kind", "{kind}" }
                        span { class: "asset-ref-label", "{title}" }
                    }
                }
            } else {
                rsx! {
                    Link { class: "asset-ref", to: Route::WriteEdit { id: *post_id },
                        span { class: "asset-ref-kind",
                            "{kind}"
                            if let Some(note) = note {
                                " · {note}"
                            }
                        }
                        span { class: "asset-ref-label", "{title}" }
                    }
                }
            }
        }
        AssetRef::Comment {
            author_name,
            post_id,
            post_title,
            post_slug,
            post_status,
            ..
        } => {
            let label = format!("{author_name} 于《{post_title}》");
            if *post_status == AssetRefPostStatus::Published {
                rsx! {
                    a { class: "asset-ref", href: "/post/{post_slug}", target: "_blank", rel: "noopener",
                        span { class: "asset-ref-kind", "{kind}" }
                        span { class: "asset-ref-label", "{label}" }
                    }
                }
            } else {
                rsx! {
                    Link { class: "asset-ref", to: Route::WriteEdit { id: *post_id },
                        span { class: "asset-ref-kind", "{kind}" }
                        span { class: "asset-ref-label", "{label}" }
                    }
                }
            }
        }
        AssetRef::Note { note_id, title } => rsx! {
            Link { class: "asset-ref", to: Route::EditNote { id: *note_id },
                span { class: "asset-ref-kind", "{kind} · 含历史版本" }
                span { class: "asset-ref-label", "{title}" }
            }
        },
        AssetRef::UserAvatar { label, .. } => rsx! {
            div { class: "asset-ref",
                span { class: "asset-ref-kind", "{kind}" }
                span { class: "asset-ref-label", "{label}" }
            }
        },
        AssetRef::FriendAvatar { name, .. } => rsx! {
            Link { class: "asset-ref", to: Route::FriendsAdmin {},
                span { class: "asset-ref-kind", "{kind}" }
                span { class: "asset-ref-label", "{name}" }
            }
        },
    }
}

#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn AssetDetail(
    asset: AssetDto,
    on_changed: EventHandler<()>,
    on_delete: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let toast = use_toast();
    let a = asset.asset.clone();
    let saved_alt = a.alt.clone().unwrap_or_default();
    let mut alt = use_signal(|| saved_alt.clone());
    let mut saving = use_signal(|| false);
    let used = asset.ref_count > 0;
    let preview = format!("/uploads/{}?w=640", a.path);
    let url = format!("/uploads/{}", a.path);
    let missing_alt = saved_alt.trim().is_empty();
    let id = a.id.clone();

    // Callback 可复制，失焦与回车共用。
    let save_alt = use_callback(move |()| {
        let next = alt.peek().trim().to_string();
        if next == saved_alt.trim() || saving() {
            return;
        }
        saving.set(true);
        let id = id.clone();
        spawn(async move {
            match crate::api::assets::update_asset_alt(id, next).await {
                Ok(res) if res.success => {
                    toast.success("替代文本已保存");
                    on_changed.call(());
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(format!("保存失败：{e}")),
            }
            saving.set(false);
        });
    });
    let copy = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let url = url.clone();
            spawn(async move {
                let origin = web_sys::window()
                    .and_then(|w| w.location().origin().ok())
                    .unwrap_or_default();
                if crate::utils::js::copy_text(&format!("{origin}{url}")).await {
                    toast.success("已复制图片链接");
                } else {
                    toast.error("复制失败，请手动复制路径");
                }
            });
        }
    };

    rsx! {
        aside { class: "asset-detail", aria_label: "素材详情",
            div { class: "asset-detail-head",
                h2 { "详情" }
                IconButton {
                    icon: IconName::X,
                    label: "关闭详情",
                    size: ButtonSize::Sm,
                    onclick: move |_| on_close.call(()),
                }
            }
            // 点击大图进入灯箱（lightbox-single：只看这一张，原图 = 去掉 query）。
            div { class: "asset-detail-preview assets-lightbox",
                img {
                    class: "lightbox-single",
                    src: "{preview}",
                    alt: "{a.alt.clone().unwrap_or_default()}",
                    "data-error-text": "本地文件已丢失",
                }
            }
            div { class: "asset-detail-file",
                p { class: "asset-detail-name", "{a.filename}" }
                p { class: "asset-detail-meta",
                    if a.width > 0 {
                        "{a.width}×{a.height} · "
                    }
                    "{format_bytes(a.size_bytes)} · {a.mime}"
                }
                p { class: "asset-detail-path", "/uploads/{a.path}" }
            }
            div {
                label { class: "asset-detail-label", r#for: "asset-alt",
                    span { "替代文本" }
                    if missing_alt {
                        span { class: "asset-detail-warn", "未填写" }
                    }
                }
                input {
                    id: "asset-alt",
                    class: "field",
                    placeholder: "描述这张图片，供读屏与搜索使用",
                    maxlength: 500,
                    value: "{alt}",
                    disabled: saving(),
                    oninput: move |e| alt.set(e.value()),
                    onblur: move |_| save_alt(()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !e.data().is_composing() {
                            e.prevent_default();
                            save_alt(());
                        }
                    },
                }
            }
            div {
                p { class: "asset-detail-label", "引用" }
                if asset.refs.is_empty() {
                    p { class: "asset-detail-empty", "没有文章、笔记、评论或头像引用这张图片。" }
                } else {
                    div { class: "asset-refs",
                        for (i, r) in asset.refs.iter().enumerate() {
                            div { key: "{i}", {ref_row(r)} }
                        }
                    }
                }
            }
            div { class: "asset-detail-actions",
                button {
                    class: button_class(ButtonVariant::Secondary, ButtonSize::Md),
                    r#type: "button",
                    onclick: copy,
                    "复制链接"
                }
                button {
                    class: button_class(ButtonVariant::Danger, ButtonSize::Md),
                    r#type: "button",
                    disabled: used,
                    onclick: move |_| on_delete.call(()),
                    "删除"
                }
            }
            if used {
                p { class: "asset-detail-note",
                    "被 {asset.ref_count} 处引用，移除引用后才能删除。"
                }
            }
        }
    }
}
