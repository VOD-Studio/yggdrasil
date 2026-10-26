//! 关于页：单栏编辑排版。站点简介、常写标签条形图、每年篇数柱状图、技术栈与版本、
//! 入口卡片与订阅。统计全部来自已缓存的公开接口，在 `LazyRoute::data` 里一次创建
//! （`hooks::ssr_resource::route_data`）。

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::changelog::latest_release;
use crate::api::posts::{posts_per_year, TagListResponse, YearCount};
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{Icon, IconName};
use crate::hooks::ssr_resource::{prefetch_route_data, route_data, unit, PageJson, RouteData};
use crate::models::post::Tag;
use crate::pages::archives::{fetch_tags, ranked_tags};
use crate::routes::Route;

/// 条形图列出的标签数。
const TOP_TAGS: usize = 8;
/// 柱状图最多显示的年份数（取最近的）。
const MAX_YEARS: usize = 8;
/// 柱子的最大高度（px），与样式里的绘图区一致。
const BAR_MAX_PX: f64 = 96.0;

/// 最新发布的版本号与可选日期。
type Release = Option<(String, Option<String>)>;

/// 关于页路由，对应 `/about`（`SsrMode::InOrder`）。
pub struct AboutRoute {
    years: RouteData<Vec<YearCount>>,
    tags: RouteData<TagListResponse>,
    release: RouteData<Release>,
}

#[leptos::lazy]
async fn fetch_years() -> PageJson<Vec<YearCount>> {
    PageJson::new(posts_per_year().await)
}

#[leptos::lazy]
async fn fetch_release() -> PageJson<Release> {
    PageJson::new(latest_release().await)
}

#[lazy_route]
impl LazyRoute for AboutRoute {
    fn data() -> Self {
        Self {
            years: route_data(|| (), unit(fetch_years)),
            tags: route_data(|| (), unit(fetch_tags)),
            release: route_data(|| (), unit(fetch_release)),
        }
    }

    fn view(this: Self) -> AnyView {
        let AboutRoute {
            years,
            tags,
            release,
        } = this;
        view! {
            <Title text="关于 · Yggdrasil"/>
            <article class="about">
                <div class="about-head page-enter">
                    <h1 class="about-title">"关于"</h1>
                    <span class="about-mascot">
                        <img
                            class="about-mascot-img"
                            src="/images/about/claude-girl.webp"
                            alt="橙色长发的像素风女孩，双手捧着一只橙色小动物"
                            width="256"
                            height="256"
                            decoding="async"
                        />
                        <img
                            class="about-mascot-img about-mascot-hover"
                            src="/images/about/claude-girl-hover.webp"
                            alt=""
                            aria-hidden="true"
                            width="256"
                            height="256"
                            decoding="async"
                        />
                    </span>
                </div>
                <div class="about-prose page-enter" style="--enter-index: 1">
                    <p>
                        "这里是 xfy 的个人站点。大多数文章在拆解一个具体的实现：一台 6502 的时钟、"
                        "一个聊天系统的断线续传、一个黑洞渲染器的积分器。也有一些学习笔记，记下读书和踩坑时的发现。"
                    </p>
                </div>
                <Suspense fallback=|| {
                    view! {
                        <DelayedSkeleton>
                            <AboutStatsSkeleton/>
                        </DelayedSkeleton>
                    }
                }>
                    {move || Suspend::new(async move {
                        // 统计失败只是不显示对应图表，页面其余部分照常。
                        let years = years.await.unwrap_or_default();
                        let tags = tags
                            .await
                            .map(|r| ranked_tags(r.tags))
                            .unwrap_or_default();
                        let release = release.await.ok().flatten();
                        view! { <AboutStats years=years tags=tags release=release/> }
                    })}
                </Suspense>
            </article>
        }
        .into_any()
    }
}

impl AboutRoute {
    /// 悬停预取：与 `data()` 相同的取数函数与 source。
    pub fn prefetch_data() {
        prefetch_route_data((), unit(fetch_years));
        prefetch_route_data((), unit(fetch_tags));
        prefetch_route_data((), unit(fetch_release));
    }
}

