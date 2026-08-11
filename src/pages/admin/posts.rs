//! 文章管理页面（全部文章列表，`/admin/posts`）。
//!
//! 本页只承载「全部文章」列表；回收站已拆分为独立路由 `/admin/posts/trash`
//! （见 `posts_trash.rs`），二者与评论管理共同组成侧边栏「内容管理」子菜单
//! （issue #17）。翻页由客户端 signal 驱动（不走路由参数）。
//! 数据加载与写操作仅在 WASM 前端通过 Dioxus server functions 完成。

use dioxus::prelude::*;
use dioxus::router::components::Link;

// 分页数据接口：list_posts 是 server function，两端都生成（wasm 端为 client stub，
// server 端为真实实现），故无需 cfg。实际请求只在 use_paginated 的 wasm 分支发出。
use crate::api::posts::{list_posts, PostListResponse};
// 操作类 server function 仅在 WASM 代码路径调用，SSR 下触发 unused imports，
// 按项目惯例放行。
#[allow(unused_imports)]
use crate::api::posts::{
    delete_post, rebuild_content_html, rebuild_post_content_html, CreatePostResponse, RebuildResult,
};
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::forms::{FormInput, INPUT_INLINE_CLASS};
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::posts_skeleton::PostsSkeleton;
use crate::components::ui::{
    Pagination, StatusBadge, Tooltip, ADMIN_ROW_HOVER, ADMIN_TABLE_CLASS, BTN_OUTLINE, BTN_PRIMARY,
    BTN_TEXT_ACCENT, BTN_TEXT_RED, SPINNER_SVG,
};
use crate::hooks::query::use_paginated;
use crate::models::post::{PostListItem, PostStatus};
use crate::router::Route;

/// 每页展示的文章数量。
const POSTS_PER_PAGE: i32 = 20;

/// 文章管理入口组件：全部文章列表页。
///
/// 纯壳组件：header（标题 + 重建缓存 + 发布文章入口）+ `AllPostsList`。
/// 回收站已拆至独立路由 `/admin/posts/trash`（见 `posts_trash.rs::PostsTrash`）。
#[component]
pub fn Posts() -> Element {
    rsx! {
        div { class: "animate-page-enter w-full max-w-7xl mx-auto space-y-6",
            div { class: "flex flex-col md:flex-row md:items-end justify-between gap-6 pb-6 border-b border-paper-border mb-6",
                div {
                    h1 { class: "text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                        "全部文章"
                    }
                    p { class: "text-base text-[var(--color-paper-secondary)] mt-2",
                        "所有文章及草稿"
                    }
                }
                div { class: "flex items-center gap-3",
                    RebuildCacheBar {}
                    Link { class: "{BTN_PRIMARY}", to: Route::Write {}, "发布文章" }
                }
            }
            AllPostsList {}
        }
    }
}

