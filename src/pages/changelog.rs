//! 更新日志页面模块。
//!
//! 对应路由 `/changelog`。
//!
//! 数据获取：`use_server_future` 调用 `get_changelog` server function，取回
//! 编译期内嵌 CHANGELOG.md 的**结构化解析结果**（版本 → 分类 → 条目 HTML）。
//! 内容随二进制版本固定，只受 SSR 页面缓存 TTL 约束，无需任何主动失效。
//! 页面无路由参数、future 不会重跑，因此无需 `router().current()` 订阅
//! （该陷阱详见 `post_detail.rs` 头文档）。
//!
//! # 布局
//! 左侧 200px 版本目录卡片（独立滚动、两端渐隐、滑动胶囊标出当前版本、阅读位置），
//! 小屏改为横向版本条；右侧按版本分节，每节按新增 / 改进 / 修复等分组。
//! 目录的阅读跟随由 yggdrasil-core 的 `__initChangelogNav` 维护。

use dioxus::prelude::*;

use crate::api::changelog::{get_changelog, ChangeGroup, ChangelogData, VersionEntry};
use crate::components::skeletons::changelog_skeleton::ChangelogSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;
use crate::components::ui::{Icon, IconName};
use crate::router::Route;

/// 更新日志页面组件，对应路由 `/changelog`。
#[component]
pub fn Changelog() -> Element {
    let response = use_server_future(get_changelog)?;

    // 与 post_detail 同一约定：None（加载中）→ 骨架屏；Err → 抛给错误边界。
    let data = response.read().as_ref().map(|r| match r {
        Ok(resp) => Ok(resp.clone()),
        Err(e) => Err(e.clone()),
    });

    let ChangelogData { versions } = match data {
        Some(Ok(resp)) => resp,
        Some(Err(err)) => return Err(err.into()),
        None => {
            return rsx! {
                DelayedSkeleton { ChangelogSkeleton {} }
            };
        }
    };

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

    rsx! {
        document::Title { "更新日志 · Yggdrasil" }
        div { class: "changelog",
            ChangelogHeader { total: Some(total), since }
            div { class: "changelog-layout",
                if versions.len() > 1 {
                    VersionNav { versions: versions.clone() }
                }
                div { class: "changelog-releases",
                    for v in versions.iter() {
                        VersionCard { key: "{v.version}", version: v.clone() }
                    }
                }
            }
        }
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
#[component]
pub fn ChangelogHeader(
    #[props(default)] total: Option<usize>,
    #[props(default)] since: Option<String>,
) -> Element {
    rsx! {
        Link { class: "changelog-back", to: Route::About {},
            Icon { name: IconName::ArrowLeft, size: 14 }
            "关于"
        }
        header { class: "changelog-header page-enter",
            div {
                h1 { class: "changelog-title", "更新日志" }
                p { class: "changelog-intro", "每个版本新增、改进和修复了什么。" }
            }
            if let Some(total) = total {
                p { class: "changelog-count tabular-nums",
                    "{total} 个版本"
                    if let Some(since) = since {
                        " · {since} 起"
                    }
                }
            }
        }
    }
}

/// 有界版本目录：桌面纵向滚动，小屏横向滚动。
/// 浏览器增强负责阅读位置、滑动指示器和目录自身滚动，卸载时清理监听。
#[component]
fn VersionNav(versions: Vec<VersionEntry>) -> Element {
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        if let Some(window) = web_sys::window() {
            crate::utils::js::invoke_optional_global(&window, "__initChangelogNav", &[]);
        }
    });

    #[cfg(target_arch = "wasm32")]
    use_drop(move || {
        if let Some(window) = web_sys::window() {
            crate::utils::js::invoke_optional_global(&window, "__disposeChangelogNav", &[]);
        }
    });

    let count = versions.len();
    let progress = 1.0 / count as f64;

    rsx! {
        nav { class: "changelog-nav", aria_label: "版本目录",
            div { class: "changelog-nav-panel", style: "--nav-progress: {progress}",
                div { class: "changelog-nav-head",
                    span { "版本" }
                    span { class: "tabular-nums", "data-nav-position": "", "01 / {count:02}" }
                }
                div { class: "changelog-nav-scroll", tabindex: "0", aria_label: "滚动浏览所有版本",
                    div { class: "changelog-nav-items",
                        div { class: "changelog-nav-indicator", aria_hidden: "true" }
                        for (index, v) in versions.iter().enumerate() {
                            VersionNavItem {
                                key: "{v.version}",
                                version: v.version.clone(),
                                date: v.date.as_deref().map(|d| d.get(5..).unwrap_or(d).replace('-', ".")),
                                active: index == 0,
                            }
                        }
                    }
                }
                div { class: "changelog-nav-progress", aria_hidden: "true", span {} }
            }
        }
    }
}

/// 原生锚点链接，SSR 和未水合时也能导航。
#[component]
fn VersionNavItem(version: String, date: Option<String>, active: bool) -> Element {
    rsx! {
        a {
            href: "#v{version}",
            class: "changelog-nav-link",
            aria_current: active.then_some("location"),
            span { class: "changelog-nav-label", "{version}" }
            if let Some(date) = date {
                span { class: "changelog-nav-date tabular-nums", "{date}" }
            }
        }
    }
}

/// 一个版本：等宽版本号、日期与「最新」，intro（如有），再按分类分组列出条目。
#[component]
fn VersionCard(version: VersionEntry) -> Element {
    let VersionEntry {
        version: ver,
        date,
        is_latest,
        intro_html,
        groups,
    } = version;

    rsx! {
        section { id: "v{ver}", class: "changelog-version", aria_labelledby: "v{ver}-title",
            div { class: "changelog-version-head",
                h2 { id: "v{ver}-title", class: "changelog-version-title", "{ver}" }
                if let Some(d) = &date {
                    time { class: "changelog-version-date tabular-nums", datetime: "{d}", {date_label(d)} }
                }
                if is_latest {
                    span { class: "badge badge-clay", "最新" }
                }
            }
            if !intro_html.is_empty() {
                div { class: "changelog-intro-html", dangerous_inner_html: "{intro_html}" }
            }
            for group in groups.iter() {
                ChangeGroupView { key: "{group.category:?}", group: group.clone() }
            }
        }
    }
}

/// 单个分类组：分类小标签与条目列表。新增用陶土底，其余为中性灰。
#[component]
fn ChangeGroupView(group: ChangeGroup) -> Element {
    let ChangeGroup {
        category,
        items_html,
    } = group;

    rsx! {
        div { class: "changelog-group",
            h3 { class: "changelog-category", "data-category": category.css_class(), "{category.label()}" }
            div { class: "changelog-items", dangerous_inner_html: "{items_html}" }
        }
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
