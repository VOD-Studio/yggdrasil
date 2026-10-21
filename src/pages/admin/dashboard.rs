//! 后台仪表盘。
//!
//! 问候与常用操作 → 四格数字（已发布、草稿、评论、标签）→ 左栏「每月发布篇数」柱图
//! 与近期文章，右栏最新评论与站点信息。各区块分别加载，失败的区块原位显示「加载失败 +
//! 重试」，不回退成 0。数据仅在 WASM 前端通过 server function 加载。样式见
//! styles/pages/admin-dashboard.css。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::api::admin_stats::AdminNavCounts;
use crate::api::database::backup::BackupInfo;
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, Icon, IconName, LoadError, UserAvatar,
};
use crate::models::comment::{AdminComment, CommentStatus};
use crate::models::post::{MonthCount, PostListItem, PostStats, PostStatus, Tag};
use crate::router::Route;
use crate::utils::time::{local_now, to_local};

/// 区块的加载状态。
#[derive(Clone, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
enum Load<T> {
    Loading,
    Ready(T),
    Failed,
}

impl<T: Clone> Load<T> {
    fn ready(&self) -> Option<T> {
        match self {
            Load::Ready(value) => Some(value.clone()),
            _ => None,
        }
    }
}

/// 按本地时间问候。
fn greeting(hour: u32) -> &'static str {
    match hour {
        5..=10 => "早上好",
        11..=12 => "中午好",
        13..=17 => "下午好",
        _ => "晚上好",
    }
}

fn weekday_label(weekday: chrono::Weekday) -> &'static str {
    use chrono::Weekday::*;
    match weekday {
        Mon => "周一",
        Tue => "周二",
        Wed => "周三",
        Thu => "周四",
        Fri => "周五",
        Sat => "周六",
        Sun => "周日",
    }
}

/// 柱图纵轴上限：取最大值向上凑到 4 的倍数，网格线落在整数上；全为 0 时为 4。
fn chart_max(months: &[MonthCount]) -> i64 {
    let max = months.iter().map(|m| m.count).max().unwrap_or(0).max(1);
    (max + 3) / 4 * 4
}

/// 使用最多的标签。
fn top_tag(tags: &[Tag]) -> Option<&Tag> {
    tags.iter()
        .filter(|t| t.post_count > 0)
        .max_by(|a, b| a.post_count.cmp(&b.post_count).then(b.name.cmp(&a.name)))
}

/// 评论正文的一行预览：去掉图片与常见 Markdown 记号，只有图片的评论显示「[图片]」。
fn comment_preview(md: &str) -> String {
    let mut text = String::new();
    let mut rest = md;
    // 图片 `![alt](url)` 整段去掉。
    while let Some(start) = rest.find("![") {
        text.push_str(&rest[..start]);
        let tail = &rest[start..];
        match tail.find(')') {
            Some(end) if tail[..end].contains("](") => rest = &tail[end + 1..],
            _ => {
                text.push_str("![");
                rest = &tail[2..];
            }
        }
    }
    text.push_str(rest);
    let text: String = text
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '#' | '>'))
        .collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        "[图片]".to_string()
    } else {
        text
    }
}

