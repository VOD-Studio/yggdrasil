//! 文章详情页面模块。
//!
//! 对应路由 `/post/:slug`。
//!
//! 数据获取：通过 `use_server_future` 调用 `get_post_by_slug` server function，
//! 根据 URL 中的 slug 获取单篇文章详情（含正文 HTML、目录、封面及上下篇导航）。
//!
//! # 反应式取数的关键
//! Dioxus 0.7 的 `use_server_future`（内部即 `use_resource`）只在闭包内读取的
//! **signal** 变化时才会重跑 future——它通过 `ReactiveContext` 追踪闭包执行期间的
//! 订阅。但本组件的 `slug` 是路由宏注入的普通 `String` prop，被 `move` 进闭包后
//! 成了冻结快照，读取它不会建立订阅。因此上/下一篇导航（同一路由变体间的 slug
//! 变化）会复用组件实例、更新 props，却无法触发 future 重跑——表现为「URL 变了
//! 但内容不变，刷新才生效」。
//!
//! 修复：在闭包内通过 `router().current::<Route>()` 读取当前 slug。`current()`
//! 内部调用 `subscribe_to_current_context()`，在 `use_server_future` 的
//! ReactiveContext 中注册订阅；路由变化时订阅触发，future 自动重跑。
//! 在 `wasm32` 目标下，server function 的函数体被替换为向服务端端点发起 HTTP POST 请求的客户端存根；
//! 实际的数据库访问逻辑仅在 `feature = "server"` 启用时运行。

use dioxus::prelude::*;

use crate::api::posts::{get_post_by_slug, SinglePostResponse};
use crate::components::post::post_content::PostContent;
use crate::components::post::post_cover::PostCover;
use crate::components::post::post_footer::PostFooter;
use crate::components::post::post_header::PostHeader;
use crate::components::post::post_toc::PostToc;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::post_detail_skeleton::PostDetailSkeleton;
use crate::router::Route;

/// 文章详情页面组件，对应路由 `/post/:slug`。
///
/// 根据 slug 异步获取文章，渲染文章头部、封面、目录、正文、页脚及评论区；
/// 若文章不存在或加载失败，则展示对应的提示页面。
#[component]
pub fn PostDetail(slug: String) -> Element {
    // 取得路由上下文句柄（不订阅组件层渲染，仅在闭包内按需订阅）。
    // 见模块文档：必须在闭包内读取路由状态才能建立反应式订阅，future 才会在
    // slug 变化（上/下一篇导航）时重跑。`slug` prop 本身是冻结的 String 快照，
    // 不能作为依赖。
    let router = dioxus::router::router();

    let post = use_server_future(move || {
        // 在闭包内读取当前 slug：current() 内部会 subscribe_to_current_context()，
        // 把订阅注册到 use_server_future 的 ReactiveContext，路由变化即重跑。
        let current_slug = match router.current::<Route>() {
            Route::PostDetail { slug } => slug,
            // 组件卸载/路由切走的瞬间可能命中其它变体，退回用 prop 值兜底。
            _ => slug.clone(),
        };
        get_post_by_slug(current_slug)
    })?;

    // 将结果映射为 Ok(post) 或 Err(ServerFnError) 用于错误边界捕获
    let post_data = post.read().as_ref().map(|r| match r {
        Ok(SinglePostResponse { post: Some(post) }) => Ok(post.clone()),
        Ok(SinglePostResponse { post: None }) => Err(ServerFnError::ServerError {
            message: "not_found".to_string(),
            code: 404,
            details: None,
        }),
        Err(e) => Err(e.clone()),
    });

    let post = match post_data {
        Some(Ok(post)) => post,
        Some(Err(err)) => {
            // Bubble the error up to the ErrorBoundary
            return Err(err.into());
        }
        None => {
            return rsx! {
                DelayedSkeleton { PostDetailSkeleton {} }
            };
        }
    };

    rsx! {
        article { class: "post-single animate-page-enter", key: "{post.slug}",
            PostHeader { post: post.clone() }

            // 如果文章设置了封面图，则渲染封面组件。
            if let Some(cover) = &post.cover_image {
                PostCover { src: cover.clone() }
            }

            // 如果文章生成了目录 HTML，则渲染目录组件。
            if let Some(toc) = &post.toc_html {
                PostToc { toc_html: toc.clone() }
            }

            // 用单元素 keyed 列表包裹 PostContent，key 绑定 slug。
            //
            // 为什么不能直接给 PostContent 加 key：Dioxus 的 key diff（diff_keyed_children）
            // 只在「兄弟节点列表」里生效，对单个非列表元素的 key 变化会走 diff_non_keyed
            // 路径、按位置复用，不触发 remount。把组件放进单元素 for 循环并带 key，
            // 才会进入 keyed diff——slug 变化时旧 PostContent 被移除、新的被创建。
            //
            // 为什么需要 remount：上下篇切换时 PostContent 若被复用（仅重渲染），
            // (1) 内部 CodeRunner 用片段索引作 key，两篇文章索引可能相同（如 1/3/5），
            //     keyed diff 按相同 key 复用 CodeRunner 实例——其挂载 use_effect 的
            //     「防重复 init」守卫阻止 CodeMirror 挂载到新的（已替换的）DOM 容器，
            //     表现为翻页后代码块消失；
            // (2) PostContent 自身的 use_effect（__initPostContent 复制按钮 / 灯箱初始化）
            //     也不会重跑，新文章的交互脚本不初始化。
            // 强制 remount 让编辑器与脚本随文章切换全部重新初始化。
            for post_slug in std::iter::once(post.slug.clone()) {
                PostContent {
                    key: "{post_slug}",
                    content_html: post.content_html.clone().unwrap_or_default(),
                }
            }

            PostFooter { post: post.clone() }

            // 仅对已发布文章展示评论区域，使用 SuspenseBoundary 处理加载状态。
            if post.status == crate::models::post::PostStatus::Published {
                div { class: "mt-12 border-t border-[var(--color-paper-border)]/60 pt-8",
                    // 用单元素 keyed 列表包裹 CommentSection，key 绑定 post.id，强制
                    // 上/下一篇切换时 remount（与上方 PostContent 同理，见其注释）。
                    // 否则 PostDetail 组件实例被复用、CommentSection 也被复用，其
                    // use_resource 闭包捕获的 post_id 是冻结快照、refresh_trigger 不随
                    // 导航变化，资源不重启、仍请求旧文章评论——B 文章评论区显示 A 文章
                    // 评论（issue #10）。remount 让 CommentContext 重置、资源以新
                    // post_id 重新拉取。
                    for comment_key in std::iter::once(post.id) {
                        SuspenseBoundary {
                            key: "{comment_key}",
                            fallback: move |_| rsx! {
                                DelayedSkeleton { crate::components::skeletons::comment_skeleton::CommentListSkeleton {} }
                            },
                            crate::components::comments::section::CommentSection { post_id: post.id }
                        }
                    }
                }
            }
        }
    }
}
