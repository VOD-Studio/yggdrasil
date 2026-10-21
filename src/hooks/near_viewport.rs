//! 元素接近视口的单向可见性激活。

use dioxus::prelude::*;

/// 观察指定 id 的元素是否进入「视口 + 提前量」范围；一旦相交就把返回的信号置为
/// `true` 并停止观察（单向激活，滚出视口不回退——调用方负责激活后的资源生命周期）。
///
/// - SSR（非 wasm32）端永远返回 `false`：服务端不装观察器，首次浏览器渲染与 SSR
///   输出一致，可见性判断只在 hydration 之后的 effect 里生效。
/// - 浏览器不支持 IntersectionObserver 时保持 `false`：调用方必须提供交互式激活
///   兜底（点击/按钮），不能让目标永远无法激活。
/// - 提前量由 `root_margin` 控制，默认约一个视口高度；调用方按移动端体验调整。
///
/// 卸载安全：观察器与回调存放在 `use_hook` 的 Rc 里，`use_drop` 断开连接；
/// 回调触发的信号写入发生在组件存活期内（相交即断开），不会有迟到回写。
#[allow(clippy::let_unit_value)]
pub fn use_near_viewport(target_id: String, root_margin: &'static str) -> Signal<bool> {
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut visible = use_signal(|| false);

    #[cfg(target_arch = "wasm32")]
    {
        use std::{cell::RefCell, rc::Rc};
        use wasm_bindgen::{closure::Closure, JsCast};

        type Watch = (
            web_sys::IntersectionObserver,
            Closure<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>,
        );
        let watcher = use_hook(|| Rc::new(RefCell::new(None::<Watch>)));
        let watcher_for_effect = watcher.clone();
        use_effect(move || {
            // 重跑前断开上一次的观察器（visible 翻转后 effect 再跑一次时清干净）。
            if let Some((observer, _callback)) = watcher_for_effect.borrow_mut().take() {
                observer.disconnect();
            }
            // 已激活就不再观察；effect 因读取 visible() 建立订阅，翻转后自动重跑至此。
            if visible() {
                return;
            }
            let Some(target) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(&target_id))
            else {
                return;
            };
            let callback = Closure::new(
                move |entries: js_sys::Array, observer: web_sys::IntersectionObserver| {
                    if entries.iter().any(|value| {
                        value
                            .unchecked_into::<web_sys::IntersectionObserverEntry>()
                            .is_intersecting()
                    }) {
                        observer.disconnect();
                        visible.set(true);
                    }
                },
            );
            let init = web_sys::IntersectionObserverInit::new();
            init.set_root_margin(root_margin);
            match web_sys::IntersectionObserver::new_with_options(
                callback.as_ref().unchecked_ref(),
                &init,
            ) {
                Ok(observer) => {
                    observer.observe(&target);
                    watcher_for_effect
                        .borrow_mut()
                        .replace((observer, callback));
                }
                Err(_) => {
                    // IntersectionObserver 不可用（老浏览器）：保持未激活，
                    // 由调用方的交互入口（点击代码 / 运行按钮）兜底激活。
                }
            }
        });
        use_drop(move || {
            if let Some((observer, _callback)) = watcher.borrow_mut().take() {
                observer.disconnect();
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (target_id, root_margin);
    }

    visible
}
