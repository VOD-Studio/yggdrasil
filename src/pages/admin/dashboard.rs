//! 管理后台仪表盘页面。
//!
//! 单页监控 + 可行动导向：统计卡带（真实增量徽章 + 30 日 sparkline）→ 待审
//! 评论行动卡 → 近期文章活动流。动效预算遵循 NN/g/Atlassian 共识（hover
//! ≤200ms、入场 ≤400ms、数字滚动 ≤500ms）；接口失败如实呈现「加载失败 +
//! 重试」，不回退成 0 也不骨架永转（设计依据见
//! `docs/research/2026-admin-dashboard-design.md`）。
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
use crate::components::ui::{
    ADMIN_CARD_CLASS, ADMIN_TABLE_CLASS, BTN_OUTLINE, BTN_PRIMARY, BTN_SECONDARY,
};
use crate::models::post::{PostListItem, PostStats};
use crate::router::Route;

#[component]
#[allow(unused_mut)]
pub fn Admin() -> Element {
    let mut stats = use_signal(|| None::<PostStats>);
    let mut recent_posts = use_signal(|| None::<Vec<PostListItem>>);
    let mut pending_count = use_signal(|| None::<i64>);
    // 三路加载各自的失败标志：接口失败如实呈现「加载失败 + 重试」，
    // 不回退成 0（0 是真实数据，失败不是 0），也不让骨架屏永转。
    let mut stats_failed = use_signal(|| false);
    let mut posts_failed = use_signal(|| false);
    let mut pending_failed = use_signal(|| false);
    let mut loaded = use_signal(|| false);

    use_effect(move || {
        if !loaded() {
            loaded.set(true);
            #[cfg(target_arch = "wasm32")]
            {
                spawn(async move {
                    match get_post_stats().await {
                        Ok(PostStatsResponse { stats: s }) => stats.set(Some(s)),
                        Err(_) => stats_failed.set(true),
                    }
                });
                spawn(async move {
                    match list_posts(1, 5, None).await {
                        Ok(PostListResponse { posts, total: _ }) => recent_posts.set(Some(posts)),
                        Err(_) => posts_failed.set(true),
                    }
                });
                spawn(async move {
                    match get_pending_count().await {
                        Ok(resp) => pending_count.set(Some(resp.count)),
                        Err(_) => pending_failed.set(true),
                    }
                });
            }
        }
    });

    // 重试：清失败标志并翻转 loaded，触发 use_effect 重新发起全部加载。
    let mut retry = move |_| {
        stats_failed.set(false);
        posts_failed.set(false);
        pending_failed.set(false);
        loaded.set(false);
    };

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
                match (stats(), stats_failed()) {
                    (Some(s), _) => {
                        rsx! {
                            StatCard {
                                value: s.total,
                                label: "总文章数".to_string(),
                                trend: Some(TrendBadge {
                                    // 涨跌颜色 + 箭头图标双编码（WCAG 2.2 SC 1.4.1：
                                    // 颜色不得作为唯一信息载体）；+0 走中性样式不带箭头。
                                    text: if s.recent_30d > 0 {
                                        format!("↑ 近30天 +{}", s.recent_30d)
                                    } else {
                                        "近30天 +0".to_string()
                                    },
                                    positive: s.recent_30d > 0,
                                }),
                                sparkline: Some(s.activity_30d.clone()),
                                delay_ms: 0,
                            }
                            StatCard {
                                value: s.published,
                                label: "已发布".to_string(),
                                trend: None,
                                sparkline: None,
                                delay_ms: 120,
                            }
                            StatCard {
                                value: s.drafts,
                                label: "草稿".to_string(),
                                trend: None,
                                sparkline: None,
                                delay_ms: 240,
                            }
                        }
                    }
                    (None, true) => {
                        rsx! {
                            div { class: "{ADMIN_CARD_CLASS} md:col-span-3 p-8 h-36 flex flex-col sm:flex-row sm:items-center justify-between gap-4 animate-page-enter",
                                div {
                                    div { class: "text-sm font-medium text-[var(--color-paper-secondary)]",
                                        "总文章数 / 已发布 / 草稿"
                                    }
                                    div { class: "text-base text-[var(--color-paper-primary)] mt-2",
                                        "统计数据加载失败"
                                    }
                                }
                                button {
                                    class: "{BTN_OUTLINE}",
                                    onclick: retry,
                                    "重试"
                                }
                            }
                        }
                    }
                    (None, false) => {
                        rsx! {
                            for _ in 0..3 {
                                div { class: "{ADMIN_CARD_CLASS} p-8 flex flex-col justify-between h-36 animate-pulse",
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
                match (pending_count(), pending_failed()) {
                    (Some(count), _) => {
                        let (color_class, text_class) = if count > 0 {
                            (
                                "text-amber-600 dark:text-amber-400",
                                "text-amber-600 dark:text-amber-400",
                            )
                        } else {
                            (
                                "text-[var(--color-paper-secondary)]",
                                "text-[var(--color-paper-primary)]",
                            )
                        };
                        rsx! {
                            Link {
                                class: "block {ADMIN_CARD_CLASS} p-8 bg-[var(--color-paper-entry)] hover:bg-[var(--color-paper-border)]/20 transition-all h-36 flex flex-col justify-between group hover:-translate-y-1 hover:shadow-md duration-200 {pending_enter_class}",
                                style: "animation-delay: 360ms",
                                to: Route::AdminComments {},
                                div { class: "text-sm font-medium {color_class}", "待审评论" }
                                div { class: "flex items-baseline justify-between mt-4",
                                    CountUp {
                                        target: count,
                                        class: format!("text-4xl font-light tracking-tight tabular-nums {text_class}"),
                                    }
                                    div { class: "text-xs font-medium text-[var(--color-paper-secondary)] group-hover:text-[var(--color-paper-primary)] transition-colors",
                                        "去审核 →"
                                    }
                                }
                            }
                        }
                    }
                    (None, true) => {
                        rsx! {
                            div { class: "{ADMIN_CARD_CLASS} p-8 h-36 flex flex-col justify-between animate-page-enter",
                                div { class: "text-sm font-medium text-[var(--color-paper-secondary)]",
                                    "待审评论"
                                }
                                div { class: "flex items-center justify-between mt-4",
                                    span { class: "text-sm text-[var(--color-paper-tertiary)]",
                                        "加载失败"
                                    }
                                    button {
                                        class: "{BTN_OUTLINE}",
                                        onclick: retry,
                                        "重试"
                                    }
                                }
                            }
                        }
                    }
                    (None, false) => {
                        rsx! {
                            div { class: "{ADMIN_CARD_CLASS} p-8 h-36 flex flex-col justify-between animate-pulse",
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
                match (recent_posts(), posts_failed()) {
                    // 空库 / 无文章：展示空状态占位（与 posts.rs 列表页一致）。
                    // 放在 ADMIN_TABLE_CLASS 容器之外，避免 overflow-hidden 裁掉插画的 py-20 内边距。
                    (Some(posts), _) if posts.is_empty() => {
                        rsx! {
                            EmptyState {
                                title: "暂无文章",
                                description: "还没有创建任何文章，开始写下你的第一篇文字吧。",
                                action: Some(EmptyStateAction {
                                    label: "写文章".to_string(),
                                    onclick: Callback::new(move |_| {
                                        let _ = dioxus::router::navigator().push(Route::Write {});
                                    }),
                                }),
                            }
                        }
                    }
                    (Some(posts), _) => {
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
                    (None, true) => {
                        rsx! {
                            div { class: "{ADMIN_TABLE_CLASS} px-8 py-6 flex items-center justify-between animate-page-enter",
                                span { class: "text-sm text-[var(--color-paper-secondary)]",
                                    "近期文章加载失败"
                                }
                                button {
                                    class: "{BTN_OUTLINE}",
                                    onclick: retry,
                                    "重试"
                                }
                            }
                        }
                    }
                    // 加载中：骨架屏。
                    (None, false) => {
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

/// 趋势徽章：`positive` 决定绿色语义样式（与全站 green 徽章约定一致），
/// 文本自带 ↑ 箭头与颜色形成双编码；中性数据（如 +0）走描边样式。
#[derive(Clone, PartialEq)]
struct TrendBadge {
    text: String,
    positive: bool,
}

#[component]
fn StatCard(
    value: i64,
    label: String,
    trend: Option<TrendBadge>,
    sparkline: Option<Vec<i64>>,
    delay_ms: i32,
) -> Element {
    let badge_class = match &trend {
        Some(t) if t.positive => {
            "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300"
        }
        _ => "border border-[var(--color-paper-border)] text-[var(--color-paper-tertiary)]",
    };
    rsx! {
        div {
            class: "{ADMIN_CARD_CLASS} p-8 flex flex-col justify-between h-36 relative group hover:-translate-y-1 hover:shadow-md transition-all duration-200 animate-page-enter",
            style: "animation-delay: {delay_ms}ms",
            div { class: "flex justify-between items-start",
                div { class: "text-sm font-medium text-[var(--color-paper-secondary)]",
                    "{label}"
                }
                if let Some(t) = trend {
                    div { class: "text-xs px-2 py-0.5 rounded-full {badge_class}",
                        "{t.text}"
                    }
                }
            }
            div { class: "flex items-end justify-between gap-2 mt-4",
                CountUp {
                    target: value,
                    class: "text-4xl font-light tracking-tight tabular-nums text-[var(--color-paper-primary)]"
                        .to_string(),
                }
                if let Some(data) = sparkline {
                    Sparkline { data }
                }
            }
        }
    }
}

/// 迷你折线图：近 30 个自然日每日新建文章数。
///
/// 折线（位置/长度通道）是 NN/g 认可的前注意处理最准的趋势形式；无文章日
/// 自然落为基线（全零时呈现一条平线，如实呈现而非隐藏）。装饰性图表，
/// 趋势语义已由 TrendBadge 文案双编码承载，故对读屏器隐藏。
#[component]
fn Sparkline(data: Vec<i64>) -> Element {
    const W: f64 = 96.0;
    const H: f64 = 28.0;
    const PAD: f64 = 2.0;
    let n = data.len();
    if n < 2 {
        return rsx! {};
    }
    // max(1) 护住全零序列：所有点落基线，避免除零。
    let max = (*data.iter().max().unwrap_or(&0)).max(1) as f64;
    let step_x = W / (n - 1) as f64;
    let points: Vec<String> = data
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let y = H - PAD - (*v as f64 / max) * (H - 2.0 * PAD);
            format!("{:.1},{:.1}", i as f64 * step_x, y)
        })
        .collect();
    let polyline_points = points.join(" ");
    let area_d = format!("M0,{H} L{} L{W},{H} Z", points.join(" L"));

    rsx! {
        svg {
            class: "w-24 h-7 shrink-0",
            view_box: "0 0 {W} {H}",
            fill: "none",
            "aria-hidden": "true",
            path {
                d: area_d,
                fill: "color-mix(in srgb, var(--color-paper-accent) 12%, transparent)",
            }
            polyline {
                points: polyline_points,
                stroke: "var(--color-paper-accent)",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
            }
        }
    }
}

/// 数字滚动组件:值从 0 以 easeOutQuint 缓动递增到 `target`(约 450ms)。
///
/// 语义是「数据已聚合完成」的信号而非装饰；450ms 在 NN/g 动效时长上限
/// (500ms) 内。命中 `prefers-reduced-motion` 时直接显示终值。动画在
/// `use_effect` 内驱动,渲染体保持纯净(见 dioxus-render-purity 规范);
/// 数据仅 WASM 端加载,SSR 不挂载本组件。
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
            const DURATION_MS: i64 = 450;
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
        // 整行跳转后台只读预览（/admin/preview/:slug），草稿亦可预览。
        Link {
            class: "flex flex-col sm:flex-row sm:justify-between sm:items-center px-8 py-5 hover:bg-[var(--color-paper-accent-soft)] transition-colors cursor-pointer group animate-row-enter",
            style: "animation-delay: {delay_ms}ms",
            to: Route::PostPreview { slug: post.slug.clone() },
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
            span { class: "flex items-center gap-3 text-sm text-[var(--color-paper-secondary)] mt-2 sm:mt-0",
                "{date_str}"
                // 可点击性暗示：整行是 Link，hover 时浮现箭头。
                span { class: "opacity-0 group-hover:opacity-100 transition-opacity text-[var(--color-paper-tertiary)]",
                    "→"
                }
            }
        }
    }
}
