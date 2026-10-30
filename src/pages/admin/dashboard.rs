//! 后台仪表盘。
//!
//! 问候与常用操作 → 四格数字（已发布、草稿、评论、标签）→ 左栏「每月发布篇数」柱图
//! 与近期文章，右栏最新评论与站点信息。各区块分别加载，失败的区块原位显示「加载失败 +
//! 重试」，不回退成 0。数据仅在 WASM 前端通过 server function 加载（Effect + 世代号，
//! 服务端不请求）。样式见 styles/pages/admin-dashboard.css。

use std::future::Future;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::admin_stats::AdminNavCounts;
use crate::api::database::backup::BackupInfo;
use crate::components::admin_layout::AdminPageHead;
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::ui::{
    button_class, ButtonSize, ButtonVariant, Icon, IconName, LoadError, StatusDot, UserAvatar,
};
use crate::context::UserContext;
use crate::models::comment::{AdminComment, CommentStatus};
use crate::models::post::{MonthCount, PostListItem, PostStats, Tag};
use crate::routes::Route;
use crate::utils::time::{hm, local_now, md, to_local};

/// 区块的加载状态。
#[derive(Clone, PartialEq)]
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

/// 一个独立加载的区块：状态信号 + 世代号（丢弃过期响应）。
struct Section<T: Send + Sync + 'static> {
    state: RwSignal<Load<T>>,
    generation: RwSignal<u64>,
}

// 信号句柄本身是 Copy，不要求 `T: Clone`，所以不用 derive。
impl<T: Send + Sync + 'static> Clone for Section<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for Section<T> {}

