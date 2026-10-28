//! 站内导航：JS 协调器与 Leptos 路由器之间的握手。
//!
//! JS 协调器 `window.__routeTransitions`（`libs/yggdrasil-core/src/route-transitions.ts`）
//! 拥有 history 与视图过渡：它拦截站内链接点击与 `popstate`，在视图过渡的 update 回调里
//! 写入 history，然后通知 Rust；Leptos 的 `BrowserUrl` 只负责“读 URL → 渲染”。
//!
//! - [`NavigationBridge`]：连接协调器，收到通知后把地址栏 URL 写入 `BrowserUrl` 的 URL 信号。
//! - [`RouteMarker`]：包住路由出口，路由内容提交到 DOM 后通知协调器“已渲染”，并提供
//!   [`use_rendered_route`] 所需的“已提交路由”。
//! - [`RouteProgress`]：路由切换较慢时在页面顶端显示进度条。
//! - [`push`] / [`replace`] / [`back`] / [`forward`] / [`external`]：程序化导航的唯一入口，
//!   **禁止**使用 `leptos_router::hooks::use_navigate` 与 `<A>`。
//!
//! 悬停、聚焦、触屏按下站内链接时，协调器回调 `NavigationBridge` 交给它的预取闭包，
//! 由 [`crate::route_prefetch`] 提前下载目标页面的分包。
//!
//! 站内链接一律是普通 `<a href>`；指向非 SPA 路由的链接（`/feed.xml`、`/doc/...`、
//! `/uploads/...` 等）必须带 `rel="external"`（或 `download`/`target`）。

use leptos::html;
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use std::sync::atomic::AtomicBool;
#[cfg(target_arch = "wasm32")]
use std::sync::atomic::Ordering;

