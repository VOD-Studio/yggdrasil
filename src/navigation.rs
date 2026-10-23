//! 站内导航：JS 协调器与 Leptos 路由器之间的握手。
//!
//! JS 协调器 `window.__routeTransitions`（`libs/yggdrasil-core/src/route-transitions.ts`）
//! 拥有 history 与视图过渡：它拦截站内链接点击与 `popstate`，在视图过渡的 update 回调里
//! 写入 history，然后通知 Rust；Leptos 的 `BrowserUrl` 只负责“读 URL → 渲染”。
//!
//! - [`NavigationBridge`]：连接协调器，收到通知后把地址栏 URL 写入 `BrowserUrl` 的 URL 信号。
//! - [`RouteMarker`]：包住路由出口，路由内容提交到 DOM 后通知协调器“已渲染”。
//! - [`push`] / [`replace`] / [`back`] / [`forward`] / [`external`]：程序化导航的唯一入口，
//!   **禁止**使用 `leptos_router::hooks::use_navigate` 与 `<A>`。
//!
//! 站内链接一律是普通 `<a href>`；指向非 SPA 路由的链接（`/feed.xml`、`/doc/...`、
//! `/uploads/...` 等）必须带 `rel="external"`（或 `download`/`target`）。

use leptos::html;
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use std::sync::atomic::AtomicBool;
#[cfg(target_arch = "wasm32")]
use std::sync::atomic::Ordering;

/// 协调器是否已连接（仅浏览器端 `NavigationBridge` 水合后置真）。
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
static CONNECTED: AtomicBool = AtomicBool::new(false);

/// 站内跳转（新增 history 记录）。协调器未连接时退化为整页跳转；服务端为空操作。
pub fn push(route: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if CONNECTED.load(Ordering::Acquire) && crate::bridges::navigation::push(route) {
            return;
        }
        if let Some(window) = web_sys::window() {
            let _ = window.location().assign(route);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = route;
}

/// 站内跳转（替换当前 history 记录）。协调器未连接时退化为 `location.replace`。
pub fn replace(route: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if CONNECTED.load(Ordering::Acquire) && crate::bridges::navigation::replace(route) {
            return;
        }
        if let Some(window) = web_sys::window() {
            let _ = window.location().replace(route);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = route;
}

/// 历史记录后退。协调器缺失时用 `history.back()`。
pub fn back() {
    #[cfg(target_arch = "wasm32")]
    {
        if CONNECTED.load(Ordering::Acquire) && crate::bridges::navigation::back() {
            return;
        }
        if let Some(history) = web_sys::window().and_then(|window| window.history().ok()) {
            let _ = history.back();
        }
    }
}

/// 历史记录前进。协调器缺失时用 `history.forward()`。
pub fn forward() {
    #[cfg(target_arch = "wasm32")]
    {
        if CONNECTED.load(Ordering::Acquire) && crate::bridges::navigation::forward() {
            return;
        }
        if let Some(history) = web_sys::window().and_then(|window| window.history().ok()) {
            let _ = history.forward();
        }
    }
}

/// 整页跳转到站外或非 SPA 地址。协调器缺失时用 `location.assign`。
pub fn external(url: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if CONNECTED.load(Ordering::Acquire) && crate::bridges::navigation::external(url) {
            return;
        }
        if let Some(window) = web_sys::window() {
            let _ = window.location().assign(url);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = url;
}

/// 连接 JS 协调器；放在 `<Router>` 内、`<Routes>` 前，不渲染 DOM。
///
/// 水合后只执行一次：协调器每次发布新路由（视图过渡的 update 回调里）都会通知这里，
/// 这里读取已被 JS 写好的地址栏并设置 `BrowserUrl` 的 URL 信号，路由器随即渲染新页面。
#[component]
pub fn NavigationBridge() -> impl IntoView {
    #[cfg(feature = "hydrate")]
    {
        use leptos_router::location::{BrowserUrl, LocationProvider};
        use wasm_bindgen::prelude::Closure;

        let browser_url = use_context::<BrowserUrl>();
        // JS 持有函数引用：闭包必须活到 disconnect 之后，存进本地存储并在卸载时释放。
        let notify = StoredValue::new_local(None::<Closure<dyn FnMut()>>);
        Effect::new(move |_| {
            let Some(browser_url) = browser_url.clone() else {
                return;
            };
            let closure = Closure::wrap(Box::new(move || {
                if let Ok(url) = BrowserUrl::current() {
                    browser_url.as_url().set(url);
                }
            }) as Box<dyn FnMut()>);
            if crate::bridges::navigation::connect(&closure) {
                CONNECTED.store(true, Ordering::Release);
                notify.set_value(Some(closure));
            }
        });
        on_cleanup(move || {
            if CONNECTED.swap(false, Ordering::AcqRel) {
                crate::bridges::navigation::disconnect();
            }
        });
    }
}

/// 当前路由的“路径 + 查询”字符串，作为视图过渡协调器核对 DOM 的路由标记。
fn route_key(pathname: &str, search: &str) -> String {
    if search.is_empty() {
        pathname.to_owned()
    } else {
        format!("{pathname}?{search}")
    }
}

/// 路由出口外壳：渲染 `data-vt-route`/`data-vt-navigation` 标记，并在路由内容提交后通知协调器。
///
/// 标记用 `set_attribute` 直接写入（不经响应式绑定），保证调用 `rendered` 时 DOM 已带正确值；
/// `is_routing` 为真（路由器正在切换）时不通知，回落后由 Effect 重新触发。
#[component]
pub fn RouteMarker(is_routing: ReadSignal<bool>, children: Children) -> impl IntoView {
    let location = use_location();
    let pathname = location.pathname;
    let search = location.search;
    let first_route = route_key(&pathname.get_untracked(), &search.get_untracked());
    let marker = NodeRef::<html::Div>::new();

    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        let route = route_key(&pathname.get(), &search.get());
        if is_routing.get() {
            return;
        }
        request_animation_frame(move || {
            // URL 已变、路由器还没来得及把 is_routing 置真的窗口内放弃本次通知。
            if is_routing.get_untracked() {
                return;
            }
            let Some(element) = marker.get_untracked() else {
                return;
            };
            let id = crate::bridges::navigation::navigation_id();
            let _ = element.set_attribute("data-vt-route", &route);
            let _ = element.set_attribute("data-vt-navigation", &id.to_string());
            crate::bridges::navigation::rendered(id, &route);
        });
    });
    #[cfg(not(feature = "hydrate"))]
    let _ = is_routing;

    view! {
        <div
            style="display: contents"
            data-vt-route=first_route
            data-vt-navigation="0"
            node_ref=marker
        >
            {children()}
        </div>
    }
}
