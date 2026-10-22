//! 顶部导航栏组件
//!
//! 文字标、栏目导航（当前项为浮起的胶囊）与右侧操作区；760px 以下只留文字标、
//! 搜索与菜单按钮，菜单为底部抽屉（栏目、外观与次要链接）。样式见
//! styles/components/shell.css。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::components::ui::{use_sheet_close, BottomSheet, Icon, IconName};
use crate::router::Route;
use crate::theme::ThemeToggle;

/// 手机导航抽屉的 DOM ID；预载回归脚本按 `aria-controls` 定位菜单按钮。
const MENU_ID: &str = "mobile-nav-menu";

/// 导航项配置，用于描述 Header 中的一个链接。
///
/// 字段：
/// - `route`：目标路由
/// - `label`：显示文本
/// - `is_active`：当前是否处于激活状态
#[derive(Clone, PartialEq)]
pub struct NavItemConfig {
    /// 目标路由。
    pub route: Route,
    /// 显示文本。
    pub label: &'static str,
    /// 当前是否处于激活状态。
    pub is_active: bool,
}

/// 顶部导航栏组件。
///
/// Props：
/// - `nav_items`：导航项列表
/// - `right_content`：右侧自定义内容（如搜索入口、主题切换）
/// - `max_width`：内部导航的宽度类，需与正文 `max-w-*` 一致以保证左右边缘对齐
#[component]
pub fn Header(
    nav_items: Vec<NavItemConfig>,
    right_content: Element,
    #[props(default = "max-w-3xl")] max_width: &'static str,
) -> Element {
    let mut sheet_open = use_signal(|| false);
    let menu_button_id = format!("{MENU_ID}-trigger");
    let palette = crate::components::search_palette::use_search_palette();

    rsx! {
        header { "data-vt-shell": "frontend-header", class: "site-header",
            div { class: "site-header-inner {max_width}",
                Link { class: "wordmark", to: Route::Home {},
                    span { class: "wordmark-mark", aria_hidden: "true" }
                    "Yggdrasil"
                }
                nav { class: "site-nav", aria_label: "主导航",
                    for item in nav_items.iter().cloned() {
                        NavItem {
                            key: "{item.label}",
                            route: item.route,
                            label: item.label,
                            is_active: item.is_active,
                        }
                    }
                }
                div { class: "site-actions",
                    {right_content}
                    if let Some(palette) = palette {
                        button {
                            class: "touch-icon",
                            r#type: "button",
                            aria_label: "搜索",
                            aria_haspopup: "dialog",
                            onclick: move |_| palette.open(),
                            Icon { name: IconName::Search, size: 19 }
                        }
                    } else {
                        Link { class: "touch-icon", to: Route::Search { q: String::new() }, aria_label: "搜索",
                            Icon { name: IconName::Search, size: 19 }
                        }
                    }
                    button {
                        id: "{menu_button_id}",
                        class: "touch-icon",
                        r#type: "button",
                        aria_label: "打开导航",
                        aria_haspopup: "dialog",
                        aria_expanded: "{sheet_open()}",
                        aria_controls: MENU_ID,
                        onclick: move |_| sheet_open.set(true),
                        Icon { name: IconName::Menu, size: 19 }
                    }
                }
            }
        }
        NavSheet {
            open: sheet_open,
            id: MENU_ID,
            trigger_id: menu_button_id.clone(),
            nav_items,
        }
    }
}

/// 单个桌面导航项：当前项内含 `.nav-indicator` 胶囊（转场时只有胶囊在栏目间滑动，文字留在原位）。
#[component]
fn NavItem(route: Route, label: &'static str, is_active: bool) -> Element {
    rsx! {
        Link {
            class: "nav-item",
            to: route,
            aria_current: is_active.then_some("page"),
            if is_active {
                span { class: "nav-indicator", aria_hidden: "true" }
            }
            "{label}"
        }
    }
}

/// 手机导航抽屉：底部面板，含栏目、外观分段控件与次要链接。选中栏目或链接后关闭。
#[component]
fn NavSheet(
    open: Signal<bool>,
    id: &'static str,
    trigger_id: String,
    nav_items: Vec<NavItemConfig>,
) -> Element {
    rsx! {
        BottomSheet { open, id, trigger_id, label: "导航",
            NavSheetBody { nav_items }
        }
    }
}

#[component]
fn NavSheetBody(nav_items: Vec<NavItemConfig>) -> Element {
    let sheet = use_sheet_close();
    rsx! {
        nav { aria_label: "栏目",
            ul { class: "flex flex-col gap-0.5",
                for item in nav_items {
                    li { key: "{item.label}",
                        SheetNavItem {
                            route: item.route,
                            label: item.label,
                            is_active: item.is_active,
                            on_navigate: move |_| sheet.close(),
                        }
                    }
                }
            }
        }
        div { class: "my-3 mx-1 h-px bg-hairline" }
        div { class: "sheet-row",
            span { "外观" }
            ThemeToggle { labels: true }
        }
        div { class: "sheet-links",
            a { href: "/feed.xml", "RSS" }
            Link { to: Route::Changelog {}, onclick: move |_| sheet.close(), "更新日志" }
            Link { to: Route::WritingGuide {}, onclick: move |_| sheet.close(), "写作指南" }
        }
    }
}

/// 抽屉中的栏目链接（48px 触控目标），点击后关闭抽屉。
#[component]
fn SheetNavItem(
    route: Route,
    label: &'static str,
    is_active: bool,
    on_navigate: EventHandler<()>,
) -> Element {
    rsx! {
        Link {
            class: "sheet-nav-item",
            to: route,
            aria_current: is_active.then_some("page"),
            onclick: move |_| on_navigate.call(()),
            "{label}"
        }
    }
}

/// 顶部导航栏右侧的搜索入口：「搜索 ⌘K」小胶囊。
///
/// 默认跳转搜索页并随路由感知激活状态；`active` 可显式覆盖，`on_click` 传入时
/// 拦截默认跳转，用于打开搜索面板或图鉴演示。
#[component]
pub fn SearchIconLink(
    /// 可选显式覆盖激活状态。默认为随全局路由自动感知 (`Route::Search`).
    #[props(default)]
    active: Option<bool>,
    /// 可选点击事件回调。若提供，将拦截默认路由跳转。
    #[props(default)]
    on_click: Option<EventHandler<MouseEvent>>,
) -> Element {
    let route_active = matches!(use_route::<Route>(), Route::Search { .. });
    let is_active = active.unwrap_or(route_active);
    rsx! {
        Link {
            class: "search-trigger",
            to: Route::Search { q: String::new() },
            aria_current: is_active.then_some("page"),
            aria_label: "搜索",
            aria_keyshortcuts: "Meta+K Control+K",
            // 传入 on_click 时只执行回调；Link 自身的 prevent_default 拦不住它内部的跳转。
            onclick: on_click,
            onclick_only: true,
            Icon { name: IconName::Search, size: 15 }
            span { "搜索" }
            kbd { "⌘K" }
        }
    }
}
