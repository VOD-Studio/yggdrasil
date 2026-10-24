//! 素材详情栏：大图（点击进灯箱）、文件信息、替代文本、引用来源、复制链接与删除。
//! 被引用的素材禁止删除并写明引用数。
use super::card::ref_kind;
use crate::components::forms::FormInput;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, IconButton, IconName,
};
use crate::models::asset::{AssetDto, AssetRef, AssetRefPostStatus};
use crate::routes::Route;
use crate::utils::error::failure_message;
use crate::utils::format_bytes;
use leptos::prelude::*;

/// 引用来源的一行。已发布文章与评论新标签打开前台页（规避后台到前台跨布局导航的
/// 回收问题，也不打断后台上下文）；草稿、回收站、笔记进入后台编辑页；头像只展示。
fn ref_row(r: &AssetRef) -> AnyView {
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
                view! {
                    <a
                        class="asset-ref"
                        href=format!("/post/{slug}")
                        target="_blank"
                        rel="noopener"
                    >
                        <span class="asset-ref-kind">{kind}</span>
                        <span class="asset-ref-label">{title.clone()}</span>
                    </a>
                }
                .into_any()
            } else {
                view! {
                    <a class="asset-ref" href=Route::WriteEdit { id: *post_id }.to_string()>
                        <span class="asset-ref-kind">
                            {kind}
                            {note.map(|note| format!(" · {note}"))}
                        </span>
                        <span class="asset-ref-label">{title.clone()}</span>
                    </a>
                }
                .into_any()
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
                view! {
                    <a
                        class="asset-ref"
                        href=format!("/post/{post_slug}")
                        target="_blank"
                        rel="noopener"
                    >
                        <span class="asset-ref-kind">{kind}</span>
                        <span class="asset-ref-label">{label}</span>
                    </a>
                }
                .into_any()
            } else {
                view! {
                    <a class="asset-ref" href=Route::WriteEdit { id: *post_id }.to_string()>
                        <span class="asset-ref-kind">{kind}</span>
                        <span class="asset-ref-label">{label}</span>
                    </a>
                }
                .into_any()
            }
        }
        AssetRef::Note { note_id, title } => view! {
            <a class="asset-ref" href=Route::EditNote { id: *note_id }.to_string()>
                <span class="asset-ref-kind">{format!("{kind} · 含历史版本")}</span>
                <span class="asset-ref-label">{title.clone()}</span>
            </a>
        }
        .into_any(),
        AssetRef::UserAvatar { label, .. } => view! {
            <div class="asset-ref">
                <span class="asset-ref-kind">{kind}</span>
                <span class="asset-ref-label">{label.clone()}</span>
            </div>
        }
        .into_any(),
        AssetRef::FriendAvatar { name, .. } => view! {
            <a class="asset-ref" href=Route::FriendsAdmin.to_string()>
                <span class="asset-ref-kind">{kind}</span>
                <span class="asset-ref-label">{name.clone()}</span>
            </a>
        }
        .into_any(),
    }
}

