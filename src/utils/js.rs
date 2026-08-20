//! WASM 端 JS 互操作小工具（调用 `window` 上的可选全局函数）。
//!
//! 仅在 `target_arch = "wasm32"` 下编译；服务端 SSR 路径不参与。

#![cfg(target_arch = "wasm32")]

/// 读取 `window` 上的可选全局函数并调用；函数未定义/为 null 时静默跳过。
///
/// 替代 `js_sys::eval("if(window.__x) window.__x(...)")` 字符串拼贴模式：用
/// `Reflect::get` 取属性 + `Function::apply` 调用，无字符串求值，与 `bridges::tiptap`
/// 的类型化 extern 风格一致。
///
/// 用于调用各 IIFE 库暴露的 `window.__init*` 入口（lightbox / mermaid / 锚点拦截等），
/// 这些库由 `Dioxus.toml` 全局 `<script src>` 注入，加载时机不保证早于调用点。
pub fn invoke_optional_global(
    window: &web_sys::Window,
    name: &str,
    args: &[wasm_bindgen::JsValue],
) {
    use wasm_bindgen::JsCast;
    if let Ok(fn_val) = js_sys::Reflect::get(window, &name.into()) {
        if !fn_val.is_undefined() && !fn_val.is_null() {
            let arr = js_sys::Array::new();
            for a in args {
                arr.push(a);
            }
            let _ = fn_val
                .unchecked_into::<js_sys::Function>()
                .apply(window, &arr);
        }
    }
}
