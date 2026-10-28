//! 草稿/文章预览页面模块（管理员只读）。
//!
//! 对应路由 `/admin/preview/:slug`。
//!
//! 与公开详情页 [`crate::pages::post_detail`] 的关键差异：
//! - 取数走 admin-only [`crate::api::posts::get_post_preview`]，不做 `status = 'published'` 过滤，
//!   故草稿可预览；草稿绝不进公开缓存（`cache::get/set_post_by_slug`）。
//! - 顶部浮起预览条（状态、桌面 / 手机宽度切换、返回编辑、草稿可直接发布），
//!   手机档把正文放进 390px 画框。
//! - 文末说明评论区在预览中隐藏。
//! - 错误/未命中**就地渲染**，不向错误边界上抛。
//! - 不渲染评论区（草稿无评论）。
//!
//! # 为何预览放在 `/admin/*` 下
//! SSR HTML 缓存按 URL 落盘/入内存，不区分用户身份。若管理员在 `/post/<draft-slug>`
//! 看到草稿，渲染出的草稿 HTML 会被缓存，匿名访客再请求同一 URL 即命中缓存、直接拿到
//! 草稿正文——草稿属保密内容，泄漏不可接受。`/admin/preview/<slug>` 受 `admin_guard`
//! 中间件保护：匿名在 SSR 渲染前就被 302 跳走，不产生可泄漏的缓存。
//!
//! # 取数
//! 后台数据是私有的，SSR 不取数：直接访问首屏为骨架屏，水合后由 Effect 拉取（数据加载
//! 规则 2）。以路由参数 `slug` 为键，带请求世代号；`slug` 变化时先清空旧文章再取新文章，
//! 发布后原地刷新（不清空）。

use crate::api::posts::{get_post_preview, SinglePostResponse};
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::post::post_content::PostContent;
use crate::components::post::post_cover::PostCover;
use crate::components::post::post_footer::PostFooter;
use crate::components::post::post_header::PostHeader;
use crate::components::post::post_toc::PostToc;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::post_preview_skeleton::PostPreviewSkeleton;
use crate::components::ui::{
    button_class, use_toast, ButtonSize, ButtonVariant, Icon, IconName, SegmentedControl,
    SegmentedItem, SegmentedKind, SegmentedSize,
};
use crate::models::post::{Post, PostFooterData, PostHeaderData, PostStatus};
use crate::routes::Route;
use crate::utils::error::failure_message;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use leptos_router::{lazy_route, LazyRoute};

/// 预览路由：纯客户端数据，无 SSR 预取。
pub struct PreviewRoute;

#[lazy_route]
impl LazyRoute for PreviewRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <PostPreview/> }.into_any()
    }
}

/// 页面主体的互斥状态；用 `Memo` 去重，同一篇文章内的状态刷新不会重建整个预览。
#[derive(Clone, PartialEq)]
enum Branch {
    /// 取数中，或结果还不属于当前 slug。
    Loading,
    /// 文章不存在、已进回收站或加载失败。
    Missing,
    /// 当前 slug 的文章已就绪。
    Ready,
}

