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
//! 双栏：左侧 sticky 版本导航（桌面端）+ 右侧版本卡片列表。
//! 每个版本卡片内按分类（Added / Fixed / Security …）分组，每组带色标 badge。
//! 配色遵循全站 Catppuccin 双强调色约束（详见 `changelog.rs` 模块文档）。

use dioxus::prelude::*;

use crate::api::changelog::{get_changelog, ChangeGroup, ChangelogData, VersionEntry};
use crate::components::skeletons::changelog_skeleton::ChangelogSkeleton;
use crate::components::skeletons::delayed_skeleton::DelayedSkeleton;

/// 更新日志页面组件，对应路由 `/changelog`。
///
/// 结构：页头 → 统计栏 → 双栏（版本导航 + 版本卡片列表）。
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
    let latest = versions.iter().find(|v| v.is_latest);

    rsx! {
        div { class: "animate-page-enter",
            header { class: "page-header mb-6",
                h1 { class: "text-4xl font-bold text-paper-primary tracking-tight",
                    "更新日志"
                }
            }

            // 统计栏：最新版本 + 版本总数。镜像 post-meta 的安静感。
            div { class: "flex flex-wrap items-center gap-x-4 gap-y-1 mb-8 text-sm text-paper-tertiary",
                if let Some(v) = latest {
                    div { class: "flex items-center gap-1.5",
                        span { class: "w-2 h-2 rounded-full bg-[var(--color-paper-accent)]" }
                        span { "最新版本 " }
                        span { class: "font-medium text-paper-primary", "v{v.version}" }
                    }
                }
                span { "共 {total} 个版本" }
            }

            // 双栏布局
            div { class: "changelog-layout",
                // 有界侧栏 / 小屏横向版本条，共用 scroll-spy。
                if versions.len() > 1 {
                    VersionNav { versions: versions.clone() }
                }

                // 版本卡片列表
                div { class: "flex-1 min-w-0 space-y-6",
                    for v in versions.iter() {
                        VersionCard { key: "{v.version}", version: v.clone() }
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
        nav { class: "changelog-nav", aria_label: "更新日志版本导航",
            div { class: "changelog-nav-panel", style: "--nav-progress: {progress}",
                div { class: "changelog-nav-head",
                    span { "版本导航" }
                    span { class: "changelog-nav-count", "{count}" }
                }
                div { class: "changelog-nav-scroll", tabindex: "0", aria_label: "滚动浏览所有版本",
                    div { class: "changelog-nav-items",
                        div { class: "changelog-nav-indicator", aria_hidden: "true" }
                        for (index, v) in versions.iter().enumerate() {
                            VersionNavItem {
                                key: "{v.version}",
                                version: v.version.clone(),
                                active: index == 0,
                                latest: v.is_latest,
                            }
                        }
                    }
                }
                div { class: "changelog-nav-foot", aria_hidden: "true",
                    div { class: "changelog-nav-position",
                        span { "阅读位置" }
                        span { "data-nav-position": "", "01 / {count:02}" }
                    }
                    div { class: "changelog-nav-progress", span {} }
                }
            }
        }
    }
}

/// 原生锚点链接，SSR 和未水合时也能导航。
#[component]
fn VersionNavItem(version: String, active: bool, latest: bool) -> Element {
    rsx! {
        a {
            href: "#v{version}",
            class: "changelog-nav-link",
            aria_current: active.then_some("location"),
            span { class: "changelog-nav-dot", aria_hidden: "true" }
            span { class: "changelog-nav-label", "{version}" }
            if latest {
                span { class: "changelog-nav-latest", "最新" }
            }
        }
    }
}

/// 单个版本卡片。
///
/// 结构：版本头（版本号 + 最新标记 + 日期）→ intro（如有）→ 分类组列表。
/// 每个分类组带色标 badge + 条目列表。
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
        article {
            id: "v{ver}",
            class: "changelog-version rounded-[2rem] bg-[var(--color-paper-entry)] border border-transparent hover:border-[var(--color-paper-border)] transition-colors p-6 md:p-8",

            // 版本头
            div { class: "flex items-baseline gap-3",
                h2 { class: "text-xl md:text-2xl font-bold text-paper-primary tracking-tight",
                    "v{ver}"
                }
                if is_latest {
                    span { class: "changelog-badge changelog-badge--latest", "最新" }
                }
                if let Some(d) = &date {
                    span { class: "text-sm text-paper-tertiary ml-auto", "{d}" }
                }
            }

            // intro（Unreleased 占位文字等）
            if !intro_html.is_empty() {
                div {
                    class: "md-content text-sm text-paper-secondary mt-2",
                    dangerous_inner_html: "{intro_html}",
                }
            }

            // 分类组
            if !groups.is_empty() {
                div { class: "mt-4 space-y-5",
                    for group in groups.iter() {
                        ChangeGroupView { key: "{group.category:?}", group: group.clone() }
                    }
                }
            }
        }
    }
}

/// 单个分类组视图：badge + 条目列表。
#[component]
fn ChangeGroupView(group: ChangeGroup) -> Element {
    let ChangeGroup {
        category,
        items_html,
    } = group;
    let badge_class = format!("changelog-badge changelog-badge--{}", category.css_class());

    rsx! {
        div {
            // badge 行
            div { class: "mb-2",
                span { class: "{badge_class}", "{category.label()}" }
            }
            // 条目列表（复用 md-content 内联格式 + changelog-items 列表样式覆盖）
            div {
                class: "md-content changelog-items",
                dangerous_inner_html: "{items_html}",
            }
        }
    }
}
