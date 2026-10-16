//! 顶部导航栏组件
//!
//! 文字标、栏目导航（当前项为浮起的胶囊）与右侧操作区；760px 以下只留文字标、
//! 搜索与菜单按钮，菜单为底部抽屉（栏目、外观与次要链接）。样式见
//! styles/components/shell.css。

use dioxus::html::InteractionLocation;
use dioxus::prelude::*;
use dioxus::router::components::Link;

use crate::components::ui::{Icon, IconName};
use crate::router::Route;
use crate::theme::ThemeToggle;

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

/// 抽屉关闭动画时长（ms），与 shell.css 的 `sheet-out` 一致。
const SHEET_EXIT_MS: u32 = 200;
/// 拖拽死区：手指移动超过 4px 才开始跟手，避免轻点误拖。
const DRAG_DEAD_ZONE: f64 = 4.0;
/// 向下拖过该距离松手即关闭。
const DRAG_CLOSE_DISTANCE: f64 = 80.0;

/// 拖拽位移：死区内为 0，向上拖不跟手（抽屉已贴底）。
fn drag_offset(start_y: f64, current_y: f64) -> f64 {
    let dy = current_y - start_y;
    if dy <= DRAG_DEAD_ZONE {
        0.0
    } else {
        dy - DRAG_DEAD_ZONE
    }
}