#[component]
fn AboutStats(years: Vec<YearCount>, tags: Vec<Tag>, release: Release) -> impl IntoView {
    let total: i64 = years.iter().map(|y| y.count).sum();
    let version = release
        .as_ref()
        .map(|(v, _)| v.clone())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let version_fact = match release.as_ref().and_then(|(_, date)| date.clone()) {
        Some(date) => format!("{version} · {date}"),
        None => version.clone(),
    };
    let facts = [
        ("前端", "Leptos 0.8 · WebAssembly".to_string()),
        ("服务端", "Axum · 服务端渲染".to_string()),
        ("数据", "PostgreSQL".to_string()),
        ("AI 接入", "MCP：读写文章与笔记".to_string()),
        ("正文", "可运行代码 · Mermaid · 公式".to_string()),
        ("当前版本", version_fact),
    ]
    .into_iter()
    .map(|(label, value)| {
        view! {
            <div>
                <dt>{label}</dt>
                <dd class="tabular-nums">{value}</dd>
            </div>
        }
    })
    .collect_view();

    let intro = years.first().filter(|_| total > 0).map(|first| {
        view! {
            <p
                class="about-prose about-prose-muted tabular-nums page-enter"
                style="--enter-index: 1"
            >
                {format!("从 {} 年的第一篇写到现在，一共 {total} 篇。", first.year)}
            </p>
        }
    });
    let tag_bars = (!tags.is_empty()).then(|| {
        let top = tags.into_iter().take(TOP_TAGS).collect::<Vec<_>>();
        view! { <TagBars tags=top/> }
    });
    let year_bars = (total > 0).then(|| view! { <YearBars years=years/> });

    view! {
        {intro} {tag_bars} {year_bars}
        <section class="about-site" aria-labelledby="about-site-title">
            <h2 id="about-site-title" class="about-site-title">
                "这个站点"
            </h2>
            <div class="about-prose about-prose-small">
                <p>
                    "Yggdrasil 既是站名，也是它的程序：一份 Rust 代码，同时编译出浏览器里的 WebAssembly "
                    "前端和服务器上的 Axum 后端。实现细节写在"
                    <a href="/doc/yggdrasil/index.html" target="_blank" rel="noopener external">
                        "站点文档"
                    </a> "里。"
                </p>
            </div>
            <dl class="about-facts">{facts}</dl>
        </section>
        <section class="about-entries" aria-label="更多">
            <AboutEntry
                href=Route::WritingGuide.to_string()
                title="写作指南"
                description="把这里的写作方式交给你的 AI：SKILL.md、MCP 接入与正文语法。"
                meta="/about/writing".to_string()
            />
            <AboutEntry
                href=Route::ComponentShowcase.to_string()
                title="组件图鉴"
                description="组成这个站点的界面组件，每一个都能直接操作。"
                meta="/about/components".to_string()
            />
            <AboutEntry
                href=Route::Changelog.to_string()
                title="更新日志"
                description="每个版本改了什么、修了什么。"
                meta=format!("最新 {version}")
            />
        </section>
        <section class="about-subscribe" aria-labelledby="about-subscribe-title">
            <div>
                <h2 id="about-subscribe-title">"订阅"</h2>
                <p>"新文章发布后，阅读器会自动收到。"</p>
            </div>
            <div class="about-subscribe-links">
                <a class="btn btn-secondary btn-sm" href="/feed.xml" rel="external">
                    <Icon name=IconName::Rss size=15/>
                    "RSS"
                </a>
                <a class="btn btn-secondary btn-sm" href="/feed.json" rel="external">
                    "JSON Feed"
                </a>
            </div>
        </section>
    }
}

