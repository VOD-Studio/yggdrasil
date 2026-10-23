//! 按组件需求加载浏览器库，资源完成后才允许构造 wasm-bindgen Options/实例。
//!
//! 加载入口是 yggdrasil-core 挂在 window 上的 `__loadBrowserLibrary(name)`
//! （按库名注入脚本与样式、去重并返回 Promise）。

use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = __loadBrowserLibrary, catch)]
    async fn load_browser_library(
        name: &str,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>;
}

/// 浏览器库加载状态句柄。
///
/// `state`：`None` 加载中（SSR 恒为 None），`Some(Ok(true))` 就绪，
/// `Some(Ok(false))` 未启用，`Some(Err(_))` 加载失败。
///
/// 不用 `LocalResource`：Resource 在 `<Suspense>` 内被读取时会登记为挂起项，
/// 编辑器按需启用（聚焦表单）触发重新加载时，外层 Suspense 会退回 fallback 并把
/// 评论区整段移出再插回 DOM，正在输入的编辑器因此失焦。这里用 Effect + spawn_local
/// 直接写普通信号，加载过程对 Suspense 不可见。
#[derive(Clone, Copy)]
pub struct LibraryResource {
    reload: RwSignal<u32>,
    state: RwSignal<Option<Result<bool, String>>>,
}

impl LibraryResource {
    /// 重新加载（加载失败后的"重新加载"按钮）。
    pub fn restart(&self) {
        self.reload.update(|v| *v = v.wrapping_add(1));
    }

    /// 当前加载状态（响应式读取）。
    pub fn read_state(&self) -> Option<Result<bool, String>> {
        self.state.get()
    }

    /// 库是否已就绪（非响应式读取，供事件处理器使用）。
    pub fn ready_untracked(&self) -> bool {
        self.state
            .with_untracked(|state| matches!(state, Some(Ok(true))))
    }
}

/// enabled 可订阅组件状态（Effect 内读取，变化时重新加载）；
/// 组件卸载后 `try_set` 失败即丢弃迟到的加载结果，不会挂载旧组件。
pub fn use_browser_library(
    name: &'static str,
    enabled: impl Fn() -> bool + Send + Sync + 'static,
) -> LibraryResource {
    let reload = RwSignal::new(0u32);
    let state = RwSignal::new(None);
    // 每次（重新）加载递增世代号，只接受最新一次请求的结果。
    let generation = StoredValue::new(0u64);
    // Effect 只在浏览器执行。
    Effect::new(move |_| {
        reload.track();
        let enabled = enabled();
        let current = generation.try_update_value(|g| {
            *g += 1;
            *g
        });
        let Some(current) = current else { return };
        #[cfg(target_arch = "wasm32")]
        {
            if !enabled {
                state.set(Some(Ok(false)));
                return;
            }
            state.set(None);
            leptos::task::spawn_local(async move {
                let result = load_browser_library(name)
                    .await
                    .map(|_| true)
                    .map_err(|error| {
                        web_sys::console::error_1(&error);
                        "加载失败，请检查网络后重试。".to_string()
                    });
                if generation.try_get_value() == Some(current) {
                    let _ = state.try_set(Some(result));
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (name, enabled, current);
    });
    LibraryResource { reload, state }
}

/// 库是否已加载就绪（响应式读取）。
pub fn library_ready(library: LibraryResource) -> bool {
    library.state.with(|state| matches!(state, Some(Ok(true))))
}

/// 加载失败时的提示条 + 重新加载按钮。
#[component]
pub fn LibraryLoadError(library: LibraryResource) -> impl IntoView {
    move || {
        library
            .read_state()
            .and_then(|result| result.err())
            .map(|message| {
                view! {
                    <div role="alert" class="flex items-center gap-3 p-3 text-sm text-red-500 dark:text-red-400">
                        <span>{message}</span>
                        <button type="button" class="underline underline-offset-4" on:click=move |_| library.restart()>
                            "重新加载"
                        </button>
                    </div>
                }
            })
    }
}