/// 协调器是否已连接（仅浏览器端 `NavigationBridge` 在整棵树水合完毕后置真）。
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
/// 整棵树（含懒加载路由分包）水合完毕后才连接，并随即在 `<html>` 上置 `data-hydrated`
/// （浏览器回归脚本与性能测试等它出现再操作）。水合期间路由器换页会让后续节点对不上
/// SSR 输出，水合 panic 后整个 WASM 失效、站内链接全部点不动；未连接时协调器把站内
/// 链接点击交还浏览器整页跳转（见 `route-transitions.ts` 的 `onClick`）。
///
/// 连接后协调器每次发布新路由（视图过渡的 update 回调里）都会通知这里，
/// 这里读取已被 JS 写好的地址栏并设置 `BrowserUrl` 的 URL 信号，路由器随即渲染新页面。
#[component]
pub fn NavigationBridge() -> impl IntoView {
    #[cfg(feature = "hydrate")]
    {
        use crate::context::UserContext;
        use crate::models::user::UserRole;
        use leptos_router::location::{BrowserUrl, LocationProvider};
        use wasm_bindgen::prelude::Closure;

        let browser_url = use_context::<BrowserUrl>();
        let shared = Owner::current_shared_context();
        // JS 持有函数引用：闭包必须活到 disconnect 之后，存进本地存储并在卸载时释放。
        let notify = StoredValue::new_local(None::<Closure<dyn FnMut()>>);
        let prefetch = StoredValue::new_local(None::<Closure<dyn FnMut(String, bool)>>);
        let user = use_context::<UserContext>();
        Effect::new(move |_| {
            let browser_url = browser_url.clone();
            let shared = shared.clone();
            leptos::task::spawn_local(async move {
                // Effect 在懒加载路由分包水合前就会运行，必须等 during_hydration 回落。
                while shared.as_ref().is_some_and(|sc| sc.during_hydration()) {
                    crate::utils::time::sleep_ms(10).await;
                }
                // 等待期间组件已卸载：不再连接。
                if notify.try_with_value(|_| ()).is_none() {
                    return;
                }
                if let Some(browser_url) = browser_url {
                    let closure = Closure::wrap(Box::new(move || {
                        if let Ok(url) = BrowserUrl::current() {
                            browser_url.as_url().set(url);
                        }
                    }) as Box<dyn FnMut()>);
                    let prefetch_closure =
                        Closure::wrap(Box::new(move |route: String, with_data: bool| {
                            let is_admin = user.is_some_and(|ctx| {
                                ctx.user
                                    .try_with_untracked(|u| {
                                        u.as_ref().is_some_and(|u| u.role == UserRole::Admin)
                                    })
                                    .unwrap_or(false)
                            });
                            crate::route_prefetch::prefetch(&route, with_data, is_admin);
                        }) as Box<dyn FnMut(String, bool)>);
                    if crate::bridges::navigation::connect(&closure, &prefetch_closure) {
                        CONNECTED.store(true, Ordering::Release);
                        notify.set_value(Some(closure));
                        prefetch.set_value(Some(prefetch_closure));
                    }
                }
                if let Some(root) = document().document_element() {
                    let _ = root.set_attribute("data-hydrated", "true");
                }
            });
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

/// 一条路由地址：路径（不含查询串）与查询串（不含 `?`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RenderedRoute {
    pub pathname: String,
    pub search: String,
}

impl RenderedRoute {
    /// 路径加查询串，如 `/search?q=rust`。
    pub fn key(&self) -> String {
        route_key(&self.pathname, &self.search)
    }
}

/// 已提交到 DOM 的路由，由 [`RouteMarker`] 提供。
#[derive(Clone, Copy)]
struct CommittedRoute(ReadSignal<RenderedRoute>);

/// 布局外壳应跟随的路由：当前显示在页面上的那一个，而不是地址栏。
///
/// 懒加载路由切换时，地址栏先变，路由器在新页面的分包与数据就绪前继续显示旧页面；
/// 外壳（导航激活项、主区宽度等）若直接读 `use_location()`，会先切到新页面的样子，
/// 把旧页面套进错误的版式。这里返回的值在新页面提交到 DOM 后才更新。
///
/// 组件首次计算取地址栏：在路由切换中新建的布局本身就属于即将显示的新页面。
/// 须在 [`RouteMarker`] 内使用。
pub fn use_rendered_route() -> Memo<RenderedRoute> {
    let location = use_location();
    let CommittedRoute(committed) = expect_context::<CommittedRoute>();
    Memo::new(move |previous: Option<&RenderedRoute>| {
        let committed = committed.get();
        if previous.is_some() {
            committed
        } else {
            RenderedRoute {
                pathname: location.pathname.get_untracked(),
                search: location.search.get_untracked(),
            }
        }
    })
}

/// 路由出口外壳：渲染 `data-vt-route`/`data-vt-navigation` 标记，并在路由内容提交后通知协调器。
///
/// 标记用 `set_attribute` 直接写入（不经响应式绑定），保证调用 `rendered` 时 DOM 已带正确值；
/// `is_routing` 为真（路由器正在切换）时不通知，回落后由 Effect 重新触发。
/// 同一时机更新 [`use_rendered_route`] 读取的已提交路由。
#[component]
pub fn RouteMarker(is_routing: ReadSignal<bool>, children: Children) -> impl IntoView {
    let location = use_location();
    let pathname = location.pathname;
    let search = location.search;
    let first_route = RenderedRoute {
        pathname: pathname.get_untracked(),
        search: search.get_untracked(),
    };
    let first_key = first_route.key();
    let committed = RwSignal::new(first_route);
    provide_context(CommittedRoute(committed.read_only()));
    let marker = NodeRef::<html::Div>::new();

    #[cfg(feature = "hydrate")]
    Effect::new(move |was_routing: Option<bool>| {
        let route = RenderedRoute {
            pathname: pathname.get(),
            search: search.get(),
        };
        if is_routing.get() {
            return true;
        }
        let commit = move |route: &RenderedRoute| {
            if committed.with_untracked(|current| current != route) {
                committed.set(route.clone());
            }
        };
        // 路由器刚把新页面挂进 DOM：在同一轮微任务里同步外壳，赶在浏览器绘制之前，
        // 不让新页面以旧版式露出一帧。
        if was_routing == Some(true) {
            commit(&route);
        }
        // 不用 requestAnimationFrame：视图过渡进行时浏览器暂停渲染，rAF 要等过渡回调结束才触发，
        // 而协调器正等着这次通知来结束回调（互相等待，只能靠超时兜底）。定时器不受影响。
        set_timeout(
            move || {
                // URL 已变、路由器还没来得及把 is_routing 置真的窗口内放弃本次通知。
                if is_routing.get_untracked() {
                    return;
                }
                // 不经路由器切换的地址变化（仅查询串变化、未匹配路由）在这里提交。
                commit(&route);
                let Some(element) = marker.get_untracked() else {
                    return;
                };
                let key = route.key();
                let id = crate::bridges::navigation::navigation_id();
                let _ = element.set_attribute("data-vt-route", &key);
                let _ = element.set_attribute("data-vt-navigation", &id.to_string());
                crate::bridges::navigation::rendered(id, &key);
            },
            std::time::Duration::ZERO,
        );
        false
    });
    #[cfg(not(feature = "hydrate"))]
    let _ = is_routing;

    view! {
        <div
            style="display: contents"
            data-vt-route=first_key
            data-vt-navigation="0"
            node_ref=marker
        >
            {children()}
        </div>
    }
}

/// 路由切换超过这个时长仍未完成才显示进度条；与协调器的视图过渡等待时长（`WAIT_MS`）一致，
/// 过渡期间浏览器暂停渲染，早于它显示也看不到。
#[cfg(feature = "hydrate")]
const ROUTE_PROGRESS_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

/// 路由切换进度条：新页面的分包或数据加载较慢时，在页面顶端显示一条往复滑动的细线，
/// 表明点击已生效。快速切换（不超过 300ms）不显示。
#[component]
pub fn RouteProgress(is_routing: ReadSignal<bool>) -> impl IntoView {
    let active = RwSignal::new(false);

    #[cfg(feature = "hydrate")]
    {
        // 每次切换换一个世代号，过期的计时器不再点亮进度条。
        let generation = StoredValue::new(0_u64);
        Effect::new(move |_| {
            let routing = is_routing.get();
            generation.update_value(|g| *g += 1);
            if !routing {
                active.set(false);
                return;
            }
            let id = generation.get_value();
            set_timeout(
                move || {
                    if generation.get_value() == id && is_routing.get_untracked() {
                        active.set(true);
                    }
                },
                ROUTE_PROGRESS_DELAY,
            );
        });
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = is_routing;

    view! {
        <div class="route-progress" data-active=move || active.get().then_some("") aria-hidden="true">
            <span></span>
        </div>
    }
}
