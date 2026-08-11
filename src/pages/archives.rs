//! 归档页面模块。
//!
//! 对应路由 `/archives`。
//!
//! 数据获取：通过 `use_server_future` 调用 `list_published_posts(1, 10000)` server function，
//! 一次性拉取全部已发布文章，然后在内存中按发布日期的年、月进行分组展示。
//! 在 `wasm32` 目标下，server function 的函数体被替换为向服务端端点发起 HTTP POST 请求的客户端存根；
//! 实际的数据库访问逻辑仅在 `feature = "server"` 启用时运行。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::api::posts::{list_published_posts, PostListResponse};
use crate::components::empty_state::EmptyState;
use crate::components::skeletons::archive_skeleton::ArchiveSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
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
                    month: month_en.to_string(),
                    month_en: month_en.to_string(),
                    posts: vec![post.clone()],
                });
                continue;
            }
        }
        years.push(YearGroup {
            year,
            months: vec![MonthGroup {
                month: month_en.to_string(),
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
        div { class: "animate-page-enter",
            header { class: "page-header mb-6",
                h1 { class: "text-4xl font-bold text-paper-primary tracking-tight",
                    "归档"
                }
            }
            ArchivesContent {}
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
    let posts_res = use_server_future(move || list_published_posts(1, 10000))?;

    let posts_data = posts_res.read();
    match &*posts_data {
        Some(Ok(PostListResponse { posts, total })) => {
            if *total == 0 {
                rsx! {
                    EmptyState {
                        title: "还没有文章归档",
                        description: "发布文章后，这里会自动按年月进行归档显示。",
                    }
                }
            } else {
                let grouped = group_posts(posts);
                rsx! {
                    div { class: "mt-2 text-base text-paper-secondary",
                        "共 "
                        span { class: "font-medium text-paper-primary", "{total}" }
                        " 篇文章"
                    }
                    for year_group in grouped.iter() {
                        YearSection {
                            key: "{year_group.year}",
                            year_group: year_group.clone(),
                        }
                    }
                }
            }
        }
        Some(Err(_)) => {
            rsx! {
                div { class: "text-center text-red-500 dark:text-red-400 py-20", "加载失败" }
            }
        }
        None => {
            rsx! {
                DelayedSkeleton { ArchiveSkeleton {} }
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
        div { class: "archive-year mt-10",
            h2 {
                class: "archive-year-header text-2xl font-bold text-paper-primary mb-4",
                id: "{year_group.year}",
                a {
                    class: "archive-header-link hover:opacity-80 transition-opacity",
                    href: "#{year_group.year}",
                    "{year_group.year}"
                }
                sup { class: "archive-count text-sm text-paper-secondary ml-1", "{total}" }
            }
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

/// 单一月份归档区块组件，展示该月份下的文章条目。
#[component]
fn MonthSection(month_group: MonthGroup, year: String) -> Element {
    let count = month_group.posts.len();

    rsx! {
        div { class: "archive-month flex flex-col md:flex-row md:items-start py-2.5 border-b border-paper-border/50",
            h3 {
                class: "archive-month-header text-lg font-medium text-paper-secondary md:w-[200px] shrink-0 mt-0 mb-0 py-1.5",
                id: "{year}-{month_group.month_en}",
                a {
                    class: "archive-header-link hover:opacity-80 transition-opacity",
                    href: "#{year}-{month_group.month_en}",
                    "{month_group.month}"
                }
                sup { class: "archive-count text-sm text-paper-secondary ml-1", "{count}" }
            }
            div { class: "archive-posts flex-1",
                for post in month_group.posts.iter() {
                    ArchiveEntry { key: "{post.id}", post: post.clone() }
                }
            }
        }
    }
}

/// 单条归档文章组件，展示标题与发布日期，并通过覆盖层链接到文章详情。
#[component]
fn ArchiveEntry(post: PostListItem) -> Element {
    let date_str = post.formatted_date();

    rsx! {
        div { class: "archive-entry relative py-1.5 my-2.5 group",
            h3 { class: "archive-entry-title text-base font-normal text-paper-primary m-0",
                "{post.title}"
            }
            div { class: "archive-meta text-sm text-paper-secondary mt-1", "{date_str}" }
            Link {
                class: "entry-link absolute inset-0 z-10",
                aria_label: "post link to {post.title}",
                to: Route::PostDetail {
                    slug: post.slug.clone(),
                },
            }
        }
    }
}
