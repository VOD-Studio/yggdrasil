//! 按组件需求加载浏览器库，资源完成后才允许构造 wasm-bindgen Options/实例。

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = __loadBrowserLibrary, catch)]
    async fn load_browser_library(
        name: &str,
    ) -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>;
}

pub type LibraryResource = Resource<Result<bool, String>>;

/// enabled 可订阅组件状态；卸载会取消 Resource，迟到的加载结果不会挂载旧组件。
pub fn use_browser_library(
    name: &'static str,
    mut enabled: impl FnMut() -> bool + 'static,
) -> LibraryResource {
    use_resource(move || {
        let enabled = enabled();
        async move {
            #[cfg(target_arch = "wasm32")]
            {
                if !enabled {
                    return Ok(false);
                }
                load_browser_library(name).await.map_err(|error| {
                    web_sys::console::error_1(&error);
                    "加载失败，请检查网络后重试。".to_string()
                })?;
                Ok(true)
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = (name, enabled);
                Ok(false)
            }
        }
    })
}

pub fn library_ready(library: LibraryResource) -> bool {
    matches!(&*library.read(), Some(Ok(true)))
}

#[component]
pub fn LibraryLoadError(mut library: LibraryResource) -> Element {
    let message = library
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .cloned();
    rsx! {
        if let Some(message) = message {
            div { role: "alert", class: "flex items-center gap-3 p-3 text-sm text-red-500 dark:text-red-400",
                span { "{message}" }
                button { r#type: "button", class: "underline underline-offset-4", onclick: move |_| library.restart(), "重新加载" }
            }
        }
    }
}
