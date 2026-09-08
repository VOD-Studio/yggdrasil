//! 归档页面模块。
//!
//! 对应路由 `/archives`。
//! 顶部标签索引复用可折叠卡片与 TagChip，独立加载，避免标签接口失败影响时间归档。
//!
//! 数据获取：通过 `use_server_future` 调用 `list_published_posts(1, 10000)` server function，
//! 一次性拉取全部已发布文章，然后在内存中按发布日期的年、月进行分组展示。
//! 在 `wasm32` 目标下，server function 的函数体被替换为向服务端端点发起 HTTP POST 请求的客户端存根；
//! 实际的数据库访问逻辑仅在 `feature = "server"` 启用时运行。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::api::posts::{list_published_posts, list_tags, PostListResponse, TagListResponse};
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::archive_skeleton::ArchiveSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::skeletons::tags_skeleton::TagsSkeleton;
use crate::components::ui::{CollapsibleSettingsCard, TagChip, BTN_OUTLINE};
use crate::models::post::PostListItem;
use crate::router::Route;

/// 按年份分组的文章归档结构。
#[derive(Clone, PartialEq)]
struct YearGroup {
    year: String,
    months: Vec<MonthGroup>,
}

/// 按月份分组的文章归档结构。
#[derive(Clone, PartialEq)]
struct MonthGroup {
    month: String,
    month_en: String,
    posts: Vec<PostListItem>,
}

/// 将文章列表按 `formatted_date()` 返回的 `YYYY-MM-DD` 格式进行年、月分组。
///
/// 返回的结果按原始文章顺序组织，调用前已按发布时间降序排列。
fn group_posts(posts: &[PostListItem]) -> Vec<YearGroup> {
    let mut years: Vec<YearGroup> = vec![];

    for post in posts {
        let date_str = post.formatted_date();

        // 将日期字符串拆分为 [年, 月, 日] 三部分。
        let parts: Vec<&str> = date_str.split('-').collect();
        if parts.len() != 3 {
            continue;
        }
        let year = parts[0].to_string();
        let month_num = parts[1];
        // 将数字月份转换为英文月份名称，用于展示与锚点 id。
        let month_en = match month_num {
            "01" => "January",
            "02" => "February",
            "03" => "March",
            "04" => "April",
            "05" => "May",
            "06" => "June",
            "07" => "July",
            "08" => "August",
            "09" => "September",
            "10" => "October",
            "11" => "November",
            "12" => "December",
            _ => month_num,
        };

        // 尝试追加到当前年份与月份的组中；如果不匹配则新建分组。
        if let Some(yg) = years.last_mut() {
            if yg.year == year {
                if let Some(mg) = yg.months.last_mut() {
                    if mg.month_en == month_en {
                        mg.posts.push(post.clone());
                        continue;
                    }
                }
                yg.months.push(MonthGroup {
                    month: month_num.to_string(),
                    month_en: month_en.to_string(),
                    posts: vec![post.clone()],
                });
                continue;
            }
        }
        years.push(YearGroup {
            year,
            months: vec![MonthGroup {
                month: month_num.to_string(),
                month_en: month_en.to_string(),
                posts: vec![post.clone()],
            }],
        });
    }

    years
}

/// 归档页面组件，对应路由 `/archives`。
///
/// 渲染页面标题，并委托给 `ArchivesContent` 展示按年月分组的文章列表。
#[component]
pub fn Archives() -> Element {
    rsx! {
        div { class: "archives-page animate-page-enter",
            header { class: "archive-intro",
                p { class: "archive-eyebrow", span { aria_hidden: "true" } "THE ARCHIVE / 时间的索引" }
                div { class: "archive-intro-line",
                    h1 { "归档" span { class: "archive-title-dot", "。" } }
                    p { "把片刻写成文字，把文字留给时间。" }
                }
            }
            SuspenseBoundary {
                fallback: move |_| rsx! {
                    div { "data-vt-list-pending": "true",
                        DelayedSkeleton { TagsSkeleton {} }
                    }
                },
                ArchiveTags {}
            }
            SuspenseBoundary {
                fallback: move |_| rsx! { DelayedSkeleton { ArchiveSkeleton { include_tags: false } } },
                ArchivesContent {}
            }
        }
    }
}

