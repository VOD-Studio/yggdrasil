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
            div { class: "flex gap-8",
                // 版本导航：桌面端 sticky 侧栏（scroll-spy 高亮当前版本）
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

/// 版本导航侧栏（桌面端 sticky）。
///
/// 高亮语义：accent 标记「当前视口顶部命中的版本卡片」（scroll-spy），而非固定
/// 标记最新版本——否则带 #hash 访问或点击导航跳转旧版本时，侧栏高亮仍停在最新
/// 版本上，与右侧内容区脱节。
#[component]
fn VersionNav(versions: Vec<VersionEntry>) -> Element {
    // 当前命中的版本号。None = SSR/首帧尚未计算，渲染时回退首项——页面顶端
    // 即最新版本，与修复前的常驻高亮表现一致，也无 hydration mismatch。
    let active = use_signal(|| None::<String>);

    // scroll-spy：window scroll 监听 + getBoundingClientRect 判定。一版本一张
    // 卡片，数量有限，每次事件十几次 rect 读取成本可忽略，无需 IntersectionObserver
    // 那套可见性集合管理。
    #[cfg(target_arch = "wasm32")]
    {
        use dioxus::prelude::{use_drop, use_effect, use_hook};
        use std::cell::RefCell;
        use std::rc::Rc;
        use wasm_bindgen::JsCast;

        let mut active = active;

        // 手写监听而非复用 hooks/event_listener.rs：那里 handler 只在事件触发时
        // 运行，这里挂载后要先算一次初始命中（直接带 #hash 访问时浏览器原生锚点
        // 滚动已完成，首帧即应高亮正确版本）。模式镜像 ui.rs 的 Escape 监听：
        // use_hook 持有 Closure，use_drop 移除监听防泄漏。
        type ScrollState = Rc<RefCell<Option<wasm_bindgen::prelude::Closure<dyn FnMut()>>>>;
        let state: ScrollState = use_hook(|| Rc::new(RefCell::new(None)));
        let state_for_effect = state.clone();
        let state_for_drop = state.clone();

        // effect 体内不读任何 signal（compute 里只有 peek/set），无依赖 → 只跑一次。
        use_effect(move || {
            let Some(window) = web_sys::window() else {
                return;
            };
            let Some(document) = window.document() else {
                return;
            };
            let Ok(list) = document.query_selector_all("article.changelog-version") else {
                return;
            };
            let mut cards: Vec<web_sys::Element> = Vec::with_capacity(list.length() as usize);
            for i in 0..list.length() {
                if let Some(el) = list
                    .item(i)
                    .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
                {
                    cards.push(el);
                }
            }
            if cards.is_empty() {
                return;
            }

            // compute 捕获 window 副本，原件留给下方注册监听。
            let window_for_compute = window.clone();
            let mut compute = move || {
                // 视口顶部判定线：sticky header + 卡片 scroll-mt-20（80px）落点余量。
                const THRESHOLD_PX: f64 = 120.0;

                // 顶部已越过判定线的最后一张卡片 = 当前阅读位置。
                // 卡片 id 为 "v{version}"，strip 掉渲染时加的前缀还原版本号。
                let mut current: Option<String> = None;
                for el in &cards {
                    if el.get_bounding_client_rect().top() <= THRESHOLD_PX {
                        current = el.id().strip_prefix('v').map(str::to_owned);
                    } else {
                        break;
                    }
                }
                // 页面顶端没有任何卡片越过判定线 → 回退首项（最新版本）。
                let mut ver = current.or_else(|| {
                    cards
                        .first()
                        .and_then(|e| e.id().strip_prefix('v').map(str::to_owned))
                });

                // 滚到页面底部 → 强制末项：末卡片内容短时永远到不了判定线。
                let at_bottom = window_for_compute
                    .inner_height()
                    .ok()
                    .and_then(|v| v.as_f64())
                    .zip(window_for_compute.scroll_y().ok())
                    .zip(
                        document
                            .document_element()
                            .map(|e| f64::from(e.scroll_height())),
                    )
                    .is_some_and(|((vh, sy), sh)| vh + sy >= sh - 4.0);
                if at_bottom {
                    ver = cards
                        .last()
                        .and_then(|e| e.id().strip_prefix('v').map(str::to_owned));
                }

                if let Some(ver) = ver {
                    if active.peek().as_deref() != Some(ver.as_str()) {
                        active.set(Some(ver));
                    }
                }
            };

            compute();

            let closure =
                wasm_bindgen::prelude::Closure::wrap(Box::new(compute) as Box<dyn FnMut()>);
            let _ =
                window.add_event_listener_with_callback("scroll", closure.as_ref().unchecked_ref());
            *state_for_effect.borrow_mut() = Some(closure);
        });

        use_drop(move || {
            if let Some(closure) = state_for_drop.borrow_mut().take() {
                if let Some(window) = web_sys::window() {
                    let _ = window.remove_event_listener_with_callback(
                        "scroll",
                        closure.as_ref().unchecked_ref(),
                    );
                }
            }
        });
    }

    // 回退首项：SSR 与 WASM 首帧一致（active 均为 None）。
    let fallback = versions.first().map(|v| v.version.clone());
    let active_ver = active.read().clone().or(fallback);

    rsx! {
        nav { class: "hidden lg:block w-40 shrink-0",
            div { class: "sticky top-20",
                for v in versions.iter() {
                    VersionNavItem {
                        key: "{v.version}",
                        version: v.version.clone(),
                        active: active_ver.as_deref() == Some(v.version.as_str()),
                    }
                }
            }
        }
    }
}

/// 版本导航侧栏中的单条链接。
///
/// `active` = scroll-spy 命中的当前版本（见 [`VersionNav`]），用 accent 高亮。
#[component]
fn VersionNavItem(version: String, active: bool) -> Element {
    let base = "group flex items-center gap-2 py-1.5 text-sm transition-colors";
    let text_class = if active {
        "font-medium text-paper-accent"
    } else {
        "text-paper-secondary group-hover:text-paper-primary"
    };
    let dot_class = if active {
        "bg-[var(--color-paper-accent)]"
    } else {
        "bg-[var(--color-paper-border)]"
    };

    rsx! {
        a { href: "#v{version}", class: "{base}",
            span { class: "w-1.5 h-1.5 rounded-full shrink-0 {dot_class}" }
            span { class: "{text_class}", "{version}" }
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
            class: "changelog-version scroll-mt-20 rounded-[2rem] bg-[var(--color-paper-entry)] border border-transparent hover:border-[var(--color-paper-border)] transition-colors p-6 md:p-8",

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
