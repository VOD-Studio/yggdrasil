//! 文章详情页面模块。
//!
//! 对应路由 `/post/:slug`。
//!
//! 数据获取：`LazyRoute::data` 在主包里同步创建 `Resource`，调用
//! `get_public_post_by_slug` server function，根据 URL 中的 slug 获取单篇文章详情
//! （含正文 HTML、目录、封面及上下篇导航）；视图代码在 `--split` 构建下拆进独立
//! WASM 分包，与数据并发加载。
//!
//! # 反应式取数的关键
//! Dioxus 版的 slug 是路由宏注入的普通 `String` prop，被 `move` 进 future 闭包后
//! 成了冻结快照，上/下一篇导航（同一路由间的 slug 变化）复用组件实例却不重跑
//! future——表现为「URL 变了但内容不变」，原版只能另读 `router().current()` 建 memo。
//! Leptos 路由在参数变化时同样复用本路由视图，因此资源必须以 `use_params_map`
//! 派生的 `Memo<String>` 为 source：slug 变化即重新请求，memo 的相等比较过滤
//! 无关的路由通知。
//!
//! # 按文章重建子树
//! 原版用单元素 keyed 列表强制 PostContent / CommentSection / PostToc 随文章
//! remount（重新初始化增强脚本、scroll-spy 与评论资源，issue #10）。这里整篇文章
//! 视图由 `Suspend` 在资源每次解析后重新产出，子组件天然随文章重建，无需 key。

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::{lazy_route, LazyRoute};

use crate::api::posts::get_public_post_by_slug;
use crate::components::comments::section::CommentSection;
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::post::post_content::PostContent;
use crate::components::post::post_cover::PostCover;
use crate::components::post::post_footer::PostFooter;
use crate::components::post::post_header::PostHeader;
use crate::components::post::post_toc::PostToc;
use crate::components::post::share_meta::PostShareMeta;
use crate::components::skeletons::comment_skeleton::CommentListSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::post_detail_skeleton::PostDetailSkeleton;
use crate::components::ui::IconName;
use crate::error::AppError;
use crate::models::post::{Post, PostFooterData, PostHeaderData, PostStatus, PublicPostResponse};
use crate::pages::not_found::NotFound;

/// 文章详情路由，对应 `/post/:slug`。
pub struct PostDetailRoute {
    post: Resource<Result<PublicPostResponse, AppError>>,
}

#[lazy_route]
impl LazyRoute for PostDetailRoute {
    fn data() -> Self {
        let params = use_params_map();
        let slug = Memo::new(move |_| params.read().get("slug").unwrap_or_default());
        Self {
            post: Resource::new(move || slug.get(), get_public_post_by_slug),
        }
    }

    /// 根据 slug 异步获取文章，渲染文章头部、封面、目录、正文、页脚及评论区；
    /// 若文章不存在或加载失败，则展示对应的提示页面（原版由 ErrorLayout 错误边界渲染）。
    fn view(this: Self) -> AnyView {
        let post = this.post;
        view! {
            // 原型专用：分包内的发布标识，滚动发布验证据此判断加载的是哪一版分包。
            <span hidden data-spike-build=crate::BUILD_ID></span>
            <Suspense fallback=|| {
                view! {
                    <DelayedSkeleton>
                        <PostDetailSkeleton/>
                    </DelayedSkeleton>
                }
            }>
                {move || Suspend::new(async move {
                    match post.await {
                        Ok(PublicPostResponse { post: Some(post) }) => {
                            view! { <PostDetail post=post.into_display_post()/> }.into_any()
                        }
                        // 文章不存在：与原版错误边界一致渲染 404 页（NotFound 自行提交 404 状态码）。
                        Ok(PublicPostResponse { post: None }) => view! { <NotFound/> }.into_any(),
                        Err(err) if err.is_not_found() => view! { <NotFound/> }.into_any(),
                        Err(err) => {
                            err.commit_status();
                            view! { <PostLoadFailed/> }.into_any()
                        }
                    }
                })}
            </Suspense>
        }
        .into_any()
    }
}

/// 文章详情主体：文章头部、封面、正文、页脚、评论区与目录。
#[component]
fn PostDetail(post: Post) -> impl IntoView {
    let header = PostHeaderData::from(&post);
    let footer = PostFooterData::from(&post);
    let cover = post
        .cover_image
        .clone()
        .map(|cover| view! { <PostCover src=cover post_id=post.id/> });
    // 仅对已发布文章展示评论区域，Suspense 处理评论加载状态。
    let comments = (post.status == PostStatus::Published).then(|| {
        let post_id = post.id;
        view! {
            <div class="post-comments">
                <Suspense fallback=|| {
                    view! {
                        <DelayedSkeleton>
                            <CommentListSkeleton/>
                        </DelayedSkeleton>
                    }
                }>
                    <CommentSection post_id=post_id/>
                </Suspense>
            </div>
        }
    });
    let toc = post
        .toc_html
        .clone()
        .filter(|t| !t.is_empty())
        .map(|toc| view! { <PostToc toc_html=toc/> });
    let content_html = post.content_html.clone().unwrap_or_default();
    let title = format!("{} · Yggdrasil", post.title);
    let vt_detail = post.id.to_string();

    view! {
        <div class="post-layout">
            <article class="post-single animate-page-enter" data-vt-detail=vt_detail>
                // P4.2：页眉/页脚只取小型字段集，避免复制完整正文。
                <PostHeader post=header/>
                {cover}
                <PostContent content_html=content_html/>
                <PostFooter post=footer/>
                {comments}
            </article>
            {toc}
        </div>
        <Title text=title/>
        <PostShareMeta post=post/>
    }
}

/// 加载失败（非 404）的兜底，对应原版 `ErrorLayout` 的通用 fallback。
#[component]
fn PostLoadFailed() -> impl IntoView {
    let navigate = use_navigate();
    let action = EmptyStateAction {
        label: "返回首页".to_string(),
        onclick: Callback::new(move |_| navigate("/", Default::default())),
    };
    view! {
        <div class="flex min-h-[50vh] items-center justify-center px-6 md:min-h-[55vh] animate-page-enter">
            <EmptyState
                icon=IconName::CircleAlert
                title="页面加载失败"
                description="网络或服务出了点问题，稍后再试一次。"
                action=action
            />
        </div>
    }
}