#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub fn Admin() -> Element {
    let ctx: crate::context::UserContext = use_context();
    let mut stats = use_signal(|| Load::<PostStats>::Loading);
    let mut recent = use_signal(|| Load::<Vec<PostListItem>>::Loading);
    let mut counts = use_signal(|| Load::<AdminNavCounts>::Loading);
    let mut comments = use_signal(|| Load::<Vec<AdminComment>>::Loading);
    let mut tags = use_signal(|| Load::<Vec<Tag>>::Loading);
    let mut backups = use_signal(|| Load::<Vec<BackupInfo>>::Loading);
    let mut reload = use_signal(|| 0_u32);

    use_effect(move || {
        let _ = reload();
        #[cfg(target_arch = "wasm32")]
        {
            if !matches!(*stats.peek(), Load::Ready(_)) {
                stats.set(Load::Loading);
                spawn(async move {
                    stats.set(match crate::api::posts::get_post_stats().await {
                        Ok(res) => Load::Ready(res.stats),
                        Err(_) => Load::Failed,
                    });
                });
            }
            if !matches!(*recent.peek(), Load::Ready(_)) {
                recent.set(Load::Loading);
                spawn(async move {
                    recent.set(
                        match crate::api::posts::list_posts(1, 5, Default::default()).await {
                            Ok(res) => Load::Ready(res.posts),
                            Err(_) => Load::Failed,
                        },
                    );
                });
            }
            if !matches!(*counts.peek(), Load::Ready(_)) {
                counts.set(Load::Loading);
                spawn(async move {
                    counts.set(
                        match crate::api::admin_stats::get_admin_nav_counts().await {
                            Ok(res) => Load::Ready(res),
                            Err(_) => Load::Failed,
                        },
                    );
                });
            }
            if !matches!(*comments.peek(), Load::Ready(_)) {
                comments.set(Load::Loading);
                spawn(async move {
                    comments.set(
                        match crate::api::comments::get_all_comments(1, None).await {
                            Ok(res) => Load::Ready(res.comments.into_iter().take(4).collect()),
                            Err(_) => Load::Failed,
                        },
                    );
                });
            }
            if !matches!(*tags.peek(), Load::Ready(_)) {
                tags.set(Load::Loading);
                spawn(async move {
                    tags.set(match crate::api::posts::list_tags().await {
                        Ok(res) => Load::Ready(res.tags),
                        Err(_) => Load::Failed,
                    });
                });
            }
            if !matches!(*backups.peek(), Load::Ready(_)) {
                backups.set(Load::Loading);
                spawn(async move {
                    backups.set(match crate::api::database::backup::list_backups().await {
                        Ok(res) => Load::Ready(res),
                        Err(_) => Load::Failed,
                    });
                });
            }
        }
    });
    // 只重载失败的区块；已就绪的保留。
    let retry = move |_| reload += 1;

    let now = local_now();
    let name = (ctx.user)()
        .map(|user| user.display_label().to_string())
        .unwrap_or_default();
    let stats_now = stats();
    let summary = match &stats_now {
        Load::Ready(s) => format!(
            "{} 月 {} 日 {} · 近 30 天新增 {} 篇",
            now.format("%-m"),
            now.format("%-d"),
            weekday_label(chrono::Datelike::weekday(&now)),
            s.recent_30d
        ),
        _ => format!(
            "{} 月 {} 日 {}",
            now.format("%-m"),
            now.format("%-d"),
            weekday_label(chrono::Datelike::weekday(&now))
        ),
    };

    rsx! {
        div { class: "admin-dash",
            div { class: "admin-page-head",
                div {
                    h1 { "{greeting(chrono::Timelike::hour(&now))}，{name}" }
                    p { "{summary}" }
                }
                div { class: "admin-page-actions",
                    Link { class: button_class(ButtonVariant::Primary, ButtonSize::Md), to: Route::Write {},
                        Icon { name: IconName::Plus, size: 16 }
                        "写文章"
                    }
                    Link { class: button_class(ButtonVariant::Secondary, ButtonSize::Md), to: Route::NewNote {}, "快速记录" }
                    Link { class: button_class(ButtonVariant::Secondary, ButtonSize::Md), to: Route::Assets {}, "上传素材" }
                }
            }

            StatTiles { stats: stats_now.clone(), counts: counts(), tags: tags(), on_retry: retry }

            div { class: "dash-cols",
                div { class: "dash-col",
                    section { class: "admin-panel dash-chart", aria_labelledby: "dash-chart-title",
                        div { class: "dash-panel-head",
                            h2 { id: "dash-chart-title", "每月发布篇数" }
                            if let Load::Ready(s) = &stats_now {
                                span {
                                    "近 12 个月 · 共 {s.monthly_published.iter().map(|m| m.count).sum::<i64>()} 篇"
                                }
                            }
                        }
                        match &stats_now {
                            Load::Ready(s) => rsx! {
                                PublishRhythm { months: s.monthly_published.clone() }
                            },
                            Load::Loading => rsx! {
                                SkeletonBox { class: "dash-chart-skeleton" }
                            },
                            Load::Failed => rsx! {
                                LoadError { title: "发布统计加载失败", on_retry: retry }
                            },
                        }
                    }
                    RecentPosts { posts: recent(), on_retry: retry }
                }
                div { class: "dash-col",
                    LatestComments { comments: comments(), counts: counts.read().ready(), on_retry: retry }
                    SiteInfo { stats: stats_now.ready(), backups: backups(), on_retry: retry }
                }
            }
        }
    }
}

