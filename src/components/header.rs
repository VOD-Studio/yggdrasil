//! 顶部导航栏组件
//!
//! 文字标、栏目导航（当前项为浮起的胶囊）与右侧操作区；767px 以下只留文字标、
//! 搜索与菜单按钮，菜单为底部抽屉（栏目、外观与次要链接）。样式见
//! styles/components/shell.css。
//!
//! 链接一律用普通 `<a href>`：JS 视图过渡协调器接管站内点击；不用
//! `leptos_router::components::A`，因为它会按前缀自动写 `aria-current`，
//! 与这里按栏目家族计算的激活态冲突（`/` 会对所有页面高亮）。

use leptos::children::ViewFn;
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::components::ui::{use_sheet_close, BottomSheet, Icon, IconName};
use crate::routes::Route;
use crate::theme::ThemeToggle;

/// 手机导航抽屉的 DOM ID；预载回归脚本按 `aria-controls` 定位菜单按钮。
const MENU_ID: &str = "mobile-nav-menu";

/// 导航项配置，用于描述 Header 中的一个链接。
///
/// 字段：
/// - `route`：目标路由
/// - `label`：显示文本
/// - `is_active`：当前是否处于激活状态
#[derive(Debug, Clone, PartialEq)]
pub struct NavItemConfig {
    /// 目标路由。
    pub route: Route,
    /// 显示文本。
    pub label: &'static str,
    /// 当前是否处于激活状态。
    pub is_active: bool,
}

/// 某个栏目（按 `label` 区分）当前是否处于激活状态。
///
/// 栏目列表本身固定，随路径变化的只有激活态；各栏目元素按 `label` 保持同一个 DOM 节点，
/// 激活态原地切换（`aria-current` 与 `.nav-indicator` 胶囊），与转场脚本测量胶囊的方式一致。
fn item_active(nav_items: Signal<Vec<NavItemConfig>>, label: &'static str) -> Signal<bool> {
    Signal::derive(move || {
        nav_items.with(|items| {
            items
                .iter()
                .any(|item| item.label == label && item.is_active)
        })
    })
}

/// 顶部导航栏组件。
///
/// Props：
/// - `nav_items`：导航项列表（随路径变化）
/// - `right_content`：右侧自定义内容（如搜索入口、主题切换）
/// - `max_width`：内部导航的宽度类，需与正文 `max-w-*` 一致以保证左右边缘对齐
#[component]
pub fn Header(
    #[prop(into)] nav_items: Signal<Vec<NavItemConfig>>,
    #[prop(into)] right_content: ViewFn,
    #[prop(default = "max-w-3xl")] max_width: &'static str,
) -> impl IntoView {
    let sheet_open = RwSignal::new(false);
    let menu_button_id = format!("{MENU_ID}-trigger");
    let palette = crate::components::palette::use_search_palette();

    view! {
        <header data-vt-shell="frontend-header" class="site-header">
            <div class=format!("site-header-inner {max_width}")>
                <a class="wordmark" href=Route::Home.to_string()>
                    <span class="wordmark-mark" aria-hidden="true"></span>
                    "Yggdrasil"
                </a>
                <nav class="site-nav" aria-label="主导航">
                    <For
                        each=move || nav_items.get()
                        key=|item| item.label
                        children=move |item| {
                            let is_active = item_active(nav_items, item.label);
                            view! { <NavItem route=item.route label=item.label is_active=is_active/> }
                        }
                    />
                </nav>
                <div class="site-actions">
                    {right_content.run()}
                    {match palette {
                        Some(palette) => {
                            view! {
                                <button
                                    class="touch-icon"
                                    type="button"
                                    aria-label="搜索"
                                    aria-haspopup="dialog"
                                    on:click=move |_| palette.open()
                                >
                                    <Icon name=IconName::Search size=19/>
                                </button>
                            }
                                .into_any()
                        }
                        None => {
                            view! {
                                <a
                                    class="touch-icon"
                                    href=Route::Search { q: String::new() }.to_string()
                                    aria-label="搜索"
                                >
                                    <Icon name=IconName::Search size=19/>
                                </a>
                            }
                                .into_any()
                        }
                    }}
                    <button
                        id=menu_button_id.clone()
                        class="touch-icon"
                        type="button"
                        aria-label="打开导航"
                        aria-haspopup="dialog"
                        aria-expanded=move || sheet_open.get().to_string()
                        aria-controls=MENU_ID
                        on:click=move |_| sheet_open.set(true)
                    >
                        <Icon name=IconName::Menu size=19/>
                    </button>
                </div>
            </div>
        </header>
        <NavSheet open=sheet_open id=MENU_ID trigger_id=menu_button_id nav_items=nav_items/>
    }
}

