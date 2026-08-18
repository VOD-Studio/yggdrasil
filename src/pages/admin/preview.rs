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
//! # 取数不挂起（与 post_detail.rs 的关键差异，勿"改回去"）
//! 本页**不用** `use_server_future(...)?`（挂起式取数），而是 `use_signal` +
//! `spawn` 的非挂起模式（与仪表盘等所有 admin 页一致）。原因：dioxus 0.7.10
//! 存在 vdom 状态腐蚀 bug——当挂起中的 SuspenseBoundary 连同其后台子树一起
//! 卸载（本页从 `/admin/preview/*` 客户端导航回前台时正是此路径：
//! AdminLayout 的 SuspenseBoundary 在 PostPreview future 尚未 resolve 时被整体
//! 替换），dioxus-core 会对同一元素双重回收（console 报 `cannot reclaim
//! ElementId(N)`），进而令 interpreter 的节点表失步（`RawInterpreter.run` 崩溃
//! `Cannot read properties of undefined (reading 'listening')`），此后所有点击
//! 事件处理失效（表现为"列表点不动"）。前台页面虽同样挂起，但前台内部导航
//! 不卸载 FrontendLayout 的 boundary，触发不了该路径；admin 侧只有本页挂起。
//! 上游 master 的 suspense 卸载逻辑与 0.7.10 相同（未修复）；若日后升级
//! dioxus 修复了此 bug，可考虑改回挂起式取数以恢复 SSR 数据内嵌。

#[cfg(target_arch = "wasm32")]
use crate::api::posts::get_post_preview;
#[cfg(target_arch = "wasm32")]
use crate::api::posts::SinglePostResponse;
use crate::components::post::post_content::PostContent;
use crate::components::post::post_cover::PostCover;
use crate::components::post::post_footer::PostFooter;
use crate::components::post::post_header::PostHeader;
use crate::components::post::post_toc::PostToc;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::post_preview_skeleton::PostPreviewSkeleton;
use crate::components::ui::{BTN_OUTLINE, BTN_PRIMARY};
use crate::models::post::Post;
use crate::router::Route;
use dioxus::prelude::*;
use dioxus::router::components::Link;

/// 草稿/文章预览页面组件（管理员只读），对应路由 `/admin/preview/:slug`。
///
/// 渲染与公开详情页一致的正文（头部、封面、目录、正文、页脚），顶部加预览横幅。
/// 加载中显示骨架屏；文章不存在或加载失败时就地渲染提示，不向上抛错。
#[component]
pub fn PostPreview(slug: String) -> Element {
    let router = dioxus::router::router();

    // 非挂起取数（见模块文档「取数不挂起」一节）：None=加载中；
    // Some(None)=未找到/失败；Some(Some(post))=成功。错误与未命中同视图。
    #[allow(unused_mut)]
    let mut post = use_signal(|| None::<Option<Post>>);

    // 在 effect 内读取当前路由 slug 建立订阅：slug 变化（同为 PostPreview
    // 变体复用组件实例）时重新拉取，并先回骨架屏。
    use_effect(move || {
        let current_slug = match router.current::<Route>() {
            Route::PostPreview { slug } => slug,
            // 组件卸载/路由切走的瞬间可能命中其它变体，退回用 prop 值兜底。
            _ => slug.clone(),
        };
        // native 构建下 spawn 被编译掉；显式引用避免未用告警（dashboard 同款语义）。
        #[cfg(not(target_arch = "wasm32"))]
        let _ = &current_slug;
        // SSR 不取数（与仪表盘等 admin 页一致）：直接 URL 访问首屏为骨架屏，
        // 客户端水合后再拉取。
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            let resp = get_post_preview(current_slug.clone()).await;
            // 竞态守卫：仅当结果返回时仍停留在本 slug 才写回，
            // 避免快速切换时慢的旧响应覆盖新文章。
            let still_here = matches!(router.current::<Route>(),
                Route::PostPreview { slug: s } if s == current_slug);
            if still_here {
                post.set(Some(resp.ok().and_then(|SinglePostResponse { post }| post)));
            }
        });
    });

    // admin nest 内无 ErrorBoundary：错误/未命中就地渲染，不向上抛。
    // None（pending）→ 骨架屏；Some(None)（Err / post=None）→ 居中提示。
    let post = match post.read().as_ref() {
        None => {
            return rsx! {
                DelayedSkeleton { PostPreviewSkeleton {} }
            };
        }
        Some(None) => {
            return rsx! {
                div { class: "flex flex-col items-center justify-center text-center py-20 px-4 animate-page-enter",
                    p { class: "text-sm text-paper-secondary",
                        "未找到该文章（可能已被删除）。"
                    }
                    Link { class: "mt-6 {BTN_OUTLINE}", to: Route::Posts {}, "返回文章列表" }
                }
            };
        }
        Some(Some(post)) => post.clone(),
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

            PostHeader { post: post.clone(), full_reload: true }

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
            PostFooter { post: post.clone(), full_reload: true }
        }
    }
}
