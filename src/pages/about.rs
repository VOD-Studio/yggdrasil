//! 关于页：单栏编辑排版。站点简介、常写标签条形图、每年篇数柱状图、技术栈与版本、
//! 入口卡片与订阅。统计全部来自已缓存的公开接口，逐个等待以免 SSR 丢失唤醒。

use dioxus::prelude::*;

use crate::api::changelog::latest_release;
use crate::api::posts::{list_tags, posts_per_year, YearCount};
use crate::components::skeletons::atoms::SkeletonBox;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{Icon, IconName};
use crate::models::post::Tag;
use crate::pages::archives::ranked_tags;
use crate::router::Route;

/// 条形图列出的标签数。
const TOP_TAGS: usize = 8;
/// 柱状图最多显示的年份数（取最近的）。
const MAX_YEARS: usize = 8;
/// 柱子的最大高度（px），与样式里的绘图区一致。
const BAR_MAX_PX: f64 = 96.0;

#[component]
pub fn About() -> Element {
    rsx! {
        document::Title { "关于 · Yggdrasil" }
        article { class: "about",
            div { class: "about-head page-enter",
                h1 { class: "about-title", "关于" }
                span { class: "about-mascot",
                    img {
                        class: "about-mascot-img",
                        src: "/images/about/claude-girl.webp",
                        alt: "橙色长发的像素风女孩，双手捧着一只橙色小动物",
                        width: "256",
                        height: "256",
                        decoding: "async",
                    }
                    img {
                        class: "about-mascot-img about-mascot-hover",
                        src: "/images/about/claude-girl-hover.webp",
                        alt: "",
                        aria_hidden: "true",
                        width: "256",
                        height: "256",
                        decoding: "async",
                    }
                }
            }
            div { class: "about-prose page-enter", style: "--enter-index: 1",
                p {
                    "这里是 xfy 的个人站点。大多数文章在拆解一个具体的实现：一台 6502 的时钟、"
                    "一个聊天系统的断线续传、一个黑洞渲染器的积分器。也有一些学习笔记，记下读书和踩坑时的发现。"
                }
            }
            SuspenseBoundary {
                fallback: |_| rsx! { DelayedSkeleton { AboutStatsSkeleton {} } },
                AboutStats {}
            }
        }
    }
}

#[component]
fn AboutStats() -> Element {
    // SSR 只轮询正在挂起的任务，提前发出的第二个请求会丢失唤醒；逐个等待。
    let years = use_server_future(posts_per_year)?;
    let tags = use_server_future(list_tags)?;
    let release = use_server_future(latest_release)?;

    let years: Vec<YearCount> = years
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let tags: Vec<Tag> = tags
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|r| ranked_tags(r.tags.clone()))
        .unwrap_or_default();
    let release = release
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .flatten();
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
        ("前端", "Dioxus 0.7 · WebAssembly".to_string()),
        ("服务端", "Axum · 服务端渲染".to_string()),
        ("数据", "PostgreSQL".to_string()),
        ("AI 接入", "MCP：读写文章与笔记".to_string()),
        ("正文", "可运行代码 · Mermaid · 公式".to_string()),
        ("当前版本", version_fact),
    ];

    rsx! {
        if let Some(first) = years.first().filter(|_| total > 0) {
            p { class: "about-prose about-prose-muted tabular-nums page-enter", style: "--enter-index: 1",
                "从 {first.year} 年的第一篇写到现在，一共 {total} 篇。"
            }
        }
        if !tags.is_empty() {
            TagBars { tags: tags.into_iter().take(TOP_TAGS).collect::<Vec<_>>() }
        }
        if total > 0 {
            YearBars { years }
        }
        section { class: "about-site", aria_labelledby: "about-site-title",
            h2 { id: "about-site-title", class: "about-site-title", "这个站点" }
            div { class: "about-prose about-prose-small",
                p {
                    "Yggdrasil 既是站名，也是它的程序：一份 Rust 代码，同时编译出浏览器里的 WebAssembly "
                    "前端和服务器上的 Axum 后端。实现细节写在"
                    a { href: "/doc/yggdrasil/index.html", target: "_blank", rel: "noopener", "站点文档" }
                    "里。"
                }
            }
            dl { class: "about-facts",
                for (label, value) in facts {
                    div { key: "{label}",
                        dt { "{label}" }
                        dd { class: "tabular-nums", "{value}" }
                    }
                }
            }
        }
        section { class: "about-entries", aria_label: "更多",
            AboutEntry {
                to: EntryTarget::Route(Route::WritingGuide {}),
                title: "写作指南",
                description: "把这里的写作方式交给你的 AI：SKILL.md、MCP 接入与正文语法。",
                meta: "/about/writing".to_string(),
            }
            AboutEntry {
                to: EntryTarget::Document("/about/components"),
                title: "组件图鉴",
                description: "组成这个站点的界面组件，每一个都能直接操作。",
                meta: "/about/components".to_string(),
            }
            AboutEntry {
                to: EntryTarget::Route(Route::Changelog {}),
                title: "更新日志",
                description: "每个版本改了什么、修了什么。",
                meta: format!("最新 {version}"),
            }
        }
        section { class: "about-subscribe", aria_labelledby: "about-subscribe-title",
            div {
                h2 { id: "about-subscribe-title", "订阅" }
                p { "新文章发布后，阅读器会自动收到。" }
            }
            div { class: "about-subscribe-links",
                a { class: "btn btn-secondary btn-sm", href: "/feed.xml",
                    Icon { name: IconName::Rss, size: 15 }
                    "RSS"
                }
                a { class: "btn btn-secondary btn-sm", href: "/feed.json", "JSON Feed" }
            }
        }
    }
}