impl<T: Send + Sync + 'static> Section<T> {
    fn new() -> Self {
        Self {
            state: RwSignal::new(Load::Loading),
            generation: RwSignal::new(0),
        }
    }

    /// 只重载尚未就绪的区块；`fetch` 失败（`None`）时置为失败态。
    fn refresh<Fut>(self, fetch: impl FnOnce() -> Fut)
    where
        Fut: Future<Output = Option<T>> + 'static,
    {
        if self.state.with_untracked(|s| matches!(s, Load::Ready(_))) {
            return;
        }
        if !self.state.with_untracked(|s| matches!(s, Load::Loading)) {
            self.state.set(Load::Loading);
        }
        let id = self.generation.get_untracked() + 1;
        self.generation.set(id);
        let future = fetch();
        spawn_local(async move {
            let next = match future.await {
                Some(value) => Load::Ready(value),
                None => Load::Failed,
            };
            if self.generation.try_get_untracked() == Some(id) {
                let _ = self.state.try_set(next);
            }
        });
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

/// 评论正文的一行预览：去掉图片与常见 Markdown 记号，只有图片的评论显示「\[图片\]」。
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

/// 仪表盘路由（无 SSR 数据，各区块在客户端独立加载）。
pub struct DashboardRoute;

#[lazy_route]
impl LazyRoute for DashboardRoute {
    fn data() -> Self {
        Self
    }

    fn view(_this: Self) -> AnyView {
        view! { <Admin/> }.into_any()
    }
}

#[component]
fn Admin() -> impl IntoView {
    let ctx = expect_context::<UserContext>();
    let stats = Section::<PostStats>::new();
    let recent = Section::<Vec<PostListItem>>::new();
    let counts = Section::<AdminNavCounts>::new();
    let comments = Section::<Vec<AdminComment>>::new();
    let tags = Section::<Vec<Tag>>::new();
    let backups = Section::<Vec<BackupInfo>>::new();
    let reload = RwSignal::new(0_u32);

    // 首次进入与点击重试时加载；只重载尚未就绪的区块，已就绪的保留。
    Effect::new(move |_| {
        reload.track();
        stats.refresh(|| async {
            crate::api::posts::get_post_stats()
                .await
                .ok()
                .map(|res| res.stats)
        });
        recent.refresh(|| async {
            crate::api::posts::list_posts(1, 5, Default::default())
                .await
                .ok()
                .map(|res| res.posts)
        });
        counts.refresh(|| async { crate::api::admin_stats::get_admin_nav_counts().await.ok() });
        comments.refresh(|| async {
            crate::api::comments::get_all_comments(1, None)
                .await
                .ok()
                .map(|res| res.comments.into_iter().take(4).collect())
        });
        tags.refresh(|| async {
            crate::api::posts::list_tags()
                .await
                .ok()
                .map(|res| res.tags)
        });
        backups.refresh(|| async { crate::api::database::backup::list_backups().await.ok() });
    });
    let retry = Callback::new(move |()| reload.update(|n| *n += 1));

    let now = local_now();
    let hour = chrono::Timelike::hour(&now);
    let date_label = format!(
        "{} 月 {} 日 {}",
        chrono::Datelike::month(&now),
        chrono::Datelike::day(&now),
        weekday_label(chrono::Datelike::weekday(&now))
    );
    let name = Memo::new(move |_| {
        ctx.user
            .read()
            .as_ref()
            .map(|user| user.display_label().to_string())
            .unwrap_or_default()
    });
    let title = Signal::derive(move || format!("{}，{}", greeting(hour), name.get()));
    let summary = move || match stats.state.get() {
        Load::Ready(s) => format!("{date_label} · 近 30 天新增 {} 篇", s.recent_30d),
        _ => date_label.clone(),
    };

    view! {
        <div class="admin-dash">
            <AdminPageHead
                title=title
                actions=|| {
                    view! {
                        <a
                            class=button_class(ButtonVariant::Primary, ButtonSize::Md)
                            href=Route::Write {}.to_string()
                        >
                            <Icon name=IconName::Plus size=16/>
                            "写文章"
                        </a>
                        <a
                            class=button_class(ButtonVariant::Secondary, ButtonSize::Md)
                            href=Route::NewNote {}.to_string()
                        >
                            "快速记录"
                        </a>
                        <a
                            class=button_class(ButtonVariant::Secondary, ButtonSize::Md)
                            href=Route::Assets {}.to_string()
                        >
                            "上传素材"
                        </a>
                    }
                }
            >
                {summary}
            </AdminPageHead>

            <StatTiles stats=stats.state counts=counts.state tags=tags.state on_retry=retry/>

            <div class="dash-cols">
                <div class="dash-col">
                    <section class="admin-panel dash-chart" aria-labelledby="dash-chart-title">
                        <div class="dash-panel-head">
                            <h2 id="dash-chart-title">"每月发布篇数"</h2>
                            {move || {
                                stats
                                    .state
                                    .get()
                                    .ready()
                                    .map(|s| {
                                        let total: i64 = s
                                            .monthly_published
                                            .iter()
                                            .map(|m| m.count)
                                            .sum();
                                        view! { <span>{format!("近 12 个月 · 共 {total} 篇")}</span> }
                                    })
                            }}
                        </div>
                        {move || match stats.state.get() {
                            Load::Ready(s) => {
                                view! { <PublishRhythm months=s.monthly_published/> }.into_any()
                            }
                            Load::Loading => {
                                view! { <SkeletonBox class="dash-chart-skeleton"/> }.into_any()
                            }
                            Load::Failed => {
                                view! { <LoadError title="发布统计加载失败" on_retry=retry/> }.into_any()
                            }
                        }}
                    </section>
                    <RecentPosts posts=recent.state on_retry=retry/>
                </div>
                <div class="dash-col">
                    <LatestComments
                        comments=comments.state
                        counts=Signal::derive(move || counts.state.get().ready())
                        on_retry=retry
                    />
                    <SiteInfo
                        stats=Signal::derive(move || stats.state.get().ready())
                        backups=backups.state
                        on_retry=retry
                    />
                </div>
            </div>
        </div>
    }
}

/// 一格数字：标签、数值（加载中为骨架）与可选附注；有目标路由时整格是链接。
fn tile(
    label: &'static str,
    to: Option<String>,
    value: Option<String>,
    meta: Option<(String, bool)>,
) -> AnyView {
    let body = view! {
        <span class="dash-tile-label">{label}</span>
        {match value {
            Some(value) => view! { <span class="dash-tile-value">{value}</span> }.into_any(),
            None => view! { <SkeletonBox class="dash-tile-value-skeleton"/> }.into_any(),
        }}
        {meta
            .map(|(meta, alert)| {
                view! {
                    <span class={
                        if alert { "dash-tile-meta dash-tile-meta-alert" } else { "dash-tile-meta" }
                    }>{meta}</span>
                }
            })}
    };
    match to {
        Some(href) => view! { <a class="admin-panel dash-tile" href=href>{body}</a> }.into_any(),
        None => view! { <div class="admin-panel dash-tile">{body}</div> }.into_any(),
    }
}

/// 四格数字。各格独立显示加载中与失败，失败时提供一次重试。
#[component]
fn StatTiles(
    #[prop(into)] stats: Signal<Load<PostStats>>,
    #[prop(into)] counts: Signal<Load<AdminNavCounts>>,
    #[prop(into)] tags: Signal<Load<Vec<Tag>>>,
    #[prop(into)] on_retry: Callback<()>,
) -> impl IntoView {
    let failed = move || {
        stats.with(|s| matches!(s, Load::Failed))
            || counts.with(|c| matches!(c, Load::Failed))
            || tags.with(|t| matches!(t, Load::Failed))
    };
    view! {
        {move || {
            let s = stats.get().ready();
            let c = counts.get().ready();
            let t = tags.get().ready();
            view! {
                <div class="dash-tiles">
                    {tile(
                        "已发布",
                        Some(Route::Posts {}.to_string()),
                        s.as_ref().map(|s| s.published.to_string()),
                        s.as_ref().map(|s| (format!("近 30 天新增 {}", s.recent_30d), false)),
                    )}
                    {tile(
                        "草稿",
                        Some(Route::Posts {}.to_string()),
                        s.as_ref().map(|s| s.drafts.to_string()),
                        s.as_ref()
                            .map(|s| {
                                if s.drafts == 0 {
                                    ("没有未完成的草稿".to_string(), false)
                                } else {
                                    ("等你写完".to_string(), false)
                                }
                            }),
                    )}
                    {tile(
                        "评论",
                        Some(Route::AdminComments {}.to_string()),
                        c.map(|c| c.comments.to_string()),
                        c.map(|c| (format!("待审 {}", c.pending_comments), c.pending_comments > 0)),
                    )}
                    {tile(
                        "标签",
                        None,
                        t.as_ref().map(|t| t.iter().filter(|t| t.post_count > 0).count().to_string()),
                        t.as_ref()
                            .map(|t| {
                                (
                                    top_tag(t)
                                        .map(|tag| format!("最常用：{}", tag.name))
                                        .unwrap_or_else(|| "还没有标签".to_string()),
                                    false,
                                )
                            }),
                    )}
                </div>
            }
        }}
        {move || {
            failed()
                .then(|| {
                    view! {
                        <LoadError
                            class="dash-tiles-error"
                            title="部分统计加载失败"
                            on_retry=on_retry
                        />
                    }
                })
        }}
    }
}

/// 每月发布篇数柱图：12 根柱，本月为陶土色；悬停或键盘聚焦显示提示，其余柱变淡。
#[component]
fn PublishRhythm(months: Vec<MonthCount>) -> impl IntoView {
    let hover = RwSignal::new(None::<usize>);
    let max = chart_max(&months);
    let last = months.len().saturating_sub(1);
    let bars = months
        .iter()
        .copied()
        .enumerate()
        .map(|(i, m)| {
            let pct = m.count as f64 / max as f64 * 100.0;
            let label = format!("{} 年 {} 月 · {} 篇", m.year, m.month, m.count);
            let bar_class = if m.count == 0 {
                "dash-chart-bar dash-chart-bar-zero"
            } else if i == last {
                "dash-chart-bar dash-chart-bar-current"
            } else {
                "dash-chart-bar"
            };
            let bar_style = if m.count > 0 {
                format!("height: {pct:.1}%;")
            } else {
                String::new()
            };
            let tip_label = label.clone();
            view! {
                <li
                    class="dash-chart-col"
                    tabindex="0"
                    aria-label=label
                    data-active=move || (hover.get() == Some(i)).then_some("true")
                    on:mouseenter=move |_| hover.set(Some(i))
                    on:mouseleave=move |_| hover.set(None)
                    on:focus=move |_| hover.set(Some(i))
                    on:blur=move |_| hover.set(None)
                >
                    <span class=bar_class style=bar_style></span>
                    {move || {
                        (hover.get() == Some(i))
                            .then(|| {
                                view! {
                                    <span
                                        class="dash-chart-tip"
                                        role="tooltip"
                                        style=format!("bottom: calc({pct:.1}% + 8px);")
                                    >
                                        {tip_label.clone()}
                                    </span>
                                }
                            })
                    }}
                </li>
            }
        })
        .collect_view();
    let grid = (0..=4)
        .map(|i| view! { <span data-value=(max - max / 4 * i).to_string()></span> })
        .collect_view();
    let labels = months
        .iter()
        .map(|m| view! { <span>{format!("{}月", m.month)}</span> })
        .collect_view();
    view! {
        <div
            class="dash-chart-plot"
            data-hovering=move || hover.get().is_some().then_some("true")
        >
            // 五条线（含底线）对应 max、¾、½、¼ 与 0。
            <div class="dash-chart-grid" aria-hidden="true">
                {grid}
            </div>
            <ol class="dash-chart-bars" aria-label="近 12 个月每月发布篇数">
                {bars}
            </ol>
            <div class="dash-chart-labels" aria-hidden="true">
                {labels}
            </div>
        </div>
    }
}

#[component]
fn RecentPosts(
    #[prop(into)] posts: Signal<Load<Vec<PostListItem>>>,
    #[prop(into)] on_retry: Callback<()>,
) -> impl IntoView {
    view! {
        <section class="admin-panel dash-list" aria-labelledby="dash-recent-title">
            <div class="dash-panel-head dash-panel-head-rule">
                <h2 id="dash-recent-title">"近期文章"</h2>
                <a class="dash-more" href=Route::Posts {}.to_string()>
                    "全部文章"
                    <Icon name=IconName::ArrowRight size=13/>
                </a>
            </div>
            {move || match posts.get() {
                Load::Loading => {
                    view! {
                        <div class="dash-list-body">
                            {(0..5)
                                .map(|_| view! { <SkeletonBox class="dash-row-skeleton"/> })
                                .collect_view()}
                        </div>
                    }
                        .into_any()
                }
                Load::Failed => {
                    view! { <LoadError title="近期文章加载失败" on_retry=on_retry/> }.into_any()
                }
                Load::Ready(posts) if posts.is_empty() => {
                    view! {
                        <p class="dash-empty">
                            "还没有文章。" <a href=Route::Write {}.to_string()>"写第一篇"</a>
                        </p>
                    }
                        .into_any()
                }
                Load::Ready(posts) => {
                    view! {
                        <ul class="dash-list-body">
                            {posts
                                .into_iter()
                                .map(|post| {
                                    let t = to_local(post.published_at.unwrap_or(post.created_at));
                                    let date = format!("{} {}", md(&t, '-'), hm(&t));
                                    view! {
                                        <li>
                                            <a
                                                class="dash-post-row"
                                                href=Route::WriteEdit { id: post.id }.to_string()
                                            >
                                                <span class="dash-post-title">{post.title}</span>
                                                <StatusDot
                                                    label=post.status.label()
                                                    tone=post.status.tone()
                                                />
                                                <time class="dash-post-date">{date}</time>
                                            </a>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}

#[component]
fn LatestComments(
    #[prop(into)] comments: Signal<Load<Vec<AdminComment>>>,
    #[prop(into)] counts: Signal<Option<AdminNavCounts>>,
    #[prop(into)] on_retry: Callback<()>,
) -> impl IntoView {
    view! {
        <section class="admin-panel dash-list" aria-labelledby="dash-comments-title">
            <div class="dash-panel-head dash-panel-head-rule">
                <h2 id="dash-comments-title">"最新评论"</h2>
                {move || {
                    counts
                        .get()
                        .map(|c| c.pending_comments)
                        .map(|pending| {
                            view! {
                                <a
                                    class={
                                        if pending > 0 { "dash-chip dash-chip-alert" } else { "dash-chip" }
                                    }
                                    href=Route::AdminComments {}.to_string()
                                >
                                    {format!("待审 {pending}")}
                                </a>
                            }
                        })
                }}
            </div>
            {move || match comments.get() {
                Load::Loading => {
                    view! {
                        <div class="dash-list-body">
                            {(0..4)
                                .map(|_| view! { <SkeletonBox class="dash-comment-skeleton"/> })
                                .collect_view()}
                        </div>
                    }
                        .into_any()
                }
                Load::Failed => {
                    view! { <LoadError title="最新评论加载失败" on_retry=on_retry/> }.into_any()
                }
                Load::Ready(list) if list.is_empty() => {
                    view! { <p class="dash-empty">"还没有评论。"</p> }.into_any()
                }
                Load::Ready(list) => {
                    view! {
                        <ul class="dash-list-body">
                            {list
                                .into_iter()
                                .map(|comment| {
                                    let when = md(&to_local(comment.created_at), '-');
                                    let preview = comment_preview(&comment.content_md);
                                    let is_pending = comment.status == CommentStatus::Pending;
                                    view! {
                                        <li>
                                            <a
                                                class="dash-comment-row"
                                                href=Route::AdminComments {}.to_string()
                                            >
                                                <UserAvatar
                                                    name=comment.author_name.clone()
                                                    avatar_url=Some(comment.avatar_url.clone())
                                                    class="w-[26px] h-[26px] rounded-full text-[11.5px] flex-shrink-0"
                                                />
                                                <span class="dash-comment-body">
                                                    <span class="dash-comment-meta">
                                                        <span class="dash-comment-author">
                                                            {comment.author_name.clone()}
                                                        </span>
                                                        {is_pending
                                                            .then(|| {
                                                                view! { <span class="dash-comment-pending">"待审"</span> }
                                                            })}
                                                        <time>{when}</time>
                                                    </span>
                                                    <span class="dash-comment-text">{preview}</span>
                                                    <span class="dash-comment-post">
                                                        {format!("于《{}》", comment.post_title)}
                                                    </span>
                                                </span>
                                            </a>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                    }
                        .into_any()
                }
            }}
        </section>
    }
}

#[component]
fn SiteInfo(
    #[prop(into)] stats: Signal<Option<PostStats>>,
    #[prop(into)] backups: Signal<Load<Vec<BackupInfo>>>,
    #[prop(into)] on_retry: Callback<()>,
) -> impl IntoView {
    view! {
        <section class="admin-panel dash-site" aria-labelledby="dash-site-title">
            <h2 id="dash-site-title">"站点"</h2>
            <dl>
                <dt>"版本"</dt>
                <dd class="font-mono">{env!("CARGO_PKG_VERSION")}</dd>
                <dt>"回收站"</dt>
                <dd>
                    {move || match stats.get() {
                        Some(s) => {
                            view! {
                                <a href=Route::PostsTrash {}.to_string()>{format!("{} 篇", s.trash)}</a>
                            }
                                .into_any()
                        }
                        None => "—".into_any(),
                    }}
                </dd>
                <dt>"上次备份"</dt>
                <dd>
                    {move || match backups.get() {
                        Load::Ready(list) => {
                            list.iter()
                                .filter_map(|b| b.created_at.clone())
                                .max()
                                .map(|iso| crate::utils::time::format_relative_time_iso(&iso))
                                .unwrap_or_else(|| "还没有备份".to_string())
                                .into_any()
                        }
                        Load::Failed => {
                            view! {
                                <button
                                    class="dash-inline-retry"
                                    type="button"
                                    on:click=move |_| on_retry.run(())
                                >
                                    "读取失败，重试"
                                </button>
                            }
                                .into_any()
                        }
                        Load::Loading => "…".into_any(),
                    }}
                </dd>
            </dl>
            <a class="dash-more" href=Route::System {}.to_string()>
                "系统状态"
                <Icon name=IconName::ArrowRight size=13/>
            </a>
        </section>
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
                name: "Axum".into(),
                post_count: 5,
            },
        ];
        assert_eq!(top_tag(&tags).map(|t| t.name.as_str()), Some("Axum"));
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
