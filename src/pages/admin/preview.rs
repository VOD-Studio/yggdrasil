//! 草稿/文章预览页面模块（管理员只读）。
//!
//! 对应路由 `/admin/preview/:slug`。
//!
//! 与公开详情页 [`crate::pages::post_detail::PostDetail`] 的关键差异：
//! - 取数走 admin-only [`crate::api::posts::get_post_preview`]，不做 `status = 'published'` 过滤，
//!   故草稿可预览；草稿绝不进公开缓存（`cache::get/set_post_by_slug`）。
//! - 顶部渲染预览横幅（状态徽章 + 继续编辑 / 返回列表），便于从预览直达编辑。
//! - 错误/未命中**就地渲染**（admin nest 内无 ErrorBoundary），不向错误边界上抛。
//! - 不渲染评论区（草稿无评论）。
//!
//! # 为何预览放在 `/admin/*` 下
//! Dioxus IncrementalRenderer 的 SSR 磁盘缓存按 URL（`path_and_query()`）落盘，
//! 不区分用户身份。若管理员在 `/post/<draft-slug>` 看到草稿，渲染出的草稿 HTML
//! 会被缓存到 `static/post/<draft-slug>/`，匿名访客再请求同一 URL 即命中缓存、
//! 直接拿到草稿正文——草稿属保密内容，泄漏不可接受。`/admin/preview/<slug>` 受
//! `admin_guard` 中间件保护：匿名在 SSR 渲染前就被 302 跳走，不产生可泄漏的缓存。
//!
//! # 反应式取数
//! 与 `post_detail.rs` 同一手法：`slug` prop 是路由宏注入的冻结 String 快照，
//! 闭包内需通过 `router().current::<Route>()` 读取当前 slug 才能在路由变化时重跑。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::api::posts::{get_post_preview, SinglePostResponse};
use crate::components::post::post_content::PostContent;
use crate::components::post::post_cover::PostCover;
use crate::components::post::post_footer::PostFooter;
use crate::components::post::post_header::PostHeader;
use crate::components::post::post_toc::PostToc;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::post_preview_skeleton::PostPreviewSkeleton;
use crate::components::ui::{BTN_OUTLINE, BTN_PRIMARY};
use crate::router::Route;

/// 草稿/文章预览页面组件（管理员只读），对应路由 `/admin/preview/:slug`。
///
/// 渲染与公开详情页一致的正文（头部、封面、目录、正文、页脚），顶部加预览横幅。
/// 加载中显示骨架屏；文章不存在或加载失败时就地渲染提示，不向上抛错。
#[component]
pub fn PostPreview(slug: String) -> Element {
    // 取得路由上下文句柄（不订阅组件层渲染，仅在闭包内按需订阅）。
    // 见模块文档：必须在闭包内读取路由状态才能建立反应式订阅，future 才会在
    // slug 变化时重跑。`slug` prop 本身是冻结的 String 快照，不能作为依赖。
    let router = dioxus::router::router();

    let post = use_server_future(move || {
        // 在闭包内读取当前 slug：current() 内部会 subscribe_to_current_context()，
        // 把订阅注册到 use_server_future 的 ReactiveContext，路由变化即重跑。
        let current_slug = match router.current::<Route>() {
            Route::PostPreview { slug } => slug,
            // 组件卸载/路由切走的瞬间可能命中其它变体，退回用 prop 值兜底。
            _ => slug.clone(),
        };
        get_post_preview(current_slug)
    })?;

    // admin nest 内无 ErrorBoundary：错误/未命中就地渲染，不向上抛。
    // None（pending）→ 骨架屏；Some(Err) / Ok(post=None) → 居中提示。
    let post = match post.read().as_ref() {
        None => {
            return rsx! {
                DelayedSkeleton { PostPreviewSkeleton {} }
            };
        }
        Some(Err(_)) | Some(Ok(SinglePostResponse { post: None })) => {
            return rsx! {
                div { class: "flex flex-col items-center justify-center text-center py-20 px-4 animate-page-enter",
                    p { class: "text-sm text-paper-secondary",
                        "未找到该文章（可能已被删除）。"
                    }
                    Link { class: "mt-6 {BTN_OUTLINE}", to: Route::Posts {}, "返回文章列表" }
                }
            };
        }
        Some(Ok(SinglePostResponse { post: Some(post) })) => post.clone(),
    };

    rsx! {
        article { class: "post-single animate-page-enter", key: "{post.slug}",
            // 预览横幅：状态徽章 + 继续编辑 / 返回列表。
            div { class: "flex flex-wrap items-center justify-between gap-3 mb-6 p-3 rounded-2xl bg-[var(--color-paper-entry)] border border-[var(--color-paper-border)]",
                div { class: "flex items-center gap-2 text-sm text-paper-secondary",
                    span { "预览模式" }
                    span { class: "px-2 py-0.5 rounded-full text-xs font-medium {post.status.badge_class()}",
                        "{post.status.label()}"
                    }
                }
                div { class: "flex items-center gap-2",
                    Link {
                        class: "{BTN_PRIMARY}",
                        to: Route::WriteEdit { id: post.id },
                        "继续编辑"
                    }
                    Link { class: "{BTN_OUTLINE}", to: Route::Posts {}, "返回列表" }
                }
            }

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
            // 与 post_detail.rs 同理：slug 变化时强制 remount，让正文内的脚本/
            // 编辑器随文章切换重新初始化（详见 post_detail.rs:95-115 注释）。
            for post_slug in std::iter::once(post.slug.clone()) {
                PostContent {
                    key: "{post_slug}",
                    content_html: post.content_html.clone().unwrap_or_default(),
                }
            }

            PostFooter { post: post.clone() }
        }
    }
}
