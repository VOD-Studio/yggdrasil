//! 顶部导航栏组件
//!
//! 文字标、栏目导航（当前项为浮起的胶囊）与右侧操作区；767px 以下只留文字标、
//! 搜索与菜单按钮，菜单为底部抽屉（栏目、外观与次要链接）。样式见
//! styles/components/shell.css。
//!
//! 链接一律用普通 `<a href>`：路由器拦截站内点击做客户端导航；不用
//! `leptos_router::components::A`，因为它会按前缀自动写 `aria-current`，
//! 与这里按栏目家族计算的激活态冲突（`/` 会对所有页面高亮）。

use leptos::children::ViewFn;
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::components::ui::{use_sheet_close, BottomSheet, Icon, IconName};
use crate::theme::ThemeToggle;

/// 手机导航抽屉的 DOM ID；预载回归脚本按 `aria-controls` 定位菜单按钮。
const MENU_ID: &str = "mobile-nav-menu";

/// 导航项配置，用于描述 Header 中的一个链接。
///
/// 字段：
/// - `route`：目标路由路径
/// - `label`：显示文本
/// - `is_active`：当前是否处于激活状态
#[derive(Debug, Clone, PartialEq)]
pub struct NavItemConfig {
    /// 目标路由路径（与 Dioxus `#[route]` 一致）。
    pub route: &'static str,
    /// 显示文本。
    pub label: &'static str,
    /// 当前是否处于激活状态。
    pub is_active: bool,
}

/// 顶部导航栏组件。
///
/// Props：
/// - `nav_items`：导航项列表（随路由变化）
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

    view! {
        <header data-vt-shell="frontend-header" class="site-header">
            <div class=format!("site-header-inner {max_width}")>
                <a class="wordmark" href="/">
                    <span class="wordmark-mark" aria-hidden="true"></span>
                    "Yggdrasil"
                </a>
                <nav class="site-nav" aria-label="主导航">
                    <For
                        each=move || nav_items.get()
                        key=|item| (item.label, item.is_active)
                        let:item
                    >
                        <NavItem route=item.route label=item.label is_active=item.is_active/>
                    </For>
                </nav>
                <div class="site-actions">
                    {right_content.run()}
                    // 搜索面板不在原型范围内，手机端搜索按钮走 Dioxus 版无面板时的链接分支。
                    <a class="touch-icon" href="/search" aria-label="搜索">
                        <Icon name=IconName::Search size=19/>
                    </a>
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
fn NavItem(route: &'static str, label: &'static str, is_active: bool) -> impl IntoView {
    view! {
        <a class="nav-item" href=route aria-current=is_active.then_some("page")>
            {is_active.then(|| view! { <span class="nav-indicator" aria-hidden="true"></span> })}
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
                    key=|item| (item.label, item.is_active)
                    let:item
                >
                    <li>
                        <SheetNavItem
                            route=item.route
                            label=item.label
                            is_active=item.is_active
                            on_navigate=move |_: ()| sheet.close()
                        />
                    </li>
                </For>
            </ul>
        </nav>
        <div class="my-3 mx-1 h-(--hairline) bg-hairline"></div>
        <div class="sheet-row">
            <span>"外观"</span>
            <ThemeToggle/>
        </div>
        <div class="sheet-links">
            // rel="external"：让路由器放行，整页加载 RSS（与 Dioxus 普通 a 标签行为一致）。
            <a href="/feed.xml" rel="external">"RSS"</a>
            <a href="/changelog" on:click=move |_| sheet.close()>"更新日志"</a>
            <a href="/about/writing" on:click=move |_| sheet.close()>"写作指南"</a>
        </div>
    }
}

/// 抽屉中的栏目链接（48px 触控目标），点击后关闭抽屉。
#[component]
fn SheetNavItem(
    route: &'static str,
    label: &'static str,
    is_active: bool,
    #[prop(into)] on_navigate: Callback<()>,
) -> impl IntoView {
    view! {
        <a
            class="sheet-nav-item"
            href=route
            aria-current=is_active.then_some("page")
            on:click=move |_| on_navigate.run(())
        >
            {label}
        </a>
    }
}

/// 顶部导航栏右侧的搜索入口：「搜索 ⌘K」小胶囊。
///
/// 默认跳转搜索页并随路径感知激活状态；`active` 可显式覆盖，`on_click` 传入时
/// 拦截默认跳转，只执行回调。
#[component]
pub fn SearchIconLink(
    /// 可选显式覆盖激活状态。默认随当前路径自动感知（`/search`）。
    #[prop(optional)]
    active: Option<bool>,
    /// 可选点击事件回调。若提供，将拦截默认路由跳转。
    #[prop(optional, into)]
    on_click: Option<Callback<MouseEvent>>,
) -> impl IntoView {
    let pathname = use_location().pathname;
    let is_active = move || {
        active.unwrap_or_else(|| pathname.with(|p| p == "/search" || p.starts_with("/search/")))
    };
    view! {
        <a
            class="search-trigger"
            href="/search"
            aria-current=move || is_active().then_some("page")
            aria-label="搜索"
            aria-keyshortcuts="Meta+K Control+K"
            on:click=move |ev: MouseEvent| {
                // 传入 on_click 时只执行回调；prevent_default 让路由器跳过这次点击。
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
