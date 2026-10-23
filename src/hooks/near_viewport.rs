//! 元素接近视口的单向可见性激活。

use leptos::prelude::*;

/// 观察指定 id 的元素是否进入「视口 + 提前量」范围；一旦相交就把返回的信号置为
/// `true` 并停止观察（单向激活，滚出视口不回退——调用方负责激活后的资源生命周期）。
///
/// - 服务端永远返回 `false`：服务端不装观察器，水合首帧与 SSR 输出一致，
///   可见性判断只在水合之后的 `Effect` 里生效。
/// - 浏览器不支持 IntersectionObserver 时保持 `false`：调用方必须提供交互式激活
///   兜底（点击/按钮），不能让目标永远无法激活。
/// - 提前量由 `root_margin` 控制，默认约一个视口高度；调用方按移动端体验调整。
///
/// 卸载安全：观察器与回调存放在 `StoredValue::new_local` 里，`on_cleanup` 断开连接；
/// 回调触发的信号写入经 `try_set`，组件卸载后的迟到回调不会写已释放的信号。
///
/// 约束：观察器只在首次 `Effect` 运行时按 `target_id` 查找元素并挂接，`target_id`
/// 变化不会重连。调用方必须保证 id 在组件实例存续期内稳定，且目标元素在水合完成时
/// 已在 DOM 中（当前 CodeRunner/post 容器均以确定性索引派生 id 且内容变化经
/// keyed 重建实例）。
pub fn use_near_viewport(target_id: String, root_margin: &'static str) -> ReadSignal<bool> {
    let (visible, set_visible) = signal(false);

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{closure::Closure, JsCast};

        type Watch = (
            web_sys::IntersectionObserver,
            Closure<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>,
        );
        let watcher = StoredValue::new_local(None::<Watch>);
        Effect::new(move |_| {
            // 已激活或已挂接就不再观察。
            if visible.get_untracked() || watcher.with_value(|w| w.is_some()) {
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
                        let _ = set_visible.try_set(true);
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
                    watcher.set_value(Some((observer, callback)));
                }
                Err(_) => {
                    // IntersectionObserver 不可用（老浏览器）：保持未激活，
                    // 由调用方的交互入口（点击代码 / 运行按钮）兜底激活。
                }
            }
        });
        on_cleanup(move || {
            if let Some(Some((observer, _callback))) = watcher.try_update_value(Option::take) {
                observer.disconnect();
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (target_id, root_margin, set_visible);
    }

    visible
}

#[cfg(test)]
mod tests {
    use super::*;
    use leptos::reactive::owner::Owner;

    #[test]
    fn server_side_never_activates() {
        Owner::new().with(|| {
            let visible = use_near_viewport("target".to_string(), "0px 0px 100% 0px");
            assert!(!visible.get_untracked());
        });
    }
}