/// 顶部导航栏组件。
///
/// Props：
/// - `nav_items`：导航项列表
/// - `right_content`：右侧自定义内容（如搜索入口、主题切换）
/// - `max_width`：内部导航的宽度类，需与正文 `max-w-*` 一致以保证左右边缘对齐
/// - `menu_id`：手机抽屉的稳定 DOM ID；同页有多个 Header 时必须各不相同
/// - `on_demo_navigate`：图鉴用局部导航回调；正式页面省略以保留 Link 行为
/// - `page_shell`：图鉴实例设 false，避免被页面过渡系统识别为真实站点外壳
#[component]
pub fn Header(
    nav_items: Vec<NavItemConfig>,
    right_content: Element,
    #[props(default = "max-w-3xl")] max_width: &'static str,
    #[props(default = "mobile-nav-menu")] menu_id: &'static str,
    #[props(default)] on_demo_navigate: Option<EventHandler<&'static str>>,
    #[props(default = true)] page_shell: bool,
) -> Element {
    let mut sheet_open = use_signal(|| false);
    let menu_button_id = format!("{menu_id}-trigger");

    rsx! {
        header { "data-vt-shell": page_shell.then_some("frontend-header"), class: "site-header",
            div { class: "site-header-inner {max_width}",
                if let Some(on_navigate) = on_demo_navigate {
                    button {
                        class: "wordmark",
                        r#type: "button",
                        onclick: move |_| on_navigate.call("首页"),
                        span { class: "wordmark-mark", aria_hidden: "true" }
                        "Yggdrasil"
                    }
                } else {
                    Link { class: "wordmark", to: Route::Home {},
                        span { class: "wordmark-mark", aria_hidden: "true" }
                        "Yggdrasil"
                    }
                }
                nav { class: "site-nav", aria_label: "主导航",
                    for item in nav_items.iter().cloned() {
                        NavItem {
                            key: "{item.label}",
                            route: item.route,
                            label: item.label,
                            is_active: item.is_active,
                            on_demo_navigate,
                        }
                    }
                }
                div { class: "site-actions",
                    {right_content}
                    if on_demo_navigate.is_none() {
                        Link { class: "touch-icon", to: Route::Search {}, aria_label: "搜索",
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
                        aria_controls: menu_id,
                        onclick: move |_| sheet_open.set(true),
                        Icon { name: IconName::Menu, size: 19 }
                    }
                }
            }
        }
        NavSheet {
            open: sheet_open,
            id: menu_id,
            trigger_id: menu_button_id.clone(),
            nav_items,
            on_demo_navigate,
        }
    }
}

/// 单个桌面导航项：当前项带 `.nav-indicator`（浮起胶囊，转场时在栏目间滑动）。
#[component]
fn NavItem(
    route: Route,
    label: &'static str,
    is_active: bool,
    on_demo_navigate: Option<EventHandler<&'static str>>,
) -> Element {
    let class = if is_active {
        "nav-item nav-indicator"
    } else {
        "nav-item"
    };
    rsx! {
        if let Some(on_navigate) = on_demo_navigate {
            button {
                class,
                r#type: "button",
                aria_current: is_active.then_some("page"),
                onclick: move |_| on_navigate.call(label),
                "{label}"
            }
        } else {
            Link { class, to: route, aria_current: is_active.then_some("page"), "{label}" }
        }
    }
}

/// 手机导航抽屉：底部面板，含栏目、外观分段控件与次要链接。
///
/// 从底部以 heavy 弹簧升起；点遮罩、按 Esc、选中栏目或向下拖过 80px 关闭。
/// 打开时聚焦当前栏目，关闭后焦点回到菜单按钮。
#[component]
fn NavSheet(
    open: Signal<bool>,
    id: &'static str,
    trigger_id: String,
    nav_items: Vec<NavItemConfig>,
    on_demo_navigate: Option<EventHandler<&'static str>>,
) -> Element {
    let mut open = open;
    let mut closing = use_signal(|| false);
    let mut drag_start = use_signal(|| None::<f64>);
    let mut drag_dy = use_signal(|| 0.0_f64);
    let mut was_open = use_signal(|| false);

    // 打开后聚焦当前栏目（没有当前栏目时聚焦第一项），并锁住页面滚动。
    use_effect(move || {
        let is_open = open();
        if is_open == *was_open.peek() {
            return;
        }
        was_open.set(is_open);
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            let Some(document) = web_sys::window().and_then(|w| w.document()) else {
                return;
            };
            if let Some(html) = document.document_element() {
                let _ = html.class_list().toggle_with_force("sheet-open", is_open);
            }
            if is_open {
                let selector = format!("#{id} [aria-current='page'], #{id} .sheet-nav-item");
                if let Some(el) = document
                    .query_selector(&selector)
                    .ok()
                    .flatten()
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                {
                    let _ = el.focus();
                }
            }
        }
    });

    let close = {
        let trigger_id = trigger_id.clone();
        move || {
            if !open() || closing() {
                return;
            }
            closing.set(true);
            drag_start.set(None);
            #[cfg(target_arch = "wasm32")]
            {
                use wasm_bindgen::JsCast;
                if let Some(el) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id(&trigger_id))
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                {
                    let _ = el.focus();
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            let _ = &trigger_id;
            spawn(async move {
                crate::utils::time::sleep_ms(SHEET_EXIT_MS).await;
                open.set(false);
                closing.set(false);
                drag_dy.set(0.0);
            });
        }
    };

    if !open() {
        return rsx! {};
    }

    let dy = drag_dy();
    let dragging = drag_start().is_some();
    let sheet_style = if dy > 0.0 {
        format!("transform: translateY({dy}px);")
    } else {
        String::new()
    };
    let mut close_scrim = close.clone();
    let mut close_key = close.clone();
    let mut close_drag = close.clone();
    let close_nav = close.clone();
    let close_links = close;

    rsx! {
        div {
            class: "sheet-scrim",
            "data-closing": closing().then_some("true"),
            onclick: move |_| close_scrim(),
        }
        nav {
            id,
            class: "sheet",
            role: "dialog",
            aria_modal: "true",
            aria_label: "导航",
            "data-closing": closing().then_some("true"),
            "data-dragging": dragging.then_some("true"),
            style: "{sheet_style}",
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    e.prevent_default();
                    close_key();
                }
            },
            button {
                id: "{id}-handle",
                class: "sheet-handle",
                r#type: "button",
                aria_label: "向下拖动或点按关闭",
                onpointerdown: move |e| {
                    drag_start.set(Some(e.client_coordinates().y));
                    // 捕获指针：手指移出把手后仍能收到移动与抬起事件。
                    #[cfg(target_arch = "wasm32")]
                    if let Some(el) = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.get_element_by_id(&format!("{id}-handle")))
                    {
                        let _ = el.set_pointer_capture(e.pointer_id());
                    }
                },
                onpointermove: move |e| {
                    if let Some(start) = drag_start() {
                        drag_dy.set(drag_offset(start, e.client_coordinates().y));
                    }
                },
                onpointerup: move |_| {
                    let moved = drag_dy();
                    drag_start.set(None);
                    if moved > DRAG_CLOSE_DISTANCE || moved == 0.0 {
                        close_drag();
                    } else {
                        drag_dy.set(0.0);
                    }
                },
                onpointercancel: move |_| {
                    drag_start.set(None);
                    drag_dy.set(0.0);
                },
            }
            ul { class: "flex flex-col gap-0.5",
                for item in nav_items {
                    li { key: "{item.label}",
                        SheetNavItem {
                            route: item.route,
                            label: item.label,
                            is_active: item.is_active,
                            on_demo_navigate,
                            on_navigate: {
                                let mut close = close_nav.clone();
                                move |_| close()
                            },
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
                Link {
                    to: Route::Changelog {},
                    onclick: {
                        let mut close = close_links.clone();
                        move |_| close()
                    },
                    "更新日志"
                }
                Link {
                    to: Route::WritingGuide {},
                    onclick: {
                        let mut close = close_links.clone();
                        move |_| close()
                    },
                    "写作指南"
                }
            }
        }
    }
}

/// 抽屉中的栏目链接（48px 触控目标），点击后关闭抽屉。
#[component]
fn SheetNavItem(
    route: Route,
    label: &'static str,
    is_active: bool,
    on_demo_navigate: Option<EventHandler<&'static str>>,
    on_navigate: EventHandler<()>,
) -> Element {
    rsx! {
        if let Some(demo_navigate) = on_demo_navigate {
            button {
                class: "sheet-nav-item",
                r#type: "button",
                aria_current: is_active.then_some("page"),
                onclick: move |_| {
                    demo_navigate.call(label);
                    on_navigate.call(());
                },
                "{label}"
            }
        } else {
            Link {
                class: "sheet-nav-item",
                to: route,
                aria_current: is_active.then_some("page"),
                onclick: move |_| on_navigate.call(()),
                "{label}"
            }
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
    let route_active = matches!(use_route::<Route>(), Route::Search {});
    let is_active = active.unwrap_or(route_active);
    rsx! {
        Link {
            class: "search-trigger",
            to: Route::Search {},
            aria_current: is_active.then_some("page"),
            aria_label: "搜索",
            aria_keyshortcuts: "Meta+K Control+K",
            onclick: move |e: MouseEvent| {
                if let Some(ref handler) = on_click {
                    e.prevent_default();
                    handler.call(e);
                }
            },
            Icon { name: IconName::Search, size: 15 }
            span { "搜索" }
            kbd { "⌘K" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::drag_offset;

    #[test]
    fn drag_offset_ignores_small_moves_and_upward_drags() {
        assert_eq!(drag_offset(100.0, 103.0), 0.0); // 死区内
        assert_eq!(drag_offset(100.0, 90.0), 0.0); // 向上
        assert_eq!(drag_offset(100.0, 150.0), 46.0); // 减去 4px 死区
    }
}