/// 四格数字。各格独立显示加载中与失败，失败时提供一次重试。
#[component]
fn StatTiles(
    stats: Load<PostStats>,
    counts: Load<AdminNavCounts>,
    tags: Load<Vec<Tag>>,
    on_retry: EventHandler<()>,
) -> Element {
    let failed = matches!(stats, Load::Failed)
        || matches!(counts, Load::Failed)
        || matches!(tags, Load::Failed);
    let tile = |label: &'static str,
                to: Option<Route>,
                value: Option<String>,
                meta: Option<(String, bool)>| {
        let body = rsx! {
            span { class: "dash-tile-label", "{label}" }
            match value {
                Some(value) => rsx! {
                    span { class: "dash-tile-value", "{value}" }
                },
                None => rsx! {
                    SkeletonBox { class: "dash-tile-value-skeleton" }
                },
            }
            if let Some((meta, alert)) = meta {
                span { class: if alert { "dash-tile-meta dash-tile-meta-alert" } else { "dash-tile-meta" }, "{meta}" }
            }
        };
        match to {
            Some(to) => rsx! {
                Link { class: "admin-panel dash-tile", to, {body} }
            },
            None => rsx! {
                div { class: "admin-panel dash-tile", {body} }
            },
        }
    };
    let s = stats.ready();
    let c = counts.ready();
    let t = tags.ready();
    rsx! {
        div { class: "dash-tiles",
            {tile(
                "已发布",
                Some(Route::Posts {}),
                s.as_ref().map(|s| s.published.to_string()),
                s.as_ref().map(|s| (format!("近 30 天新增 {}", s.recent_30d), false)),
            )}
            {tile(
                "草稿",
                Some(Route::Posts {}),
                s.as_ref().map(|s| s.drafts.to_string()),
                s.as_ref().map(|s| {
                    if s.drafts == 0 {
                        ("没有未完成的草稿".to_string(), false)
                    } else {
                        ("等你写完".to_string(), false)
                    }
                }),
            )}
            {tile(
                "评论",
                Some(Route::AdminComments {}),
                c.map(|c| c.comments.to_string()),
                c.map(|c| (format!("待审 {}", c.pending_comments), c.pending_comments > 0)),
            )}
            {tile(
                "标签",
                None,
                t.as_ref().map(|t| t.iter().filter(|t| t.post_count > 0).count().to_string()),
                t.as_ref().map(|t| {
                    (
                        top_tag(t).map(|tag| format!("最常用：{}", tag.name)).unwrap_or_else(|| "还没有标签".to_string()),
                        false,
                    )
                }),
            )}
        }
        if failed {
            LoadError { class: "dash-tiles-error", title: "部分统计加载失败", on_retry: move |_| on_retry.call(()) }
        }
    }
}