/// 标签云保持挂载，让收起也能完成高度与标签淡出动画。
#[component]
fn ArchiveTags() -> Element {
    let entry_id = use_hook(crate::bridges::navigation::entry_id);
    let mut tags_open =
        use_signal(|| crate::bridges::navigation::read_state("archive-tags-open").unwrap_or(false));
    use_effect(move || {
        crate::bridges::navigation::write_state(&entry_id, "archive-tags-open", &tags_open());
    });
    let mut tags_res = use_server_future(list_tags)?;
    let tags_data = tags_res.read();
    let summary = match &*tags_data {
        Some(Ok(TagListResponse { tags })) => {
            format!("{} 个标签 · 选择一个话题，发现相关文章", tags.len())
        }
        Some(Err(_)) => "标签暂时未能加载，仍可浏览下方归档".to_string(),
        None => "正在整理标签…".to_string(),
    };

    rsx! {
        CollapsibleSettingsCard {
            title: "标签索引",
            summary,
            enabled: true,
            default_open: tags_open(),
            on_toggle: move |_| tags_open.set(!tags_open()),
            class: "archive-tags",
            panel_id: "archive-tags-panel",
            div { class: "archive-tags-body",
                match &*tags_data {
                    Some(Ok(TagListResponse { tags })) if !tags.is_empty() => rsx! {
                        ul { class: "archive-tags-list", aria_label: "文章标签",
                            for (index, tag) in tags.iter().enumerate() {
                                li {
                                    key: "{tag.id}",
                                    style: "--tag-delay: {index.min(10) * 18}ms",
                                    TagChip {
                                        label: tag.name.clone(),
                                        to: Route::TagDetail { tag: tag.name.clone() },
                                        variant: "archive",
                                        count: tag.post_count,
                                    }
                                }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        p { class: "text-sm text-paper-secondary py-2", "还没有标签，先看看下方的文章吧。" }
                    },
                    Some(Err(_)) => rsx! {
                        button {
                            r#type: "button",
                            class: "{BTN_OUTLINE} archive-tags-retry",
                            onclick: move |_| tags_res.restart(),
                            "重新加载标签"
                        }
                    },
                    None => rsx! {
                        p { class: "text-sm text-paper-secondary py-2", role: "status", "正在加载标签…" }
                    },
                }
            }
        }
    }
}

/// 归档页面内容组件。
///
/// 通过 `use_server_future` 获取全部已发布文章，按年月分组后渲染；
/// 加载中显示骨架屏，失败显示错误提示。
#[component]
fn ArchivesContent() -> Element {
    // 一次性获取足够多的已发布文章，用于生成完整的年/月归档。
    let mut posts_res = use_server_future(move || list_published_posts(1, 10000))?;

    let posts_data = posts_res.read();
    match &*posts_data {
        Some(Ok(PostListResponse { posts, total })) => {
            if *total == 0 {
                rsx! {
                    span { hidden: true, "data-vt-list": "true" }
                    EmptyState {
                        title: "还没有文章归档",
                        description: "发布文章后，这里会自动按年月进行归档显示。",
                    }
                }
            } else {
                let grouped = group_posts(posts);
                rsx! {
                    section { class: "archive-timeline", "data-vt-list": "true", aria_labelledby: "archive-timeline-title",
                        div { class: "archive-toolbar",
                            div { class: "archive-toolbar-title",
                                h2 { id: "archive-timeline-title", "时间归档" }
                                span { class: "archive-total", "{total} 篇文章" }
                            }
                            span { class: "archive-order", "由近及远" span { aria_hidden: "true", "↓" } }
                        }
                        if grouped.len() > 1 {
                            nav { class: "archive-year-nav", aria_label: "按年份跳转",
                                for year_group in grouped.iter() {
                                    a { key: "{year_group.year}", href: "#{year_group.year}", "{year_group.year}" }
                                }
                            }
                        }
                        for year_group in grouped.iter() {
                            YearSection {
                                key: "{year_group.year}",
                                year_group: year_group.clone(),
                            }
                        }
                        footer { class: "archive-colophon",
                            span { aria_hidden: "true", "✳" }
                            p { "写下的，替我们记得。" }
                        }
                    }
                }
            }
        }
        Some(Err(_)) => {
            rsx! {
                div { class: "archive-state", "data-vt-list": "true", role: "alert",
                    h2 { "暂时没能翻开归档" }
                    p { "文章加载失败，请稍后再试。" }
                    button {
                        r#type: "button",
                        class: "{BTN_OUTLINE} archive-tags-retry",
                        onclick: move |_| posts_res.restart(),
                        "重新加载"
                    }
                }
            }
        }
        None => {
            rsx! {
                DelayedSkeleton { ArchiveSkeleton { include_tags: false } }
            }
        }
    }
}

/// 单一年份归档区块组件，展示该年份下的所有月份分组。
#[component]
fn YearSection(year_group: YearGroup) -> Element {
    let total = year_group
        .months
        .iter()
        .map(|m| m.posts.len())
        .sum::<usize>();

    rsx! {
        section { class: "archive-year", aria_labelledby: "{year_group.year}",
            header { class: "archive-year-heading",
                div { class: "archive-year-sticky",
                    span { class: "archive-year-kicker", aria_hidden: "true", "YEAR / 年份" }
                    h3 { class: "archive-year-number", id: "{year_group.year}",
                        a { class: "archive-header-link", href: "#{year_group.year}", "{year_group.year}" }
                    }
                    p { class: "archive-year-summary", "{total} 篇文章" span { aria_hidden: "true", "·" } "{year_group.months.len()} 个月" }
                    span { class: "archive-year-rule", aria_hidden: "true" }
                }
            }
            div { class: "archive-months",
                for month_group in year_group.months.iter() {
                    MonthSection {
                        key: "{month_group.month_en}",
                        month_group: month_group.clone(),
                        year: year_group.year.clone(),
                    }
                }
            }
        }
    }
}

/// 单一月份归档区块组件，展示该月份下的文章条目。
#[component]
fn MonthSection(month_group: MonthGroup, year: String) -> Element {
    let count = month_group.posts.len();

    rsx! {
        section { class: "archive-month", aria_labelledby: "{year}-{month_group.month_en}",
            h4 {
                class: "archive-month-header",
                id: "{year}-{month_group.month_en}",
                a {
                    class: "archive-header-link",
                    href: "#{year}-{month_group.month_en}",
                    span { class: "archive-month-number", "{month_group.month}" }
                    span { "月" }
                    span { class: "archive-month-name", "{month_group.month_en}" }
                }
                span { class: "archive-month-count", "{count:02} 篇" }
            }
            ul { class: "archive-posts",
                for (index, post) in month_group.posts.iter().enumerate() {
                    li { key: "{post.id}", style: "--archive-delay: {index.min(6) * 35}ms",
                        ArchiveEntry { post: post.clone() }
                    }
                }
            }
        }
    }
}

/// 整行使用原生链接，日期保留完整 datetime，标题在窄屏自然换行。
#[component]
fn ArchiveEntry(post: PostListItem) -> Element {
    let date_str = post.formatted_date();
    let day = post
        .published_at
        .unwrap_or(post.created_at)
        .format("%d")
        .to_string();
    let topics = post
        .tags
        .iter()
        .take(2)
        .cloned()
        .collect::<Vec<_>>()
        .join(" / ");

    rsx! {
        Link {
            class: "archive-entry",
            "data-vt-post-link": "{post.id}",
            to: Route::PostDetail { slug: post.slug.clone() },
            time { class: "archive-date", datetime: "{date_str}", title: "{date_str}", aria_label: "{date_str}", "{day}" }
            div { class: "archive-entry-copy",
                h5 { class: "archive-entry-title", "data-vt-post-id": "{post.id}", "data-vt-role": "title", "{post.title}" }
                if !topics.is_empty() || post.reading_time > 0 {
                    div { class: "archive-entry-meta",
                        if !topics.is_empty() {
                            span { class: "archive-entry-topics", "{topics}" }
                        }
                        if !topics.is_empty() && post.reading_time > 0 {
                            span { aria_hidden: "true", "·" }
                        }
                        if post.reading_time > 0 {
                            span { class: "archive-reading-time", "{post.reading_time} 分钟阅读" }
                        }
                    }
                }
            }
            span { class: "archive-entry-arrow", aria_hidden: "true", "↗" }
        }
    }
}
