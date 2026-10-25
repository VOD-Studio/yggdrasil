//! 文章详情页面模块。
//!
//! 对应路由 `/post/:slug`（`SsrMode::Async`：`<Title>`/`<Meta>` 与 404 状态码依赖文章数据）。
//!
//! 数据获取：`LazyRoute::data` 在主包里同步创建 `Resource`，调用
//! `get_public_post_by_slug` server function，根据 URL 中的 slug 获取单篇文章详情
//! （含正文 HTML、目录、封面及上下篇导航）；视图代码在 `--split` 构建下拆进独立
//! WASM 分包，与数据并发加载。
//!
//! # 反应式取数的关键
//! Leptos 路由在参数变化时复用本路由视图（上/下一篇导航即同一路由间的 slug 变化），
//! 因此资源必须以 `use_params_map` 派生的 `Memo<String>` 为 source：slug 变化即重新
//! 请求，memo 的相等比较过滤无关的路由通知。
//!
//! # 按文章重建子树
//! `PostContent`、`PostToc`、`CommentSection` 的函数体只运行一次（增强脚本初始化、
//! scroll-spy 绑定、评论资源都在挂载时建立），必须随文章重建。整篇文章视图由 `Suspend`
//! 在资源每次解析后重新产出，子组件天然随 slug 重建，无需额外 key。

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::posts::{get_public_post_by_slug, PublicPostResponse};
use crate::components::comments::section::CommentSection;
use crate::components::post::post_content::PostContent;
use crate::components::post::post_cover::PostCover;
use crate::components::post::post_footer::PostFooter;
use crate::components::post::post_header::PostHeader;
use crate::components::post::post_toc::PostToc;
use crate::components::share_meta::PostShareMeta;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::post_detail_skeleton::PostDetailSkeleton;
use crate::hooks::ssr_resource::{route_data, PageJson, RouteData};
use crate::models::post::{Post, PostFooterData, PostHeaderData, PostStatus};
use crate::pages::not_found::{NotFound, PageLoadFailed};
use crate::utils::error::is_not_found;

/// 文章详情路由，对应 `/post/:slug`。
pub struct PostRoute {
    post: RouteData<PublicPostResponse>,
}

/// 取数函数随分包加载，见 `hooks::ssr_resource`。
#[leptos::lazy]
async fn fetch_post(slug: String) -> PageJson<PublicPostResponse> {
    PageJson::new(get_public_post_by_slug(slug).await)
}

#[lazy_route]
impl LazyRoute for PostRoute {
    fn data() -> Self {
        let params = use_params_map();
        let slug = Memo::new(move |_| params.read().get("slug").unwrap_or_default());
        Self {
            post: route_data(move || slug.get(), fetch_post),
        }
    }

    /// 根据 slug 异步获取文章，渲染文章头部、封面、目录、正文、页脚及评论区；
    /// 文章不存在渲染 404 页，其它加载失败渲染“页面加载失败”。
    fn view(this: Self) -> AnyView {
        let post = this.post;
        view! {
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
                        Ok(PublicPostResponse { post: None }) => view! { <NotFound/> }.into_any(),
                        Err(err) if is_not_found(&err) => view! { <NotFound/> }.into_any(),
                        Err(_) => view! { <PageLoadFailed/> }.into_any(),
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
    // 仅对已发布文章展示评论区域；评论区自带加载骨架屏。
    let comments = (post.status == PostStatus::Published).then(|| {
        let post_id = post.id;
        view! {
            <div class="post-comments">
                <CommentSection post_id=post_id/>
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
