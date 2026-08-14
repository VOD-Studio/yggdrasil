//! 管理后台仪表盘页面。
//!
//! 采用高密度工业风设计的管理面板，突出核心数据指标与最新的工作流状态。
//! 数据仅在 WASM 前端通过 Dioxus server functions 异步加载。

use dioxus::prelude::*;
use dioxus::router::components::Link;

#[cfg(target_arch = "wasm32")]
use crate::api::comments::get_pending_count;
#[cfg(target_arch = "wasm32")]
use crate::api::posts::{get_post_stats, list_posts};
#[cfg(target_arch = "wasm32")]
use crate::api::posts::{PostListResponse, PostStatsResponse};
use crate::components::empty_state::{EmptyState, EmptyStateAction};
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::{ADMIN_CARD_CLASS, ADMIN_TABLE_CLASS, BTN_PRIMARY, BTN_SECONDARY};
use crate::models::post::{PostListItem, PostStats};
use crate::router::Route;

#[component]
#[allow(unused_mut)]
pub fn Admin() -> Element {
    let mut stats = use_signal(|| None::<PostStats>);
    let mut recent_posts = use_signal(|| None::<Vec<PostListItem>>);
    let mut pending_count = use_signal(|| None::<i64>);
    let mut loaded = use_signal(|| false);

    use_effect(move || {
        if !loaded() {
            loaded.set(true);
            #[cfg(target_arch = "wasm32")]
            {
                spawn(async move {
                    if let Ok(PostStatsResponse { stats: s }) = get_post_stats().await {
                        stats.set(Some(s));
                    }
                });
                spawn(async move {
                    if let Ok(PostListResponse { posts, total: _ }) = list_posts(1, 5, None).await {
                        recent_posts.set(Some(posts));
                    }
                });
                spawn(async move {
                    if let Ok(resp) = get_pending_count().await {
                        pending_count.set(Some(resp.count));
                    }
                });
            }
        }
    });

    // 待审卡片进场类:数据未就绪(骨架屏)时为空,就绪后补挂以触发一次入场动画。
    let pending_enter_class = if pending_count().is_some() {
        "animate-page-enter"
    } else {
        ""
    };

    rsx! {
        div { class: "animate-page-enter w-full max-w-7xl mx-auto space-y-8",
            // 顶部标题和全局操作栏
            div { class: "flex flex-col md:flex-row md:items-end justify-between gap-6 pb-8 border-b border-[var(--color-paper-border)]/50",
                div {
                    h1 { class: "animate-row-enter text-4xl font-extrabold tracking-tight text-[var(--color-paper-primary)]",
                        "仪表盘"
                    }
                    p {
                        class: "animate-row-enter text-base text-[var(--color-paper-secondary)] mt-2",
                        style: "animation-delay: 60ms",
                        "数据概览与近期活动"
                    }
                }
                div {
                    class: "animate-row-enter flex items-center gap-3",
                    style: "animation-delay: 120ms",
                    Link { class: "{BTN_SECONDARY}", to: Route::Posts {}, "全部文章" }
                    Link { class: "{BTN_PRIMARY}", to: Route::Write {}, "发布文章" }
                }
            }

            // 数据指标 Bento Grid
            div { class: "grid grid-cols-1 md:grid-cols-4 gap-4",
                match stats() {
                    Some(s) => {
                        rsx! {
                            StatCard {
                                value: s.total,
                                label: "总文章数".to_string(),
                                trend: Some(format!("近30天 +{}", s.recent_30d)),
                                delay_ms: 0,
                            }
                            StatCard {
                                value: s.published,
                                label: "已发布".to_string(),
                                trend: None,
                                delay_ms: 120,
                            }
                            StatCard {
                                value: s.drafts,
                                label: "草稿".to_string(),
                                trend: None,
                                delay_ms: 240,
                            }
                        }
                    }
                    None => {
                        rsx! {
                            for _ in 0..3 {
                                div { class: "{ADMIN_CARD_CLASS} p-6 flex flex-col justify-between h-32 animate-pulse",
                                    SkeletonBox { class: "h-3 w-20 rounded" }
                                    SkeletonBox { class: "h-10 w-16 rounded mt-4" }
                                }
                            }
                        }
                    }
                }

                // 评论待办卡片 (独立色块突出)
                // 数据就绪后补挂 animate-page-enter:类名变更触发 CSS 动画从 0% 播放,
                // 骨架屏阶段不播(避免动画被骨架屏截断,见 yggdrasil-ui-design-taste 规范)。
                Link {
                    class: "block {ADMIN_CARD_CLASS} p-8 bg-[var(--color-paper-entry)] hover:bg-[var(--color-paper-border)]/20 transition-all h-36 flex flex-col justify-between group hover:-translate-y-1 hover:shadow-md duration-300 {pending_enter_class}",
                    style: "animation-delay: 360ms; animation-duration: 600ms",
                    to: Route::AdminComments {},
                    match pending_count() {
                        Some(count) => {
                            let (color_class, text_class) = if count > 0 {
                                ("text-amber-500", "text-amber-500")
                            } else {
                                (
                                    "text-[var(--color-paper-secondary)]",
                                    "text-[var(--color-paper-primary)]",
                                )
                            };
                            rsx! {
                                div { class: "text-sm font-medium {color_class}", "待审评论" }
                                div { class: "flex items-baseline justify-between mt-4",
                                    CountUp {
                                        target: count,
                                        class: format!("text-4xl font-light tracking-tight {text_class}"),
                                    }
                                    div { class: "text-xs font-medium text-[var(--color-paper-secondary)] group-hover:text-[var(--color-paper-primary)] transition-colors",
                                        "去审核 →"
                                    }
                                }
                            }
                        }
                        None => {
                            rsx! {
                                SkeletonBox { class: "h-3 w-24 rounded" }
                                SkeletonBox { class: "h-10 w-16 rounded mt-4" }
                            }
                        }
                    }
                }
            }

            // 最近文章列表
            div { class: "mt-12",
                div {
                    class: "animate-row-enter flex items-center justify-between mb-6",
                    style: "animation-delay: 200ms",
                    h2 { class: "text-xl font-bold text-[var(--color-paper-primary)] tracking-tight",
                        "近期文章"
                    }
                }
                match recent_posts() {
                    // 空库 / 无文章：展示空状态占位（与 posts.rs 列表页一致）。
                    // 放在 ADMIN_TABLE_CLASS 容器之外，避免 overflow-hidden 裁掉插画的 py-20 内边距。
                    Some(posts) if posts.is_empty() => {
                        rsx! {
                            EmptyState {
                                title: "暂无文章",
                                description: "还没有创建任何文章，开始写下你的第一篇文字吧。",
                                action: Some(EmptyStateAction {
                                    label: "写文章".to_string(),
                                    to: Route::Write {},
                                }),
                            }
                        }
                    }
                    Some(posts) => {
                        rsx! {
                            div { class: "{ADMIN_TABLE_CLASS}",
                                div { class: "divide-y divide-paper-border",
                                    for (i, post) in posts.iter().take(5).enumerate() {
                                        RecentPostItem {
                                            key: "{post.id}",
                                            post: post.clone(),
                                            delay_ms: (i as i32) * 80,
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // 加载中：骨架屏。
                    None => {
                        rsx! {
                            div { class: "{ADMIN_TABLE_CLASS}",
                                div { class: "divide-y divide-paper-border animate-pulse",
                                    for _ in 0..5 {
                                        div { class: "flex justify-between items-center px-6 py-4",
                                            SkeletonBox { class: "h-4 w-[40%] rounded" }
                                            SkeletonBox { class: "h-3 w-24 rounded" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatCard(value: i64, label: String, trend: Option<String>, delay_ms: i32) -> Element {
    rsx! {
        div {
            class: "{ADMIN_CARD_CLASS} p-8 flex flex-col justify-between h-36 relative group hover:-translate-y-1 hover:shadow-md transition-all duration-300 animate-page-enter",
            style: "animation-delay: {delay_ms}ms; animation-duration: 600ms",
            div { class: "flex justify-between items-start",
                div { class: "text-sm font-medium text-[var(--color-paper-secondary)]",
                    "{label}"
                }
                div { class: "text-xs px-2 py-0.5 rounded-full border border-[var(--color-paper-border)] text-[var(--color-paper-tertiary)]",
                    "{trend}"
                }
            }
            CountUp {
                target: value,
                class: "text-4xl font-light tracking-tight text-[var(--color-paper-primary)] mt-4"
                    .to_string(),
            }
        }
    }
}

/// 数字滚动组件:值从 0 以 easeOutQuint 缓动递增到 `target`(约 900ms)。
///
/// 命中 `prefers-reduced-motion` 时直接显示终值。动画在 `use_effect` 内驱动,
/// 渲染体保持纯净(见 dioxus-render-purity 规范);数据仅 WASM 端加载,SSR 不挂载本组件。
#[component]
fn CountUp(target: i64, class: String) -> Element {
    let mut display = use_signal(|| 0i64);

    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        spawn(async move {
            let reduced = web_sys::window()
                .and_then(|w| {
                    w.match_media("(prefers-reduced-motion: reduce)")
                        .ok()
                        .flatten()
                })
                .map(|m| m.matches())
                .unwrap_or(false);
            if reduced || target <= 0 {
                display.set(target);
                return;
            }
            const DURATION_MS: i64 = 900;
            let start = crate::utils::time::now_millis();
            loop {
                crate::utils::time::sleep_ms(16).await;
                let elapsed = crate::utils::time::now_millis() - start;
                if elapsed >= DURATION_MS {
                    display.set(target);
                    break;
                }
                let t = elapsed as f64 / DURATION_MS as f64;
                // easeOutQuint,与 CSS 侧 cubic-bezier(0.22, 1, 0.36, 1) 同族。
                let eased = 1.0 - (1.0 - t).powi(5);
                display.set((target as f64 * eased).round() as i64);
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        {
            display.set(target);
        }
    });

    rsx! {
        div { class: "{class}", "{display}" }
    }
}

#[component]
fn RecentPostItem(post: PostListItem, delay_ms: i32) -> Element {
    let date_str = post.formatted_date();
    let status_label = post.status_label();
    let status_class = post.status_class();

    rsx! {
        div {
            class: "flex flex-col sm:flex-row sm:justify-between sm:items-center px-8 py-5 hover:bg-[var(--color-paper-accent-soft)] transition-colors cursor-pointer group animate-row-enter",
            style: "animation-delay: {delay_ms}ms",
            div { class: "flex items-center gap-6",
                span { class: "text-xs font-mono text-[var(--color-paper-tertiary)] w-12 hidden sm:block",
                    "#{post.id:04}"
                }
                span { class: "text-base font-semibold text-[var(--color-paper-primary)] group-hover:text-[var(--color-paper-accent)] transition-colors",
                    "{post.title}"
                }
                span { class: "text-xs px-3 py-1 font-medium rounded-full {status_class}",
                    "{status_label}"
                }
            }
            span { class: "text-sm text-[var(--color-paper-secondary)] mt-2 sm:mt-0",
                "{date_str}"
            }
        }
    }
}
