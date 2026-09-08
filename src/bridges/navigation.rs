//! 浏览器导航协调器桥接；历史记录中的页面状态只保存在当前文档内存中。

use serde::{de::DeserializeOwned, Serialize};

/// 读取当前历史记录保存的页面状态。SSR 和资源加载失败时没有客户端状态。
pub fn read_state<T: DeserializeOwned>(key: &str) -> Option<T> {
    #[cfg(target_arch = "wasm32")]
    {
        let value = wasm::get_module()?.read_state(key)?;
        serde_json::from_str(&value).ok()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = key;
        None
    }
}

/// 写入挂载时捕获的历史记录，避免卸载后的异步回调覆盖新页面状态。
pub fn write_state<T: Serialize>(entry_id: &str, key: &str, value: &T) {
    #[cfg(target_arch = "wasm32")]
    if let (Some(module), Ok(value)) = (wasm::get_module(), serde_json::to_string(value)) {
        module.write_state(entry_id, key, &value);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (entry_id, key, value);
}

/// 页面挂载时捕获的历史记录标识。服务端返回空字符串。
pub fn entry_id() -> String {
    #[cfg(target_arch = "wasm32")]
    if let Some(module) = wasm::get_module() {
        return module.entry_id();
    }
    String::new()
}

/// 退出登录时清理后台历史记录中的页面状态。
pub fn clear_admin_state() {
    #[cfg(target_arch = "wasm32")]
    if let Some(module) = wasm::get_module() {
        module.clear_admin_state();
    }
}

/// 与本轮路由 DOM 一起渲染的导航编号。
pub fn navigation_id() -> u32 {
    #[cfg(target_arch = "wasm32")]
    if let Some(module) = wasm::get_module() {
        return module.navigation_id();
    }
    0
}

/// 布局 effect 在实际 DOM 提交后通知 JS；JS 再核对编号和 DOM 标记。
pub fn rendered(id: u32, route: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(module) = wasm::get_module() {
        module.rendered(id, route);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (id, route);
}

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use wasm_bindgen::{prelude::*, JsCast};

    #[wasm_bindgen]
    extern "C" {
        /// `window.__routeTransitions` 是对象字面量，通过 Reflect 读取。
        pub type NavigationModule;

        #[wasm_bindgen(method, catch)]
        pub fn connect(
            this: &NavigationModule,
            notify: &Closure<dyn FnMut()>,
            prefix: Option<&str>,
        ) -> Result<(), JsValue>;

        #[wasm_bindgen(method, catch)]
        pub fn disconnect(this: &NavigationModule) -> Result<(), JsValue>;

        #[wasm_bindgen(method, js_name = currentRoute)]
        pub fn current_route(this: &NavigationModule) -> String;

        #[wasm_bindgen(method, js_name = currentPrefix)]
        pub fn current_prefix(this: &NavigationModule) -> Option<String>;

        #[wasm_bindgen(method, js_name = navigationId)]
        pub fn navigation_id(this: &NavigationModule) -> u32;

        #[wasm_bindgen(method)]
        pub fn push(this: &NavigationModule, route: &str);

        #[wasm_bindgen(method)]
        pub fn replace(this: &NavigationModule, route: &str);

        #[wasm_bindgen(method)]
        pub fn back(this: &NavigationModule);

        #[wasm_bindgen(method)]
        pub fn forward(this: &NavigationModule);

        #[wasm_bindgen(method)]
        pub fn external(this: &NavigationModule, url: &str) -> bool;

        #[wasm_bindgen(method)]
        pub fn rendered(this: &NavigationModule, id: u32, route: &str);

        #[wasm_bindgen(method, js_name = readState)]
        pub fn read_state(this: &NavigationModule, key: &str) -> Option<String>;

        #[wasm_bindgen(method, js_name = writeState)]
        pub fn write_state(this: &NavigationModule, entry_id: &str, key: &str, value: &str);

        #[wasm_bindgen(method, js_name = entryId)]
        pub fn entry_id(this: &NavigationModule) -> String;

        #[wasm_bindgen(method, js_name = clearAdminState)]
        pub fn clear_admin_state(this: &NavigationModule);
    }

    pub fn get_module() -> Option<NavigationModule> {
        let window = web_sys::window()?;
        let value = js_sys::Reflect::get(&window, &"__routeTransitions".into()).ok()?;
        if value.is_null() || value.is_undefined() {
            return None;
        }
        let connect = js_sys::Reflect::get(&value, &"connect".into()).ok()?;
        connect
            .is_function()
            .then(|| value.unchecked_into::<NavigationModule>())
    }
}