/// 每月发布篇数柱图：12 根柱，本月为陶土色；悬停或键盘聚焦显示提示，其余柱变淡。
#[component]
fn PublishRhythm(months: Vec<MonthCount>) -> Element {
    let mut hover = use_signal(|| None::<usize>);
    let max = chart_max(&months);
    let last = months.len().saturating_sub(1);
    let current = hover();
    rsx! {
        div { class: "dash-chart-plot", "data-hovering": current.is_some().then_some("true"),
            div { class: "dash-chart-grid", aria_hidden: "true",
                // 五条线（含底线）对应 max、¾、½、¼ 与 0。
                for i in 0..=4 {
                    span { key: "{i}", "data-value": "{max - max / 4 * i}" }
                }
            }
            ol { class: "dash-chart-bars", aria_label: "近 12 个月每月发布篇数",
                for (i , m) in months.iter().copied().enumerate() {
                    {
                        let pct = m.count as f64 / max as f64 * 100.0;
                        let label = format!("{} 年 {} 月 · {} 篇", m.year, m.month, m.count);
                        rsx! {
                            li {
                                key: "{m.year}-{m.month}",
                                class: "dash-chart-col",
                                tabindex: "0",
                                aria_label: "{label}",
                                "data-active": (current == Some(i)).then_some("true"),
                                onmouseenter: move |_| hover.set(Some(i)),
                                onmouseleave: move |_| hover.set(None),
                                onfocus: move |_| hover.set(Some(i)),
                                onblur: move |_| hover.set(None),
                                span {
                                    class: if m.count == 0 { "dash-chart-bar dash-chart-bar-zero" } else if i == last { "dash-chart-bar dash-chart-bar-current" } else { "dash-chart-bar" },
                                    style: if m.count > 0 { format!("height: {pct:.1}%;") } else { String::new() },
                                }
                                if current == Some(i) {
                                    span {
                                        class: "dash-chart-tip",
                                        role: "tooltip",
                                        style: "bottom: calc({pct:.1}% + 8px);",
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "dash-chart-labels", aria_hidden: "true",
                for m in months.iter() {
                    span { key: "{m.year}-{m.month}", "{m.month}月" }
                }
            }
        }
    }
}

#[component]
fn RecentPosts(posts: Load<Vec<PostListItem>>, on_retry: EventHandler<()>) -> Element {
    rsx! {
        section { class: "admin-panel dash-list", aria_labelledby: "dash-recent-title",
            div { class: "dash-panel-head dash-panel-head-rule",
                h2 { id: "dash-recent-title", "近期文章" }
                Link { class: "dash-more", to: Route::Posts {}, "全部文章" Icon { name: IconName::ArrowRight, size: 13 } }
            }
            match posts {
                Load::Loading => rsx! {
                    div { class: "dash-list-body",
                        for i in 0..5 {
                            SkeletonBox { key: "{i}", class: "dash-row-skeleton" }
                        }
                    }
                },
                Load::Failed => rsx! {
                    LoadError { title: "近期文章加载失败", on_retry: move |_| on_retry.call(()) }
                },
                Load::Ready(posts) if posts.is_empty() => rsx! {
                    p { class: "dash-empty",
                        "还没有文章。"
                        Link { to: Route::Write {}, "写第一篇" }
                    }
                },
                Load::Ready(posts) => rsx! {
                    ul { class: "dash-list-body",
                        for post in posts {
                            {
                                let date = to_local(post.published_at.unwrap_or(post.created_at))
                                    .format("%m-%d %H:%M")
                                    .to_string();
                                let published = post.status == PostStatus::Published;
                                rsx! {
                                    li { key: "{post.id}",
                                        Link { class: "dash-post-row", to: Route::WriteEdit { id: post.id },
                                            span { class: "dash-post-title", "{post.title}" }
                                            span { class: "status-dot", "data-tone": if published { "clay" } else { "muted" },
                                                "{post.status.label()}"
                                            }
                                            time { class: "dash-post-date", "{date}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn LatestComments(
    comments: Load<Vec<AdminComment>>,
    counts: Option<AdminNavCounts>,
    on_retry: EventHandler<()>,
) -> Element {
    let pending = counts.map(|c| c.pending_comments);
    rsx! {
        section { class: "admin-panel dash-list", aria_labelledby: "dash-comments-title",
            div { class: "dash-panel-head dash-panel-head-rule",
                h2 { id: "dash-comments-title", "最新评论" }
                if let Some(pending) = pending {
                    Link {
                        class: if pending > 0 { "dash-chip dash-chip-alert" } else { "dash-chip" },
                        to: Route::AdminComments {},
                        "待审 {pending}"
                    }
                }
            }
            match comments {
                Load::Loading => rsx! {
                    div { class: "dash-list-body",
                        for i in 0..4 {
                            SkeletonBox { key: "{i}", class: "dash-comment-skeleton" }
                        }
                    }
                },
                Load::Failed => rsx! {
                    LoadError { title: "最新评论加载失败", on_retry: move |_| on_retry.call(()) }
                },
                Load::Ready(list) if list.is_empty() => rsx! {
                    p { class: "dash-empty", "还没有评论。" }
                },
                Load::Ready(list) => rsx! {
                    ul { class: "dash-list-body",
                        for comment in list {
                            {
                                let when = to_local(comment.created_at).format("%m-%d").to_string();
                                let preview = comment_preview(&comment.content_md);
                                let is_pending = comment.status == CommentStatus::Pending;
                                rsx! {
                                    li { key: "{comment.id}",
                                        Link { class: "dash-comment-row", to: Route::AdminComments {},
                                            UserAvatar {
                                                name: comment.author_name.clone(),
                                                avatar_url: Some(comment.avatar_url.clone()),
                                                class: "w-[26px] h-[26px] rounded-full text-[11.5px] flex-shrink-0",
                                            }
                                            span { class: "dash-comment-body",
                                                span { class: "dash-comment-meta",
                                                    span { class: "dash-comment-author", "{comment.author_name}" }
                                                    if is_pending {
                                                        span { class: "dash-comment-pending", "待审" }
                                                    }
                                                    time { "{when}" }
                                                }
                                                span { class: "dash-comment-text", "{preview}" }
                                                span { class: "dash-comment-post", "于《{comment.post_title}》" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn SiteInfo(
    stats: Option<PostStats>,
    backups: Load<Vec<BackupInfo>>,
    on_retry: EventHandler<()>,
) -> Element {
    let last_backup = match &backups {
        Load::Ready(list) => Some(
            list.iter()
                .filter_map(|b| b.created_at.clone())
                .max()
                .map(|iso| crate::utils::time::format_relative_time_iso(&iso))
                .unwrap_or_else(|| "还没有备份".to_string()),
        ),
        _ => None,
    };
    rsx! {
        section { class: "admin-panel dash-site", aria_labelledby: "dash-site-title",
            h2 { id: "dash-site-title", "站点" }
            dl {
                dt { "版本" }
                dd { class: "font-mono", "{env!(\"CARGO_PKG_VERSION\")}" }
                dt { "回收站" }
                dd {
                    match stats {
                        Some(s) => rsx! {
                            Link { to: Route::PostsTrash {}, "{s.trash} 篇" }
                        },
                        None => rsx! { "—" },
                    }
                }
                dt { "上次备份" }
                dd {
                    match (last_backup, &backups) {
                        (Some(label), _) => rsx! { "{label}" },
                        (None, Load::Failed) => rsx! {
                            button { class: "dash-inline-retry", r#type: "button", onclick: move |_| on_retry.call(()), "读取失败，重试" }
                        },
                        _ => rsx! { "…" },
                    }
                }
            }
            Link { class: "dash-more", to: Route::System {}, "系统状态" Icon { name: IconName::ArrowRight, size: 13 } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn months(counts: &[i64]) -> Vec<MonthCount> {
        counts
            .iter()
            .enumerate()
            .map(|(i, &count)| MonthCount {
                year: 2026,
                month: i as u32 + 1,
                count,
            })
            .collect()
    }

    #[test]
    fn chart_max_rounds_up_to_quarter_gridlines() {
        assert_eq!(chart_max(&months(&[0, 0])), 4);
        assert_eq!(chart_max(&months(&[3, 1])), 4);
        assert_eq!(chart_max(&months(&[5])), 8);
        assert_eq!(chart_max(&[]), 4);
    }

    #[test]
    fn top_tag_prefers_count_then_name() {
        let tags = vec![
            Tag {
                id: 0,
                name: "Go".into(),
                post_count: 2,
            },
            Tag {
                id: 0,
                name: "Rust".into(),
                post_count: 5,
            },
            Tag {
                id: 0,
                name: "Dioxus".into(),
                post_count: 5,
            },
        ];
        assert_eq!(top_tag(&tags).map(|t| t.name.as_str()), Some("Dioxus"));
        assert!(top_tag(&[]).is_none());
    }

    #[test]
    fn greeting_follows_local_hour() {
        assert_eq!(greeting(8), "早上好");
        assert_eq!(greeting(12), "中午好");
        assert_eq!(greeting(15), "下午好");
        assert_eq!(greeting(23), "晚上好");
        assert_eq!(greeting(3), "晚上好");
    }

    #[test]
    fn comment_preview_strips_markdown_and_marks_images() {
        assert_eq!(comment_preview("**好**  文章"), "好 文章");
        assert_eq!(comment_preview("![](/uploads/a.webp)"), "[图片]");
    }
}