/// 写得最多的标签：横向条形，长度按篇数相对最多的标签。
#[component]
fn TagBars(tags: Vec<Tag>) -> Element {
    let max = tags.iter().map(|t| t.post_count).max().unwrap_or(1).max(1);
    rsx! {
        section { class: "about-chart", aria_labelledby: "about-tags-title",
            h2 { id: "about-tags-title", class: "section-label", "写得最多的" }
            ol { class: "about-tag-bars",
                for tag in tags {
                    li { key: "{tag.id}",
                        Link { class: "about-tag-name", to: Route::TagDetail { tag: tag.name.clone() }, "{tag.name}" }
                        span { class: "about-tag-track", aria_hidden: "true",
                            span {
                                class: "about-tag-bar",
                                style: "width: {bar_percent(tag.post_count, max):.1}%",
                            }
                        }
                        span { class: "about-tag-count tabular-nums", "{tag.post_count}" }
                    }
                }
            }
        }
    }
}

/// 每年写下的篇数：纵向柱子，今年（最后一年）用 Clay 标出；数值直接写在柱顶。
#[component]
fn YearBars(years: Vec<YearCount>) -> Element {
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
    rsx! {
        section { class: "about-chart", aria_labelledby: "about-years-title",
            h2 { id: "about-years-title", class: "section-label", "每年写下的篇数" }
            div {
                class: "about-year-bars",
                role: "img",
                aria_label: "{label}",
                style: "--years: {columns}",
                for year in shown {
                    div { key: "{year.year}", class: "about-year", title: "{year.year} 年 {year.count} 篇",
                        span { class: "about-year-value tabular-nums",
                            if year.count > 0 { "{year.count}" } else { "—" }
                        }
                        span {
                            class: if Some(year.year) == last { "about-year-bar is-current" } else if year.count == 0 { "about-year-bar is-empty" } else { "about-year-bar" },
                            style: "height: {bar_height(year.count, max):.0}px",
                        }
                        span { class: "about-year-label tabular-nums", "{year.year}" }
                    }
                }
            }
        }
    }
}

fn bar_percent(count: i64, max: i64) -> f64 {
    count as f64 / max as f64 * 100.0
}

/// 柱高至少 2px，0 篇的年份也留一条基线。
fn bar_height(count: i64, max: i64) -> f64 {
    (count as f64 / max as f64 * BAR_MAX_PX).max(2.0)
}

/// 入口卡片的去向。
#[derive(Clone, PartialEq)]
enum EntryTarget {
    /// 前台路由，走客户端导航。
    Route(Route),
    /// full 客户端专属路由（如组件图鉴，plan §6.1/§6.6）：渲染普通 `<a href>`，
    /// 点击走真正的文档导航，服务端按路径选择 full 模板。不能用 Link：服务端为
    /// /about 选择 reader 模板时，Link 在 SSR HTML 里留下的 data-node-hydration
    /// 监听器标记与 reader 客户端的无监听器模板失配，hydration 直接崩溃。
    Document(&'static str),
}

#[component]
fn AboutEntry(
    to: EntryTarget,
    title: &'static str,
    description: &'static str,
    meta: String,
) -> Element {
    let body = rsx! {
        span { class: "about-entry-title",
            "{title}"
            span { class: "about-entry-arrow", aria_hidden: "true", Icon { name: IconName::ArrowRight, size: 15 } }
        }
        span { class: "about-entry-description", "{description}" }
        span { class: "about-entry-meta tabular-nums", "{meta}" }
    };
    match to {
        EntryTarget::Route(to) => rsx! {
            Link { class: "about-entry", to, {body} }
        },
        EntryTarget::Document(href) => rsx! {
            a { class: "about-entry", href, {body} }
        },
    }
}

#[component]
fn AboutStatsSkeleton() -> Element {
    rsx! {
        div { role: "status", aria_label: "正在加载统计",
            div { aria_hidden: "true",
                SkeletonBox { class: "h-4 mt-4 w-64" }
                div { class: "about-chart",
                    SkeletonBox { class: "h-3 w-20 mb-4" }
                    for i in 0..6 {
                        SkeletonBox { key: "{i}", class: "h-2 mt-4 w-full" }
                    }
                }
            }
        }
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