/// 详情栏。页面按素材 id 重建本组件，`asset` 随列表刷新更新（替代文本输入框的内容与
/// 焦点因此在保存后保持）。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub(super) fn AssetDetail(
    #[prop(into)] asset: Signal<AssetDto>,
    #[prop(into)] on_changed: Callback<()>,
    #[prop(into)] on_delete: Callback<()>,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let toast = use_toast();
    // 文件本身的字段在同一素材的生命周期内不变，只取一次。
    let a = asset.with_untracked(|d| d.asset.clone());
    let id = a.id.clone();
    let alt = RwSignal::new(a.alt.clone().unwrap_or_default());
    let saving = RwSignal::new(false);
    let used = Memo::new(move |_| asset.with(|d| d.ref_count > 0));
    let ref_count = Memo::new(move |_| asset.with(|d| d.ref_count));
    let saved_alt = Memo::new(move |_| asset.with(|d| d.asset.alt.clone().unwrap_or_default()));
    let refs = Memo::new(move |_| asset.with(|d| d.refs.clone()));
    let missing_alt = Memo::new(move |_| saved_alt.with(|s| s.trim().is_empty()));
    let preview = format!("/uploads/{}?w=640", a.path);
    let url = format!("/uploads/{}", a.path);

    // 失焦与回车共用。
    let save_alt = Callback::new(move |()| {
        let next = alt.get_untracked().trim().to_string();
        if next == saved_alt.with_untracked(|s| s.trim().to_string()) || saving.get_untracked() {
            return;
        }
        saving.set(true);
        let id = id.clone();
        leptos::task::spawn_local(async move {
            match crate::api::assets::update_asset_alt(id, next).await {
                Ok(res) if res.success => {
                    toast.success("替代文本已保存");
                    on_changed.try_run(());
                }
                Ok(res) => toast.error(res.message),
                Err(e) => toast.error(failure_message("保存失败", &e)),
            }
            saving.try_set(false);
        });
    });
    let copy = {
        let url = url.clone();
        move |_| {
            #[cfg(target_arch = "wasm32")]
            {
                let url = url.clone();
                leptos::task::spawn_local(async move {
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
        }
    };

    view! {
        <aside class="asset-detail" aria-label="素材详情">
            <div class="asset-detail-head">
                <h2>"详情"</h2>
                <IconButton
                    icon=IconName::X
                    label="关闭详情"
                    size=ButtonSize::Sm
                    onclick=move |_| on_close.run(())
                />
            </div>
            // 点击大图进入灯箱（lightbox-single：只看这一张，原图 = 去掉 query）。
            <div class="asset-detail-preview assets-lightbox">
                <img
                    class="lightbox-single"
                    src=preview
                    alt=move || saved_alt.get()
                    data-error-text="本地文件已丢失"
                />
            </div>
            <div class="asset-detail-file">
                <p class="asset-detail-name">{a.filename.clone()}</p>
                <p class="asset-detail-meta">
                    {(a.width > 0).then(|| format!("{}×{} · ", a.width, a.height))}
                    {format!("{} · {}", format_bytes(a.size_bytes), a.mime)}
                </p>
                <p class="asset-detail-path">{format!("/uploads/{}", a.path)}</p>
            </div>
            <div>
                <label class="asset-detail-label" for="asset-alt">
                    <span>"替代文本"</span>
                    {move || {
                        missing_alt.get().then(|| view! { <span class="asset-detail-warn">"未填写"</span> })
                    }}
                </label>
                <FormInput
                    id="asset-alt"
                    placeholder="描述这张图片，供读屏与搜索使用"
                    maxlength=500
                    value=alt
                    disabled=saving
                    oninput=move |v: String| alt.set(v)
                    onblur=move |_| save_alt.run(())
                    onkeydown=move |e: leptos::ev::KeyboardEvent| {
                        if e.key() == "Enter" && !e.is_composing() {
                            e.prevent_default();
                            save_alt.run(());
                        }
                    }
                />
            </div>
            <div>
                <p class="asset-detail-label">"引用"</p>
                {move || {
                    refs
                        .with(|refs| {
                            if refs.is_empty() {
                                view! {
                                    <p class="asset-detail-empty">
                                        "没有文章、笔记、评论或头像引用这张图片。"
                                    </p>
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <div class="asset-refs">
                                        {refs
                                            .iter()
                                            .map(|r| view! { <div>{ref_row(r)}</div> })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }
                        })
                }}
            </div>
            <div class="asset-detail-actions">
                <button
                    class=button_class(ButtonVariant::Secondary, ButtonSize::Md)
                    type="button"
                    on:click=copy
                >
                    "复制链接"
                </button>
                <button
                    class=button_class(ButtonVariant::Danger, ButtonSize::Md)
                    type="button"
                    disabled=move || used.get()
                    on:click=move |_| on_delete.run(())
                >
                    "删除"
                </button>
            </div>
            {move || {
                used.get()
                    .then(|| {
                        view! {
                            <p class="asset-detail-note">
                                {move || format!("被 {} 处引用，移除引用后才能删除。", ref_count.get())}
                            </p>
                        }
                    })
            }}
        </aside>
    }
}
