//! WASM 端 JS 互操作小工具（调用 `window` 上的可选全局函数、写剪贴板、探测主题过渡）。
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
/// 这些库由 `shell()` 里的全局 `<script src>` 注入，加载时机不保证早于调用点。
///
/// P5 §8.2：增强函数（`__initPostContent` 等）拆入 `yggdrasil-enhance.js` 按需加载后，
/// SPA 首次导航时脚本可能晚于 effect 执行。目标函数未定义且 core 已建
/// `__pendingEnhanceCalls` 队列时调用入队，增强脚本尾部 flush 补执行；队列不存在
/// （core 未加载，理论不可达）保持原先的静默跳过。启动函数（core 提供）始终同步
/// 可用，不走队列。
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
            return;
        }
    }
    // 增强脚本尚未就绪：队列存在则入队（{name, args}），等对应 IIFE 尾部 flush。
    if let Ok(queue) = js_sys::Reflect::get(window, &"__pendingEnhanceCalls".into()) {
        if js_sys::Array::is_array(&queue) {
            let queue: &js_sys::Array = queue.unchecked_ref();
            let call = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&call, &"name".into(), &name.into());
            let arr = js_sys::Array::new();
            for a in args {
                arr.push(a);
            }
            let _ = js_sys::Reflect::set(&call, &"args".into(), &arr.into());
            let _ = queue.push(&call.into());
        }
    }
}

/// 主题切换的 View Transition 是否进行中（`<html>` 带 `is-theme-transitioning`）。
///
/// 动画播放的是伪元素快照，期间改动实时 DOM 会穿透快照、表现为瞬切；依赖主题的
/// effect 据此跳过，由 VT 回调负责换肤，动画结束后再幂等兜底。
pub fn is_theme_transitioning() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
        .is_some_and(|el| el.class_list().contains("is-theme-transitioning"))
}

/// 把文本写入剪贴板，成功返回 `true`。不支持 Clipboard API（如非安全上下文）或
/// 被拒绝时返回 `false`，调用方据此提示手动复制。
pub async fn copy_text(text: &str) -> bool {
    use wasm_bindgen::JsCast;
    let clipboard = web_sys::window().and_then(|window| {
        js_sys::Reflect::get(window.navigator().as_ref(), &"clipboard".into())
            .ok()?
            .dyn_into::<web_sys::Clipboard>()
            .ok()
    });
    match clipboard {
        Some(clipboard) => wasm_bindgen_futures::JsFuture::from(clipboard.write_text(text))
            .await
            .is_ok(),
        None => false,
    }
}
