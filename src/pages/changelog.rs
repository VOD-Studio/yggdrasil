//! 更新日志页面模块。
//!
//! 对应路由 `/changelog`（`SsrMode::InOrder`）。
//!
//! 数据获取：`LazyRoute::data` 创建 `Resource` 调用 `get_changelog` server function，
//! 取回编译期内嵌 CHANGELOG.md 的**结构化解析结果**（版本 → 分类 → 条目 HTML）。
//! 内容随二进制版本固定，只受 SSR 页面缓存 TTL 约束，无需任何主动失效。
//!
//! # 布局
//! 左侧 200px 版本目录卡片（独立滚动、两端渐隐、滑动胶囊标出当前版本、阅读位置），
//! 小屏改为横向版本条；右侧按版本分节，每节按新增 / 改进 / 修复等分组。
//! 目录的阅读跟随由 yggdrasil-core 的 `__initChangelogNav` 维护。

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::{lazy_route, LazyRoute};

use crate::api::changelog::{get_changelog, ChangeGroup, ChangelogData, VersionEntry};
use crate::components::skeletons::changelog_skeleton::ChangelogSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{Icon, IconName, StatusBadge};
use crate::pages::not_found::{NotFound, PageLoadFailed};
use crate::routes::Route;
use crate::utils::error::is_not_found;

/// 更新日志路由，对应 `/changelog`。
pub struct ChangelogRoute {
    changelog: Resource<Result<ChangelogData, ServerFnError>>,
}

#[lazy_route]
impl LazyRoute for ChangelogRoute {
    fn data() -> Self {
        Self {
            changelog: Resource::new(|| (), |_| get_changelog()),
        }
    }

    fn view(this: Self) -> AnyView {
        let changelog = this.changelog;
        view! {
            <Suspense fallback=|| {
                view! {
                    <DelayedSkeleton>
                        <ChangelogSkeleton/>
                    </DelayedSkeleton>
                }
            }>
                {move || Suspend::new(async move {
                    match changelog.await {
                        Ok(ChangelogData { versions }) => {
                            view! { <Changelog versions=versions/> }.into_any()
                        }
                        Err(err) if is_not_found(&err) => view! { <NotFound/> }.into_any(),
                        Err(_) => view! { <PageLoadFailed/> }.into_any(),
                    }
                })}
            </Suspense>
        }
        .into_any()
    }
}

#[component]
fn Changelog(versions: Vec<VersionEntry>) -> impl IntoView {
    let total = versions
        .iter()
        .filter(|v| v.version != "Unreleased")
        .count();
    // 最早一个有日期的版本：「2026.06 起」。
    let since = versions
        .iter()
        .rev()
        .find_map(|v| v.date.as_deref())
        .map(month_label);

    let nav = (versions.len() > 1).then(|| view! { <VersionNav versions=versions.clone()/> });
    let cards = versions
        .into_iter()
        .map(|version| view! { <VersionCard version=version/> })
        .collect_view();

    view! {
        <Title text="更新日志 · Yggdrasil"/>
        <div class="changelog">
            <ChangelogHeader total=total since=since/>
            <div class="changelog-layout">
                {nav} <div class="changelog-releases">{cards}</div>
            </div>
        </div>
    }
}

/// 「2026-06-02」→「2026.06」。
fn month_label(date: &str) -> String {
    date.get(..7).unwrap_or(date).replace('-', ".")
}

/// 「2026-10-20」→「2026.10.20」。
fn date_label(date: &str) -> String {
    date.replace('-', ".")
}

/// 页头：返回关于、标题、说明与版本数。骨架屏复用，数据到达前后不跳动。
///
/// `total` / `since` 缺省时不渲染版本数行（骨架屏）；可传值、`Option` 或信号。
#[component]
pub fn ChangelogHeader(
    #[prop(optional, into)] total: MaybeProp<usize>,
    #[prop(optional, into)] since: MaybeProp<String>,
) -> impl IntoView {
    view! {
        <a class="changelog-back" href=Route::About.to_string()>
            <Icon name=IconName::ArrowLeft size=14/>
            "关于"
        </a>
        <header class="changelog-header page-enter">
            <div>
                <h1 class="changelog-title">"更新日志"</h1>
                <p class="changelog-intro">"每个版本新增、改进和修复了什么。"</p>
            </div>
            {move || {
                total
                    .get()
                    .map(|total| {
                        view! {
                            <p class="changelog-count tabular-nums">
                                {format!("{total} 个版本")}
                                {move || since.get().map(|since| format!(" · {since} 起"))}
                            </p>
                        }
                    })
            }}
        </header>
    }
}

