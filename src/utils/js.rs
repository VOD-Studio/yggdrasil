//! WASM 端 JS 互操作小工具（写剪贴板、探测主题过渡）。
//!
//! 仅在 `target_arch = "wasm32"` 下编译；服务端 SSR 路径不参与。

#![cfg(target_arch = "wasm32")]

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
