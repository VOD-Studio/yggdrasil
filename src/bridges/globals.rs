//! `window.__*` 页面函数的类型化入口。
//!
//! 这些函数分别由 yggdrasil-core、yggdrasil-enhance、lightbox 三个 IIFE 定义；
//! 全局名和参数只在本模块出现，调用方用普通 Rust 函数，名字写错会在编译期暴露。
//!
//! 函数未定义时（脚本晚于调用点加载，如 SPA 首次导航）调用进入
//! `__pendingEnhanceCalls` 队列，由对应脚本尾部补执行；队列不存在（core 未加载）时
//! 静默跳过。

use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::{JsCast, JsValue};

/// 调用 `window[name](...args)`；未定义时入队，队列也不存在则静默跳过。
///
/// 用 `Reflect::get` 取属性 + `Function::apply` 调用，无字符串求值。增强函数
/// （`__initPostContent` 等）拆入 `yggdrasil-enhance.js` 按需加载后，SPA 首次导航时
/// 脚本可能晚于 effect 执行：目标函数未定义且 core 已建 `__pendingEnhanceCalls` 队列时
/// 调用入队，增强脚本尾部 flush 补执行。启动函数（core 提供）始终同步可用，不走队列。
fn call(name: &str, args: &[JsValue]) {
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Ok(fn_val) = Reflect::get(&window, &name.into()) {
        if !fn_val.is_undefined() && !fn_val.is_null() {
            let arr = Array::new();
            for a in args {
                arr.push(a);
            }
            let _ = fn_val.unchecked_into::<Function>().apply(&window, &arr);
            return;
        }
    }
    // 增强脚本尚未就绪：队列存在则入队（{name, args}），等对应 IIFE 尾部 flush。
    if let Ok(queue) = Reflect::get(&window, &"__pendingEnhanceCalls".into()) {
        if Array::is_array(&queue) {
            let queue: &Array = queue.unchecked_ref();
            let call = Object::new();
            let _ = Reflect::set(&call, &"name".into(), &name.into());
            let arr = Array::new();
            for a in args {
                arr.push(a);
            }
            let _ = Reflect::set(&call, &"args".into(), &arr.into());
            queue.push(&call.into());
        }
    }
}

/// `None` 传 `undefined`：JS 端的默认参数照常生效。
fn opt(v: Option<&str>) -> JsValue {
    v.map_or(JsValue::UNDEFINED, JsValue::from)
}

/// 跳过进行中的 View Transition（命令面板跳转前调用）。
pub fn skip_view_transition() {
    call("__skipViewTransition", &[]);
}

/// 安装 hash 锚点点击拦截器（幂等）。
pub fn init_anchor_click() {
    call("__initAnchorClick", &[]);
}

/// 内容挂载后若 URL 带 hash，滚动到对应标题。
pub fn scroll_to_hash() {
    call("__scrollToHash", &[]);
}

/// 按解析后的明暗直接设置 `<html>` 的 dark class（设置语义，非翻转）。
pub fn apply_resolved_theme(is_dark: bool) {
    call("__applyResolvedTheme", &[is_dark.into()]);
}

/// 从 `(x, y)` 起圆形扩散的主题切换过渡；目标主题由 JS 从 DOM 现状推导。
pub fn start_theme_transition(x: f64, y: f64) {
    call("__startThemeTransition", &[x.into(), y.into()]);
}

/// 增强选择器范围内的文章正文（代码块复制按钮等）。
pub fn init_post_content(selector: &str) {
    call("__initPostContent", &[selector.into()]);
}

/// 绑定选择器范围内服务端渲染 Mermaid 图表的点击放大。
pub fn init_mermaid(selector: &str) {
    call("__initMermaid", &[selector.into()]);
}

/// 初始化目录 scroll-spy；三个选择器为 `None` 时用 JS 端默认值。
pub fn init_toc_sidebar(nav: Option<&str>, content: Option<&str>, scroll: Option<&str>) {
    call("__initTocSidebar", &[opt(nav), opt(content), opt(scroll)]);
}

/// 卸载目录 scroll-spy；`nav` 为 `None` 时用 JS 端默认值。
pub fn dispose_toc_sidebar(nav: Option<&str>) {
    call("__disposeTocSidebar", &[opt(nav)]);
}

/// 初始化更新日志页的版本导航。
pub fn init_changelog_nav() {
    call("__initChangelogNav", &[]);
}

/// 卸载更新日志页的版本导航。
pub fn dispose_changelog_nav() {
    call("__disposeChangelogNav", &[]);
}

/// 初始化组件图鉴的瀑布流布局。
pub fn init_showcase_masonry() {
    call("__initShowcaseMasonry", &[]);
}

/// 卸载组件图鉴的瀑布流布局。
pub fn dispose_showcase_masonry() {
    call("__disposeShowcaseMasonry", &[]);
}

/// 让灯箱接管这些选择器匹配到的图片。
pub fn init_lightbox(selectors: &[&str]) {
    let array: Array = selectors.iter().map(|s| JsValue::from(*s)).collect();
    call("__initLightbox", &[array.into()]);
}

/// 关闭属于 `owner` 的灯箱（所有者组件卸载时调用）。
pub fn close_lightbox_for(owner: &str) {
    call("__closeLightboxFor", &[owner.into()]);
}