/// 单个桌面导航项：当前项内含 `.nav-indicator` 胶囊（转场时只有胶囊在栏目间滑动，文字留在原位）。
#[component]
fn NavItem(
    route: Route,
    label: &'static str,
    #[prop(into)] is_active: Signal<bool>,
) -> impl IntoView {
    view! {
        <a
            class="nav-item"
            href=route.to_string()
            aria-current=move || is_active.get().then_some("page")
        >
            {move || {
                is_active
                    .get()
                    .then(|| view! { <span class="nav-indicator" aria-hidden="true"></span> })
            }}
            {label}
        </a>
    }
}

/// 手机导航抽屉：底部面板，含栏目、外观分段控件与次要链接。选中栏目或链接后关闭。
#[component]
fn NavSheet(
    open: RwSignal<bool>,
    id: &'static str,
    #[prop(into)] trigger_id: String,
    nav_items: Signal<Vec<NavItemConfig>>,
) -> impl IntoView {
    view! {
        <BottomSheet open=open id=id trigger_id=trigger_id label="导航">
            <NavSheetBody nav_items=nav_items/>
        </BottomSheet>
    }
}

#[component]
fn NavSheetBody(nav_items: Signal<Vec<NavItemConfig>>) -> impl IntoView {
    let sheet = use_sheet_close();
    view! {
        <nav aria-label="栏目">
            <ul class="flex flex-col gap-0.5">
                <For
                    each=move || nav_items.get()
                    key=|item| item.label
                    children=move |item| {
                        let is_active = item_active(nav_items, item.label);
                        view! {
                            <li>
                                <SheetNavItem
                                    route=item.route
                                    label=item.label
                                    is_active=is_active
                                    on_navigate=move |_: ()| sheet.close()
                                />
                            </li>
                        }
                    }
                />
            </ul>
        </nav>
        <div class="my-3 mx-1 h-(--hairline) bg-hairline"></div>
        <div class="sheet-row">
            <span>"外观"</span>
            <ThemeToggle/>
        </div>
        <div class="sheet-links">
            // rel="external"：让路由协调器放行，整页加载 RSS。
            <a href="/feed.xml" rel="external">
                "RSS"
            </a>
            <a href=Route::Changelog.to_string() on:click=move |_| sheet.close()>
                "更新日志"
            </a>
            <a href=Route::WritingGuide.to_string() on:click=move |_| sheet.close()>
                "写作指南"
            </a>
        </div>
    }
}

/// 抽屉中的栏目链接（48px 触控目标），点击后关闭抽屉。
#[component]
fn SheetNavItem(
    route: Route,
    label: &'static str,
    #[prop(into)] is_active: Signal<bool>,
    #[prop(into)] on_navigate: Callback<()>,
) -> impl IntoView {
    view! {
        <a
            class="sheet-nav-item"
            href=route.to_string()
            aria-current=move || is_active.get().then_some("page")
            on:click=move |_| on_navigate.run(())
        >
            {label}
        </a>
    }
}