/// 全部文章列表 tab：分页列表、删除单篇、重建 content_html 缓存。
///
/// 翻页用客户端 signal 驱动（`current_page` signal + `use_paginated` 的闭包内读取
/// 建立依赖，页码变化自动重载），不走路由。删除/重建逻辑与旧实现一致。
#[component]
fn AllPostsList() -> Element {
    let mut current_page = use_signal(|| 1);
    // 搜索输入框实时绑定的文本（每键即更新，但不触发请求）。
    let mut search_input = use_signal(String::new);
    // 已提交的搜索词：空串表示不搜索。仅在此值变化时才重新请求，避免逐键打 DB。
    let mut search_query = use_signal(String::new);

    // 分页列表加载（loading / posts / total / error）由 use_paginated 统一管理。
    // page 闭包内同时读取 current_page 与 search_query 建立响应式依赖：
    // 翻页、或提交新搜索词（即便停留在第 1 页）都会自动重新请求。
    // fetch 闭包在发起请求时读取 search_query 的当前值传给后端按标题过滤。
    let paginated = use_paginated(
        move || {
            let _ = search_query();
            current_page.with(|p| *p)
        },
        POSTS_PER_PAGE,
        move |p, pp| {
            let q = search_query();
            async move {
                list_posts(p, pp, if q.is_empty() { None } else { Some(q) })
                    .await
                    .map(|PostListResponse { posts, total }| (posts, total))
                    .map_err(|e| e.to_string())
            }
        },
    );
    let mut posts = paginated.items;
    let mut total = paginated.total;
    let loading = paginated.loading;
    let error = paginated.error;

    // 删除中 / 重建中文章 ID 集合：均由本组件持有（业务逻辑不归 hook 管）。
    // 改为非乐观删除后行会保留至请求完成，可并发点多个删除，故用 HashSet
    // 与 rebuilding 同形，按行通过 contains 判断 loading 态。
    let mut deleting = use_signal(std::collections::HashSet::<i32>::new);
    // 重建中文章 ID 集合：支持多篇文章并发重建（行不会随点击消失，单值会被后点
    // 的覆盖先点的，故用 HashSet），按行通过 contains 判断 loading 态。
    let mut rebuilding = use_signal(std::collections::HashSet::<i32>::new);
    let get_posts = move || -> Vec<PostListItem> { posts() };
    // 是否处于搜索结果视图（用于区分空状态文案 / 隐藏「写文章」入口）。
    let is_searching = move || !search_query().is_empty();
    // 提交搜索：写入 search_query 并回到第 1 页（搜索结果从首页开始分页）。
    let mut submit_search = move || {
        let q = search_input().trim().to_string();
        search_query.set(q);
        current_page.set(1);
    };

    rsx! {
        // 搜索条：按标题过滤文章（仅管理后台用，覆盖草稿）。
        div { class: "flex gap-2 mb-4",
            FormInput {
                r#type: "search",
                placeholder: "搜索文章标题...",
                value: search_input(),
                class: INPUT_INLINE_CLASS,
                oninput: move |v: String| search_input.set(v),
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Enter {
                        submit_search();
                    }
                },
            }
            button { class: "{BTN_PRIMARY}", onclick: move |_| submit_search(), "搜索" }
            if is_searching() {
                button {
                    class: "{BTN_OUTLINE}",
                    onclick: move |_| {
                        search_input.set(String::new());
                        search_query.set(String::new());
                        current_page.set(1);
                    },
                    "清除"
                }
            }
        }

        if error().is_some() {
            EmptyState {
                title: "加载失败",
                description: "获取文章列表时发生错误，请稍后重试。",
            }
        } else if loading() && posts().is_empty() {
            DelayedSkeleton { PostsSkeleton {} }
        } else if posts().is_empty() {
            if is_searching() {
                EmptyState {
                    title: "未找到匹配的文章",
                    description: "换个标题关键词再试一次。",
                }
            } else {
                EmptyState {
                    title: "暂无文章",
                    description: "还没有创建任何文章，开始写下你的第一篇文字吧。",
                    action: EmptyStateAction {
                        label: "写文章".to_string(),
                        to: Route::Write {},
                    },
                }
            }
        } else {
            div { class: "{ADMIN_TABLE_CLASS}",
                table { class: "w-full text-sm",
                    thead {
                        tr { class: "border-b border-paper-border text-left text-paper-secondary",
                            th { class: "px-4 py-3 font-medium", "标题" }
                            th { class: "px-4 py-3 font-medium w-24 text-center whitespace-nowrap",
                                "状态"
                            }
                            th { class: "px-4 py-3 font-medium w-32 whitespace-nowrap",
                                "日期"
                            }
                            th { class: "px-4 py-3 font-medium w-44 text-right whitespace-nowrap",
                                "操作"
                            }
                        }
                    }
                    tbody {
                        for (idx, post) in get_posts().iter().enumerate() {
                            PostRow {
                                key: "{post.id}",
                                post: post.clone(),
                                deleting: deleting().contains(&post.id),
                                rebuilding: rebuilding().contains(&post.id),
                                stagger_index: idx as u32,
                                on_delete: move |id| {
                                    deleting.write().insert(id);
                                    spawn(async move {
                                        match delete_post(id).await {
                                            Ok(CreatePostResponse { success: true, .. }) => {
                                                posts.with_mut(|list| list.retain(|p| p.id != id));
                                                total.with_mut(|t| *t = t.saturating_sub(1));
                                            }
                                            Ok(CreatePostResponse { success: false, message: _message, .. }) => {
                                                #[cfg(target_arch = "wasm32")]
                                                web_sys::window().map(|w| w.alert_with_message(&_message).ok());
                                            }
                                            Err(_e) => {
                                                #[cfg(target_arch = "wasm32")]
                                                web_sys::window().map(|w| w.alert_with_message("删除失败").ok());
                                            }
                                        }
                                        deleting.write().remove(&id);
                                    });
                                },
                                on_rebuild: move |id| {
                                    rebuilding.write().insert(id);
                                    spawn(async move {
                                        let _ = rebuild_post_content_html(id).await;
                                        rebuilding.write().remove(&id);
                                    });
                                },
                            }
                        }
                    }
                }
            }
            Pagination {
                variant: "admin",
                current_page: current_page(),
                total: total(),
                per_page: POSTS_PER_PAGE,
                unit: "篇",
                on_prev: {
                    let mut page = current_page;
                    move |_| {
                        page.with_mut(|p| *p = (*p - 1).max(1));
                    }
                },
                on_next: {
                    let mut page = current_page;
                    move |_| {
                        page.with_mut(|p| *p += 1);
                    }
                },
                on_jump: {
                    let mut page = current_page;
                    move |p: i32| {
                        page.set(p);
                    }
                },
            }
        }
    }
}