/// 写得最多的标签：横向条形，长度按篇数相对最多的标签。
#[component]
fn TagBars(tags: Vec<Tag>) -> impl IntoView {
    let max = tags.iter().map(|t| t.post_count).max().unwrap_or(1).max(1);
    let items = tags
        .into_iter()
        .map(|tag| {
            view! {
                <li>
                    <a
                        class="about-tag-name"
                        href=Route::TagDetail {
                            tag: tag.name.clone(),
                        }
                            .to_string()
                    >
                        {tag.name.clone()}
                    </a>
                    <span class="about-tag-track" aria-hidden="true">
                        <span
                            class="about-tag-bar"
                            style=format!("width: {:.1}%", bar_percent(tag.post_count, max))
                        ></span>
                    </span>
                    <span class="about-tag-count tabular-nums">{tag.post_count}</span>
                </li>
            }
        })
        .collect_view();
    view! {
        <section class="about-chart" aria-labelledby="about-tags-title">
            <h2 id="about-tags-title" class="section-label">
                "写得最多的"
            </h2>
            <ol class="about-tag-bars">{items}</ol>
        </section>
    }
}

/// 每年写下的篇数：纵向柱子，今年（最后一年）用 Clay 标出；数值直接写在柱顶。
#[component]
fn YearBars(years: Vec<YearCount>) -> impl IntoView {
    let shown: Vec<YearCount> = years
        .iter()
        .skip(years.len().saturating_sub(MAX_YEARS))
        .cloned()
        .collect();
    let max = shown.iter().map(|y| y.count).max().unwrap_or(1).max(1);
    let label = shown
        .iter()
        .map(|y| format!("{} 年 {} 篇", y.year, y.count))
        .collect::<Vec<_>>()
        .join("，");
    let last = shown.last().map(|y| y.year);
    let columns = shown.len();
    let bars = shown
        .into_iter()
        .map(|year| {
            let bar_class = if Some(year.year) == last {
                "about-year-bar is-current"
            } else if year.count == 0 {
                "about-year-bar is-empty"
            } else {
                "about-year-bar"
            };
            let value = if year.count > 0 {
                year.count.to_string()
            } else {
                "—".to_string()
            };
            view! {
                <div class="about-year" title=format!("{} 年 {} 篇", year.year, year.count)>
                    <span class="about-year-value tabular-nums">{value}</span>
                    <span
                        class=bar_class
                        style=format!("height: {:.0}px", bar_height(year.count, max))
                    ></span>
                    <span class="about-year-label tabular-nums">{year.year}</span>
                </div>
            }
        })
        .collect_view();
    view! {
        <section class="about-chart" aria-labelledby="about-years-title">
            <h2 id="about-years-title" class="section-label">
                "每年写下的篇数"
            </h2>
            <div
                class="about-year-bars"
                role="img"
                aria-label=label
                style=format!("--years: {columns}")
            >
                {bars}
            </div>
        </section>
    }
}

fn bar_percent(count: i64, max: i64) -> f64 {
    count as f64 / max as f64 * 100.0
}

/// 柱高至少 2px，0 篇的年份也留一条基线。
fn bar_height(count: i64, max: i64) -> f64 {
    (count as f64 / max as f64 * BAR_MAX_PX).max(2.0)
}

/// 入口卡片：站内路由的普通链接，点击由路由协调器接管做客户端导航。
#[component]
fn AboutEntry(
    href: String,
    title: &'static str,
    description: &'static str,
    meta: String,
) -> impl IntoView {
    view! {
        <a class="about-entry" href=href>
            <span class="about-entry-title">
                {title}
                <span class="about-entry-arrow" aria-hidden="true">
                    <Icon name=IconName::ArrowRight size=15/>
                </span>
            </span>
            <span class="about-entry-description">{description}</span>
            <span class="about-entry-meta tabular-nums">{meta}</span>
        </a>
    }
}

#[component]
fn AboutStatsSkeleton() -> impl IntoView {
    view! {
        <div role="status" aria-label="正在加载统计">
            <div aria-hidden="true">
                <SkeletonBox class="h-4 mt-4 w-64"/>
                <div class="about-chart">
                    <SkeletonBox class="h-3 w-20 mb-4"/>
                    {(0..6)
                        .map(|_| view! { <SkeletonBox class="h-2 mt-4 w-full"/> })
                        .collect_view()}
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_scale_to_the_largest_value_and_keep_a_baseline() {
        assert_eq!(bar_percent(8, 16), 50.0);
        assert_eq!(bar_height(15, 15), BAR_MAX_PX);
        assert_eq!(bar_height(0, 15), 2.0);
    }
}
