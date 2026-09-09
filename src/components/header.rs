//! 顶部导航栏组件
//!
//! 提供站点 Logo、响应式导航菜单项与右侧自定义内容区，
//! 支持前台布局与后台布局复用，并包含小屏幕下的汉堡菜单。

use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::router::Route;

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
/// - `right_content`：右侧自定义内容（如主题切换、登出按钮）
/// - `max_width`：内部导航的宽度类，需与正文 `max-w-*` 一致以保证左右边缘对齐。
///   默认 `max-w-3xl`（前台阅读宽度）；后台传 `max-w-5xl` 与之同宽。
#[component]
pub fn Header(
    nav_items: Vec<NavItemConfig>,
    right_content: Element,
    #[props(default = "max-w-3xl")] max_width: &'static str,
) -> Element {
    let mut mobile_open = use_signal(|| false);
    // D12：对常量字符串做 use_memo 是无谓的 memo+String 分配，改用 &'static str。
    let menu_id: &str = "mobile-nav-menu";

    let is_open = mobile_open();
    let burger_icon_class = if is_open {
        "w-6 h-6 absolute transition-all duration-300 transform rotate-90 opacity-0 scale-75"
    } else {
        "w-6 h-6 absolute transition-all duration-300 transform rotate-0 opacity-100 scale-100"
    };
    let close_icon_class = if is_open {
        "w-6 h-6 absolute transition-all duration-300 transform rotate-0 opacity-100 scale-100"
    } else {
        "w-6 h-6 absolute transition-all duration-300 transform -rotate-90 opacity-0 scale-75"
    };
    let panel_class = if is_open {
        "mobile-nav-panel md:hidden bg-paper-theme/95 backdrop-blur-sm is-open"
    } else {
        "mobile-nav-panel md:hidden bg-paper-theme/95 backdrop-blur-sm"
    };
    rsx! {
        header { "data-vt-shell": "frontend-header", class: "sticky top-0 z-40 w-full bg-[var(--color-paper-theme)]/70 backdrop-blur-md transition-all duration-300",
            nav { class: "{max_width} mx-auto px-6 h-16 flex items-center justify-between",
                Link {
                    class: "text-2xl font-extrabold tracking-tight text-[var(--color-paper-primary)] hover:text-[var(--color-paper-accent)] transition-colors duration-200",
                    to: Route::Home {},
                    "Yggdrasil"
                }
                div { class: "flex items-center gap-2",
                    // 桌面端导航
                    ul { class: "hidden md:flex items-center gap-1",
                        for item in nav_items.iter().cloned() {
                            NavItem {
                                key: "{item.label}",
                                route: item.route,
                                label: item.label,
                                is_active: item.is_active,
                            }
                        }
                    }

                    {right_content}

                    // 移动端汉堡菜单按钮
                    button {
                        class: "md:hidden p-2 rounded-lg text-paper-secondary hover:text-paper-primary hover:bg-paper-entry transition-colors relative flex items-center justify-center w-10 h-10 overflow-hidden",
                        r#type: "button",
                        aria_label: if is_open { "关闭导航菜单" } else { "打开导航菜单" },
                        aria_expanded: is_open,
                        aria_controls: menu_id,
                        onclick: move |_| mobile_open.set(!mobile_open()),
                        // 汉堡图标
                        svg {
                            class: "{burger_icon_class}",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M4 6h16M4 12h16M4 18h16",
                            }
                        }
                        // 关闭图标（X）
                        svg {
                            class: "{close_icon_class}",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M6 18L18 6M6 6l12 12",
                            }
                        }
                    }
                }
            }

            // 移动端导航面板（常驻 DOM 以支持展开/折叠双向平滑动画）
            div { id: menu_id, class: "{panel_class}",
                div { class: "mobile-nav-content",
                    ul { class: "py-2 px-6 space-y-1",
                        for item in nav_items.iter().cloned() {
                            li { key: "{item.label}", class: "mobile-nav-item",
                                MobileNavItem {
                                    route: item.route,
                                    label: item.label,
                                    is_active: item.is_active,
                                    on_navigate: move |_| mobile_open.set(false),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 单个桌面导航项组件，根据 `is_active` 切换高亮样式。
#[component]
fn NavItem(route: Route, label: &'static str, is_active: bool) -> Element {
    let base_class =
        "relative inline-flex px-3 py-1 text-base rounded-lg transition-colors duration-200";
    let class_str = if is_active {
        format!("{} font-medium text-paper-accent", base_class)
    } else {
        format!(
            "{} text-paper-secondary hover:text-paper-primary",
            base_class
        )
    };

    rsx! {
        li {
            Link {
                class: "{class_str}",
                to: route,
                aria_current: is_active.then_some("page"),
                "{label}"
                if is_active {
                    span { class: "nav-indicator", "aria-hidden": "true" }
                }
            }
        }
    }
}

/// 单个移动端导航项组件，点击后关闭菜单。
#[component]
fn MobileNavItem(
    route: Route,
    label: &'static str,
    is_active: bool,
    on_navigate: EventHandler<()>,
) -> Element {
    let class_str = if is_active {
        "block w-full px-3 py-2 text-base font-medium text-paper-accent rounded-lg bg-paper-entry transition-colors"
    } else {
        "block w-full px-3 py-2 text-base text-paper-secondary hover:text-paper-primary hover:bg-paper-entry rounded-lg transition-colors"
    };

    rsx! {
        Link {
            class: "{class_str}",
            to: route,
            aria_current: is_active.then_some("page"),
            onclick: move |_| on_navigate.call(()),
            "{label}"
        }
    }
}

/// 搜索图标链接：置于 Header 右侧（主题切换左边），点击跳转搜索页。
///
/// 样式与 `ThemeToggle` 对齐（圆形 padding + currentColor 图标），保持右侧
/// 图标组视觉一致。SVG 来自 `public/icons/search_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg`，
/// 改用 `fill: "currentColor"` 以适配明暗主题。
#[component]
pub fn SearchIconLink() -> Element {
    let is_active = matches!(use_route::<Route>(), Route::Search {});
    let color = if is_active {
        "text-paper-accent"
    } else {
        "text-paper-secondary hover:text-paper-accent"
    };
    rsx! {
        Link {
            class: "relative p-2 rounded-full {color} transition-colors duration-200",
            to: Route::Search {},
            aria_current: is_active.then_some("page"),
            aria_label: "搜索",
            title: "搜索",
            svg {
                xmlns: "http://www.w3.org/2000/svg",
                height: "24px",
                view_box: "0 -960 960 960",
                width: "24px",
                fill: "currentColor",
                path { d: "M784-120 532-372q-30 24-69 38t-83 14q-109 0-184.5-75.5T120-580q0-109 75.5-184.5T380-840q109 0 184.5 75.5T640-580q0 44-14 83t-38 69l252 252-56 56ZM380-400q75 0 127.5-52.5T560-580q0-75-52.5-127.5T380-760q-75 0-127.5 52.5T200-580q0 75 52.5 127.5T380-400Z" }
            }
            if is_active {
                span { class: "nav-indicator hidden md:block", "aria-hidden": "true" }
            }
        }
    }
}