/// 重建内容缓存工具条子组件。
///
/// 封装「重建内容 / 重建全部」两个按钮及其 `do_rebuild` 异步闭包。状态
/// (`rebuilding` / `rebuild_result`) 由本组件内部持有（从 `PostsPage` 下沉至此，
/// 因合并后仅 All tab 需要，无需跨层传递）。
///
/// 从 `AllPostsList` 抽取以降低 god component 复杂度（见 dioxus-render-purity skill）。
#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
fn RebuildCacheBar() -> Element {
    let mut rebuilding = use_signal(|| false);
    let mut rebuild_result = use_signal(|| Option::<String>::None);

    // 重建文章渲染缓存：rebuild_all 为 false 时仅重建 content_html 为空的文章，
    // 为 true 时重建所有文章（用于语法/渲染逻辑升级后批量刷新已有内容）。
    let mut do_rebuild = move |rebuild_all: bool| {
        rebuilding.set(true);
        rebuild_result.set(None);
        spawn(async move {
            match rebuild_content_html(rebuild_all).await {
                Ok(RebuildResult {
                    rebuilt,
                    failed,
                    errors,
                }) => {
                    if failed > 0 {
                        let mut msg = format!("已重建 {rebuilt} 篇，失败 {failed} 篇");
                        if let Some(first) = errors.first() {
                            msg.push_str(&format!("\n{first}"));
                        }
                        rebuild_result.set(Some(msg));
                    } else {
                        rebuild_result.set(Some(format!("已重建 {rebuilt} 篇文章")));
                    }
                }
                Err(e) => {
                    rebuild_result.set(Some(format!("失败: {e}")));
                }
            }
            rebuilding.set(false);
        });
    };

    rsx! {
        // 消息绝对定位到按钮行下方，脱离文档流：出现/消失都不撑高祖先容器，
        // 避免 header 的 md:items-end 把固定底边转化为按钮上移（"按钮被顶上去" bug）。
        // 自持 rebuilding / rebuild_result state，与父组件零耦合。
        div { class: "relative flex items-center gap-3",
            div { class: "flex items-center gap-3",
                Tooltip {
                    tip: "重建 content_html 为空的文章渲染缓存".to_string(),
                    placement: "bottom",
                    button {
                        class: if rebuilding() { "relative px-4 py-2 rounded-full text-sm font-medium cursor-not-allowed text-paper-secondary border border-paper-border" } else { BTN_OUTLINE },
                        disabled: rebuilding(),
                        onclick: move |_| do_rebuild(false),
                        span { class: if rebuilding() { "opacity-40" } else { "" }, "重建内容" }
                        if rebuilding() {
                            span {
                                class: "absolute inset-0 flex items-center justify-center",
                                dangerous_inner_html: SPINNER_SVG,
                            }
                        }
                    }
                }
                Tooltip {
                    tip: "重建所有文章的渲染缓存（含已有内容）".to_string(),
                    placement: "bottom",
                    button {
                        class: if rebuilding() { "relative px-4 py-2 rounded-full text-sm font-medium cursor-not-allowed text-paper-secondary border border-paper-border" } else { BTN_OUTLINE },
                        disabled: rebuilding(),
                        onclick: move |_| do_rebuild(true),
                        span { class: if rebuilding() { "opacity-40" } else { "" }, "重建全部" }
                        if rebuilding() {
                            span {
                                class: "absolute inset-0 flex items-center justify-center",
                                dangerous_inner_html: SPINNER_SVG,
                            }
                        }
                    }
                }
            }
            // 重建结果消息：绝对定位到按钮行正下方，脱离文档流，不影响布局高度。
            if let Some(msg) = rebuild_result() {
                div { class: "absolute top-full right-0 mt-1 text-xs text-paper-secondary whitespace-pre-line",
                    "{msg}"
                }
            }
        }
    }
}