/// 草稿/文章预览页面组件（管理员只读），对应路由 `/admin/preview/:slug`。
///
/// 渲染与公开详情页一致的正文（头部、封面、目录、正文、页脚），顶部加预览横幅。
/// 加载中显示骨架屏；文章不存在或加载失败时就地渲染提示，不向上抛错。
#[component]
pub fn PostPreview() -> impl IntoView {
    let params = use_params_map();
    let slug = Memo::new(move |_| params.read().get("slug").unwrap_or_default());

    let toast = use_toast();
    let mobile = RwSignal::new(false);
    let publishing = RwSignal::new(false);
    // 发布后原地刷新状态。
    let reload = RwSignal::new(0_u32);
    // None=加载中；Some(None)=未找到/失败；Some(Some(post))=成功。错误与未命中同视图。
    let post = RwSignal::new(None::<Option<Post>>);
    // 当前 `post` 对应的 slug：路由参数变了而新结果未到时，不能暴露旧文章。
    let loaded_slug = RwSignal::new(String::new());

    // 请求世代号：slug 变化、发布刷新、卸载都会递增，旧响应据此丢弃。
    let generation = StoredValue::new(0_u64);
    let bump_generation = move || {
        generation
            .try_update_value(|g| {
                *g = g.wrapping_add(1);
                *g
            })
            .unwrap_or(0)
    };
    on_cleanup(move || {
        bump_generation();
    });

    Effect::new(move |_| {
        let current_slug = slug.get();
        let _ = reload.get();
        if loaded_slug.with_untracked(|s| s != &current_slug) {
            post.set(None);
        }
        let request_id = bump_generation();
        leptos::task::spawn_local(async move {
            let resp = get_post_preview(current_slug.clone()).await;
            if generation.try_get_value() != Some(request_id) {
                return;
            }
            post.try_set(Some(resp.ok().and_then(|SinglePostResponse { post }| post)));
            loaded_slug.try_set(current_slug);
        });
    });

    let branch = Memo::new(move |_| {
        if loaded_slug.with(|l| l != &slug.get()) {
            return Branch::Loading;
        }
        match post.with(|p| p.as_ref().map(Option::is_some)) {
            None => Branch::Loading,
            Some(false) => Branch::Missing,
            Some(true) => Branch::Ready,
        }
    });

    move || match branch.get() {
        Branch::Loading => view! {
            <DelayedSkeleton>
                <PostPreviewSkeleton/>
            </DelayedSkeleton>
        }
        .into_any(),
        Branch::Missing => view! {
            <div class="admin-panel">
                <EmptyState
                    icon=IconName::FileX
                    title="找不到这篇文章"
                    description="它可能已被移到回收站或彻底删除。"
                    action=EmptyStateAction::new(
                        "返回文章",
                        move |_| crate::navigation::push(&Route::Posts.to_string()),
                    )
                />
            </div>
        }
        .into_any(),
        Branch::Ready => {
            let Some((post_id, post_slug)) = post.with_untracked(|p| {
                p.as_ref()
                    .and_then(Option::as_ref)
                    .map(|p| (p.id, p.slug.clone()))
            }) else {
                return ().into_any();
            };
            let draft = Memo::new(move |_| {
                post.with(|p| {
                    p.as_ref()
                        .and_then(Option::as_ref)
                        .is_none_or(|p| p.status != PostStatus::Published)
                })
            });
            let publish = move |_| {
                if publishing.get_untracked() {
                    return;
                }
                publishing.set(true);
                leptos::task::spawn_local(async move {
                    match crate::api::posts::batch_posts(
                        vec![post_id],
                        crate::api::posts::BatchPostAction::Publish,
                    )
                    .await
                    {
                        Ok(res) if res.done == 1 => {
                            toast.success("已发布");
                            reload.try_update(|n| *n += 1);
                        }
                        Ok(_) => toast.error("发布失败，请回到编辑页检查后重试"),
                        Err(e) => toast.error(failure_message("发布失败", &e)),
                    }
                    publishing.try_set(false);
                });
            };

            view! {
                <div
                    class="post-preview"
                    data-width=move || if mobile.get() { "mobile" } else { "desktop" }
                >
                    <div class="preview-bar-slot">
                        <div class="preview-bar" role="region" aria-label="预览状态">
                            <span class="preview-bar-state">
                                <span
                                    class="preview-bar-dot"
                                    data-draft=move || draft.get().then_some("true")
                                    aria-hidden="true"
                                ></span>
                                <strong>{move || if draft.get() { "草稿预览" } else { "文章预览" }}</strong>
                                <span class="preview-bar-hint">
                                    {move || {
                                        if draft.get() {
                                            "仅管理员可见 · 显示最近一次保存的内容"
                                        } else {
                                            "已发布 · 显示最近一次保存的内容"
                                        }
                                    }}
                                </span>
                            </span>
                            <span class="preview-bar-sep" aria-hidden="true"></span>
                            <div class="preview-bar-actions">
                                <SegmentedControl
                                    items=vec![
                                        SegmentedItem::new(false, "桌面"),
                                        SegmentedItem::new(true, "手机"),
                                    ]
                                    value=mobile
                                    on_change=move |v| mobile.set(v)
                                    aria_label="预览宽度"
                                    kind=SegmentedKind::Radio
                                    size=SegmentedSize::Sm
                                    class="preview-viewport-switch"
                                />
                                <a
                                    class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                    href=Route::WriteEdit { id: post_id }.to_string()
                                >
                                    "返回编辑"
                                </a>
                                {move || {
                                    if draft.get() {
                                        view! {
                                            <button
                                                class=button_class(ButtonVariant::Primary, ButtonSize::Sm)
                                                type="button"
                                                disabled=move || publishing.get()
                                                on:click=publish
                                            >
                                                {move || if publishing.get() { "发布中…" } else { "发布" }}
                                            </button>
                                        }
                                            .into_any()
                                    } else {
                                        view! {
                                            <a
                                                class=button_class(ButtonVariant::Ghost, ButtonSize::Sm)
                                                href=format!("/post/{post_slug}")
                                                target="_blank"
                                                rel="noopener"
                                            >
                                                "查看文章"
                                                <Icon name=IconName::ArrowUpRight size=13/>
                                            </a>
                                        }
                                            .into_any()
                                    }
                                }}
                            </div>
                        </div>
                    </div>

                    <div class="preview-frame">
                        <article
                            class="post-single animate-page-enter"
                            data-vt-detail=post_id.to_string()
                        >
                            {move || post.with(|p| p.as_ref().and_then(Option::as_ref).map(post_body))}
                        </article>
                    </div>
                </div>
            }
            .into_any()
        }
    }
}

/// 文章正文区：头部、封面、目录、正文、页脚与评论区说明。随文章数据变化（发布后）重建。
fn post_body(post: &Post) -> impl IntoView {
    view! {
        <PostHeader post=PostHeaderData::from(post)/>

        // 如果文章设置了封面图，则渲染封面组件。
        {post
            .cover_image
            .clone()
            .map(|cover| view! { <PostCover src=cover post_id=post.id/> })}

        {post.toc_html.clone().map(|toc| view! { <PostToc toc_html=toc/> })}

        <PostContent content_html=post.content_html.clone().unwrap_or_default()/>
        <PostFooter post=PostFooterData::from(post)/>
        <p class="preview-note">"评论区在预览中隐藏。发布后读者看到的版式与这里一致。"</p>
    }
}
