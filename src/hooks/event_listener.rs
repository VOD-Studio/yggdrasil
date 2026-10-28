//! 通用 DOM 事件监听 hook：封装 add/remove_event_listener 生命周期。
//!
//! 用 `StoredValue::new_local` 持有 `(Closure, target)`，`Effect` 注册，`on_cleanup`
//! 清理，把 theme/showcase 等处手写的样板收口到一处。非 wasm32 目标下整体编译为空操作
//! （服务端无 DOM），调用方无需再写占位变量或 `#[cfg]`。
//!
//! 设计要点：
//! - `target` 通过一个初始化闭包 `acquire` 提供，在 `Effect` 首次运行时调用
//!   （此时 DOM 一定可用）；这样调用方无需自己再包一层 `Effect`。
//! - `event` 要求 `&'static str`，add 与 remove 复用同一字符串字面量。
//! - handler 是 `FnMut()`（无参）；若调用方需要 target 信息（如 `media.matches()`），
//!   应在 `acquire` 里 clone 一份后 move 进 handler。需要事件对象（如 `KeyboardEvent`
//!   的 `key()`）时不要用本 hook，直接用 `window_event_listener` 或元素的 `on:` 绑定。
//! - `acquire`、`handler` 不要求 `Send + Sync`（可直接捕获 `web_sys` 句柄）；
//!   捕获信号时它们是 `Copy` 句柄，同样无约束。

/// 注册一次性事件监听器，组件卸载时自动移除。
///
/// - `acquire`：在 `Effect` 首次运行（DOM 已就绪）时调用，返回要监听的 target
///   及其克隆（handler 内若要读 target，用这个克隆）。返回 `None` 则不注册。
/// - `event`：事件名（如 `"scroll"` / `"change"`），需为 `&'static str`。
/// - `handler`：事件触发时的回调，`FnMut()`（无参）。
///
/// 须在组件函数体（reactive owner 内）调用；仅在 wasm32 执行真实注册，非 wasm 为 noop。
///
/// # 例
///
/// ```ignore
/// use_event_listener(
///     web_sys::window,
///     "scroll",
///     move || { visible.set(/* ... */); },
/// );
/// ```
#[cfg(target_arch = "wasm32")]
pub fn use_event_listener<T, A, F>(acquire: A, event: &'static str, handler: F)
where
    T: AsRef<web_sys::EventTarget> + Clone + 'static,
    A: FnOnce() -> Option<T> + 'static,
    F: FnMut() + 'static,
{
    use leptos::prelude::*;
    use wasm_bindgen::{closure::Closure, JsCast};

    // 持有 (Closure, target)，在整个组件生命周期内复用；
    // on_cleanup 时 take 出来移除监听，防止泄漏。
    type ListenerState<T> = Option<(Closure<dyn FnMut()>, T)>;
    let state = StoredValue::<ListenerState<T>, LocalStorage>::new_local(None);

    // Effect 回调是 FnMut，但 acquire 是 FnOnce、handler 要 move 进事件闭包。
    // 用 Option 包裹后 take()，首次运行消费掉，避免重复 move。
    let mut acquire = Some(acquire);
    let mut handler = Some(handler);

    Effect::new(move |_| {
        let Some(acquire) = acquire.take() else {
            return;
        };
        let Some(mut handler) = handler.take() else {
            return;
        };
        let Some(target) = acquire() else { return };
        let closure = Closure::wrap(Box::new(move || {
            handler();
        }) as Box<dyn FnMut()>);
        let _ = target
            .as_ref()
            .add_event_listener_with_callback(event, closure.as_ref().unchecked_ref());
        state.set_value(Some((closure, target)));
    });

    on_cleanup(move || {
        if let Some(Some((closure, target))) = state.try_update_value(Option::take) {
            let _ = target
                .as_ref()
                .remove_event_listener_with_callback(event, closure.as_ref().unchecked_ref());
        }
    });
}

/// 非 wasm 占位：服务端无 DOM，编译为空操作。
///
/// 保留与 wasm 版相同的泛型形态，调用方代码两端完全一致。
#[cfg(not(target_arch = "wasm32"))]
#[allow(unused_variables, dead_code)]
pub fn use_event_listener<T, A, F>(acquire: A, event: &'static str, handler: F)
where
    T: Clone + 'static,
    A: FnOnce() -> Option<T> + 'static,
    F: FnMut() + 'static,
{
    // 服务端无 DOM，空实现。
}