/// 文章表格行组件，展示单篇文章的标题、状态、日期与操作按钮。
#[component]
fn PostRow(
    post: PostListItem,
    deleting: bool,
    rebuilding: bool,
    stagger_index: u32,
    on_delete: EventHandler<i32>,
    on_rebuild: EventHandler<i32>,
) -> Element {
    let date_str = post.formatted_date();
    // 草稿标题跳预览（/admin/preview/<slug>），已发布标题跳公开详情页。
    // 草稿在 /post/<slug> 会被 status 过滤 404，故必须分流避免死链。
    let title_dest = if post.status == PostStatus::Draft {
        Route::PostPreview {
            slug: post.slug.clone(),
        }
    } else {
        Route::PostDetail {
            slug: post.slug.clone(),
        }
    };

    rsx! {
        tr { class: "animate-row-enter {ADMIN_ROW_HOVER}",
            style: "animation-delay: {stagger_index * 35}ms",
            td { class: "px-4 py-3",
                Link {
                    class: "text-paper-primary hover:text-paper-accent transition-colors cursor-pointer",
                    to: title_dest,
                    "{post.title}"
                }
            }
            td { class: "px-4 py-3 text-center whitespace-nowrap",
                StatusBadge {
                    color_class: post.status_badge_class(),
                    label: post.status_label().to_string(),
                }
            }
            td { class: "px-4 py-3 text-paper-secondary whitespace-nowrap", "{date_str}" }
            td { class: "px-4 py-3 text-right whitespace-nowrap",
                div { class: "flex justify-end items-center gap-3",
                    Link {
                        class: "text-xs text-paper-secondary hover:text-paper-primary transition-colors cursor-pointer",
                        to: Route::WriteEdit { id: post.id },
                        "编辑"
                    }
                    Tooltip {
                        tip: "重新渲染这篇文章的 HTML".to_string(),
                        align: "end",
                        button {
                            class: if rebuilding { "relative inline-flex items-center text-xs text-paper-accent cursor-not-allowed" } else { BTN_TEXT_ACCENT },
                            disabled: rebuilding,
                            onclick: move |_| on_rebuild.call(post.id),
                            span { class: if rebuilding { "opacity-40" } else { "" }, "重建" }
                            if rebuilding {
                                span {
                                    class: "absolute inset-0 flex items-center justify-center",
                                    dangerous_inner_html: SPINNER_SVG,
                                }
                            }
                        }
                    }
                    button {
                        class: if deleting { "relative inline-flex items-center text-xs text-paper-secondary cursor-not-allowed" } else { BTN_TEXT_RED },
                        disabled: deleting,
                        onclick: move |_| on_delete.call(post.id),
                        span { class: if deleting { "opacity-40" } else { "" }, "删除" }
                        if deleting {
                            span {
                                class: "absolute inset-0 flex items-center justify-center",
                                dangerous_inner_html: SPINNER_SVG,
                            }
                        }
                    }
                }
            }
        }
    }
}