/// 顶部导航栏右侧的搜索入口：「搜索 ⌘K」小胶囊。
///
/// 默认跳转搜索页并随路径感知激活状态；`active` 可显式覆盖，`on_click` 传入时
/// 拦截默认跳转，用于打开搜索面板或图鉴演示。
#[component]
pub fn SearchIconLink(
    /// 可选显式覆盖激活状态。默认随当前路径自动感知（`/search`）。
    #[prop(optional)]
    active: Option<bool>,
    /// 可选点击事件回调。若提供，将拦截默认路由跳转。
    #[prop(optional, into)]
    on_click: Option<Callback<MouseEvent>>,
) -> impl IntoView {
    // 显式覆盖时不读路由，组件可脱离 Router 使用（图鉴演示、测试）。
    let pathname = active.is_none().then(|| use_location().pathname);
    let is_active = move || match (active, pathname) {
        (Some(active), _) => active,
        (None, Some(pathname)) => pathname.with(|p| p.trim_end_matches('/') == "/search"),
        (None, None) => false,
    };
    view! {
        <a
            class="search-trigger"
            href=Route::Search { q: String::new() }.to_string()
            aria-current=move || is_active().then_some("page")
            aria-label="搜索"
            aria-keyshortcuts="Meta+K Control+K"
            on:click=move |ev: MouseEvent| {
                // 传入 on_click 时只执行回调；prevent_default 让协调器跳过这次点击。
                if let Some(cb) = on_click {
                    ev.prevent_default();
                    cb.run(ev);
                }
            }
        >
            <Icon name=IconName::Search size=15/>
            <span>"搜索"</span>
            <kbd>"⌘K"</kbd>
        </a>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::nav::build_nav_items;

    fn render_header(path: &'static str) -> String {
        Owner::new().with(|| {
            let items = build_nav_items(path);
            view! {
                <Header
                    nav_items=items
                    right_content=|| view! { <span id="right-slot"></span> }
                    max_width="max-w-[880px]"
                />
            }
            .to_html()
        })
    }

    #[test]
    fn header_marks_only_active_item_with_indicator() {
        let html = render_header("/archives");
        assert!(html.contains("data-vt-shell=\"frontend-header\""));
        assert!(html.contains("site-header-inner max-w-[880px]"));
        assert!(html.contains("aria-label=\"主导航\""));
        // 只有「归档」带 aria-current 与胶囊。
        assert_eq!(html.matches("aria-current=\"page\"").count(), 1);
        assert_eq!(html.matches("class=\"nav-indicator\"").count(), 1);
        let active_start = html.find("aria-current=\"page\"").unwrap();
        let tail = &html[active_start..];
        assert!(tail.contains("nav-indicator"));
        assert!(tail.find("nav-indicator").unwrap() < tail.find("归档").unwrap());
        // 无搜索面板句柄时，搜索图标退化为指向 /search 的链接。
        assert!(html.contains("href=\"/search\""));
        assert!(html.contains("id=\"right-slot\""));
        // 抽屉未打开：只渲染触发按钮。
        assert!(html.contains("aria-expanded=\"false\""));
        assert!(html.contains("aria-controls=\"mobile-nav-menu\""));
        assert!(!html.contains("class=\"sheet\""));
    }

    #[test]
    fn header_without_match_has_no_indicator() {
        let html = render_header("/post/hello");
        assert!(!html.contains("nav-indicator"));
        assert!(!html.contains("aria-current"));
    }

    #[test]
    fn search_link_active_override() {
        let on = Owner::new().with(|| view! { <SearchIconLink active=true/> }.to_html());
        assert!(on.contains("aria-current=\"page\""));
        assert!(on.contains("aria-keyshortcuts=\"Meta+K Control+K\""));
        assert!(on.contains("href=\"/search\""));
        let off = Owner::new().with(|| view! { <SearchIconLink active=false/> }.to_html());
        assert!(!off.contains("aria-current"));
    }
}