/// 有界版本目录：桌面纵向滚动，小屏横向滚动。
/// 浏览器增强负责阅读位置、滑动指示器和目录自身滚动，卸载时清理监听。
#[component]
fn VersionNav(versions: Vec<VersionEntry>) -> impl IntoView {
    #[cfg(target_arch = "wasm32")]
    {
        Effect::new(move |_| {
            if let Some(window) = web_sys::window() {
                crate::utils::js::invoke_optional_global(&window, "__initChangelogNav", &[]);
            }
        });
        on_cleanup(move || {
            if let Some(window) = web_sys::window() {
                crate::utils::js::invoke_optional_global(&window, "__disposeChangelogNav", &[]);
            }
        });
    }

    let count = versions.len();
    let progress = 1.0 / count as f64;
    let items = versions
        .iter()
        .enumerate()
        .map(|(index, v)| {
            view! {
                <VersionNavItem
                    version=v.version.clone()
                    date=v.date.as_deref().map(|d| d.get(5..).unwrap_or(d).replace('-', "."))
                    active=index == 0
                />
            }
        })
        .collect_view();

    view! {
        // P5 §8.2：changelog 导航增强在 yggdrasil-enhance.js，按需注入。SSR 直达烤进
        // head 带 defer，先于 hydration；SPA 首跳由增强调用队列兜底。
        <crate::components::enhance_assets::EnhanceScript/>
        <nav class="changelog-nav" aria-label="版本目录">
            <div class="changelog-nav-panel" style=format!("--nav-progress: {progress}")>
                <div class="changelog-nav-head">
                    <span>"版本"</span>
                    <span class="tabular-nums" data-nav-position="">
                        {format!("01 / {count:02}")}
                    </span>
                </div>
                <div class="changelog-nav-scroll" tabindex="0" aria-label="滚动浏览所有版本">
                    <div class="changelog-nav-items">
                        <div class="changelog-nav-indicator" aria-hidden="true"></div>
                        {items}
                    </div>
                </div>
                <div class="changelog-nav-progress" aria-hidden="true">
                    <span></span>
                </div>
            </div>
        </nav>
    }
}

/// 原生锚点链接，SSR 和未水合时也能导航。
#[component]
fn VersionNavItem(version: String, date: Option<String>, active: bool) -> impl IntoView {
    let href = format!("#v{version}");
    view! {
        <a
            href=href
            class="changelog-nav-link"
            aria-current=active.then_some("location")
        >
            <span class="changelog-nav-label">{version}</span>
            {date.map(|date| view! { <span class="changelog-nav-date tabular-nums">{date}</span> })}
        </a>
    }
}

/// 一个版本：等宽版本号、日期与「最新」，intro（如有），再按分类分组列出条目。
#[component]
fn VersionCard(version: VersionEntry) -> impl IntoView {
    let VersionEntry {
        version: ver,
        date,
        is_latest,
        intro_html,
        groups,
    } = version;

    let date = date.map(|d| {
        view! {
            <time class="changelog-version-date tabular-nums" datetime=d.clone()>
                {date_label(&d)}
            </time>
        }
    });
    let latest = is_latest.then(|| view! { <StatusBadge color_class="badge-clay" label="最新"/> });
    let intro = (!intro_html.is_empty())
        .then(|| view! { <div class="changelog-intro-html" inner_html=intro_html></div> });
    let groups = groups
        .into_iter()
        .map(|group| view! { <ChangeGroupView group=group/> })
        .collect_view();

    view! {
        <section id=format!("v{ver}") class="changelog-version" aria-labelledby=format!("v{ver}-title")>
            <div class="changelog-version-head">
                <h2 id=format!("v{ver}-title") class="changelog-version-title">
                    {ver.clone()}
                </h2>
                {date} {latest}
            </div>
            {intro} {groups}
        </section>
    }
}

/// 单个分类组：分类小标签与条目列表。新增用陶土底，其余为中性灰。
#[component]
fn ChangeGroupView(group: ChangeGroup) -> impl IntoView {
    let ChangeGroup {
        category,
        items_html,
    } = group;

    view! {
        <div class="changelog-group">
            <h3 class="changelog-category" data-category=category.css_class()>
                {category.label()}
            </h3>
            <div class="changelog-items" inner_html=items_html></div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_use_dots() {
        assert_eq!(month_label("2026-06-02"), "2026.06");
        assert_eq!(date_label("2026-10-20"), "2026.10.20");
    }
}
