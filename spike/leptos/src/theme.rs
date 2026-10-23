//! 主题（浅色 / 深色 / 跟随系统）管理。
//!
//! 三态模型：
//! - `Theme::Light` / `Theme::Dark`：用户显式选择，持久化到 localStorage。
//! - `Theme::System`：跟随系统偏好。**移除 localStorage 持久化**，首屏防闪烁
//!   脚本因此自动回退到 `prefers-color-scheme` 分支；运行时监听
//!   `matchMedia('(prefers-color-scheme: dark)')` 的 `change` 事件，系统切换
//!   主题时实时同步 `.dark` class 与下游（CodeMirror 等）。
//!
//! SSR 与客户端 hydration 都从 `Theme::System` 开始；挂载后再读取
//! localStorage 恢复用户选择。首屏配色和图标均由 `ThemePreload` 提前设置，
//! 不依赖 WASM 加载速度或服务端无法读取的浏览器偏好。
//!
//! 下游消费者（CodeMirror、SVG 图标判定）应读 `ResolvedTheme`（实际生效明暗），
//! 而非 `Theme`（用户意图），这样 System 模式下系统偏好变化能自动传播。

use leptos::prelude::*;

use crate::components::ui::{
    IconName, SegmentedControl, SegmentedItem, SegmentedKind, SegmentedSize,
};

/// localStorage 中存储主题值的键名。
#[cfg(any(target_arch = "wasm32", test))]
const THEME_KEY: &str = "yggdrasil-theme";

/// 实际生效的明暗主题（用户选择经 `resolve()` 解析后的结果）。
///
/// 下游消费者（CodeMirror 主题、图标判定）应读此类型而非 `Theme`：
/// System 模式下系统偏好变化时，`ResolvedTheme` 会随之更新，而 `Theme` 不变。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedTheme {
    /// 浅色。
    Light,
    /// 深色。
    Dark,
}

/// 用户选择的主题意图（三态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    /// 浅色主题。
    Light,
    /// 深色主题。
    Dark,
    /// 跟随系统：移除 localStorage 持久化，运行时监听系统偏好。
    System,
}

impl Theme {
    /// 解析为实际生效的明暗主题。
    ///
    /// `Light`/`Dark` 直接返回；`System` 根据 `system_dark`（当前系统是否深色）
    /// 决定。
    pub fn resolve(&self, system_dark: bool) -> ResolvedTheme {
        match self {
            Theme::Light => ResolvedTheme::Light,
            Theme::Dark => ResolvedTheme::Dark,
            Theme::System => {
                if system_dark {
                    ResolvedTheme::Dark
                } else {
                    ResolvedTheme::Light
                }
            }
        }
    }
}

/// 读取当前系统是否为深色偏好。
///
/// WASM 端通过 `matchMedia('(prefers-color-scheme: dark)')` 读取；SSR 端拿不到
/// 客户端系统偏好，返回 `false`（首屏由 `ThemePreload` 脚本客户端纠正）。
fn read_system_dark() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(media)) = window.match_media("(prefers-color-scheme: dark)") {
                return media.matches();
            }
        }
    }
    false
}

/// 检测初始主题意图。
///
/// 在 WASM 客户端优先读取 localStorage（`"light"`/`"dark"`），无值时回退到
/// `Theme::System`（不再直接固化系统偏好）。仅在 hydration 完成后调用。
#[cfg(target_arch = "wasm32")]
fn detect_initial_theme() -> Theme {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(value)) = storage.get_item(THEME_KEY) {
                return match value.as_str() {
                    "dark" => Theme::Dark,
                    "light" => Theme::Light,
                    _ => Theme::System,
                };
            }
        }
    }
    Theme::System
}

/// 调用 `window[name](...args)`；全局函数不存在时静默跳过。
#[cfg(target_arch = "wasm32")]
fn call_window_fn(name: &str, args: &[wasm_bindgen::JsValue]) {
    use wasm_bindgen::JsCast;

    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(fn_val) = js_sys::Reflect::get(&window, &name.into()) else {
        return;
    };
    if fn_val.is_undefined() || fn_val.is_null() {
        return;
    }
    let fn_obj = fn_val.unchecked_into::<js_sys::Function>();
    let array = js_sys::Array::new();
    for arg in args {
        array.push(arg);
    }
    let _ = fn_obj.apply(&window, &array);
}

/// 提供主题上下文（对应 Dioxus 版 `use_theme_provider`），在 `App` 根部调用一次。
///
/// 同时提供两个上下文：
/// - `RwSignal<Theme>`：用户选择的主题意图（Light/Dark/System）。
/// - `Memo<ResolvedTheme>`：实际生效明暗（System 模式下会随系统偏好变化）。
///
/// 持久化策略：`Light`/`Dark` 写入 localStorage；`System` **移除** localStorage，
/// 使首屏防闪烁脚本回退到 `prefers-color-scheme`。
///
/// WASM 端注册 `matchMedia('(prefers-color-scheme: dark)')` 的 `change` 监听，
/// 系统偏好变化时更新 `system_dark`，派生的 `resolved` memo 自动重算，下游
/// （CodeMirror 等）通过读取 `resolved` 即可实时跟随系统。
///
/// `<html>` 的 `dark` class 不在此处管理。WASM 端由 `ThemeToggle` 的 `on_change`
/// 通过 `yggdrasil-core.js` 的圆形展开动画在 View Transition 回调里同步 toggle。
/// 初始 class 由 `ThemePreload` 首屏脚本设置,避免闪烁。
pub fn provide_theme() -> RwSignal<Theme> {
    // hydration 必须与 SSR 输出一致，初始模式统一为 System。
    let theme = RwSignal::new(Theme::System);
    let system_dark = RwSignal::new(read_system_dark());
    // resolved 是 theme 与 system_dark 的派生态：任一变化都自动重算。
    let resolved = Memo::new(move |_| theme.get().resolve(system_dark.get()));

    #[cfg(target_arch = "wasm32")]
    {
        let initialized = RwSignal::new(false);
        // Effect 只在浏览器 hydration 之后运行：恢复 localStorage 中的选择。
        Effect::new(move |_| {
            theme.set(detect_initial_theme());
            initialized.set(true);
        });

        // 恢复选择后才允许持久化，避免初始 System 清掉已保存的 Light/Dark。
        Effect::new(move |_| {
            if !initialized.get() {
                return;
            }
            let current = theme.get();
            let mode = match current {
                Theme::Light => "light",
                Theme::Dark => "dark",
                Theme::System => "system",
            };
            if let Some(window) = web_sys::window() {
                if let Some(html) = window.document().and_then(|doc| doc.document_element()) {
                    let _ = html.set_attribute("data-theme-mode", mode);
                }
                if let Ok(Some(storage)) = window.local_storage() {
                    if current == Theme::System {
                        let _ = storage.remove_item(THEME_KEY);
                    } else {
                        let _ = storage.set_item(THEME_KEY, mode);
                    }
                }
            }
        });
    }

    // WASM 端：System 模式下系统颜色偏好变化时，把新明暗同步到 <html> 的 dark class。
    //
    // 仅在 System 模式（theme == System）且 resolved 实际翻转时触发：Light/Dark 显式
    // 模式下 resolved 由 theme 决定，DOM 已由 ThemeToggle 的 on_change 全权处理，effect
    // 不介入，避免与手动点击的 VT 动画重复触发。
    //
    // 采用无动画的瞬切（__applyResolvedTheme，设置语义）而非 VT 圆形展开：系统偏好
    // 变化是后台事件，View Transitions 在此上下文下动画不显示，故跟随系统走瞬切；
    // 手动点击仍走 __startThemeTransition 保留圆形展开动画。
    //
    // 首次挂载也对齐 DOM：系统偏好可能在 ThemePreload 执行后、hydration 前变化。
    #[cfg(target_arch = "wasm32")]
    {
        // 上次同步的明暗只作比较，不需要订阅，用 StoredValue。
        let prev_resolved = StoredValue::new(None::<ResolvedTheme>);
        Effect::new(move |_| {
            // 仅 System 模式需要自动同步；Light/Dark 由 ThemeToggle 负责。
            if theme.get() != Theme::System {
                return;
            }
            let current = resolved.get();
            // 未翻转直接跳过。
            if prev_resolved.get_value() == Some(current) {
                return;
            }
            prev_resolved.set_value(Some(current));
            // resolved 翻转 → 瞬切同步 dark class（设置语义，非翻转）。
            let is_dark = current == ResolvedTheme::Dark;
            call_window_fn("__applyResolvedTheme", &[is_dark.into()]);
        });
    }

    // WASM 端监听系统颜色偏好变化（无论何种模式都更新 system_dark；resolved memo
    // 决定是否真正改变 ResolvedTheme）。主题上下文挂在 App 根部、与页面同寿，
    // 监听器无需卸载，闭包直接 forget 交给浏览器持有。
    #[cfg(target_arch = "wasm32")]
    {
        Effect::new(move |_| {
            use wasm_bindgen::JsCast;

            let Some(window) = web_sys::window() else {
                return;
            };
            let Ok(Some(media)) = window.match_media("(prefers-color-scheme: dark)") else {
                return;
            };
            // 补齐首轮渲染到监听器挂载之间发生的系统配色变化。
            system_dark.set(media.matches());
            let handler = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
                // MediaQueryList 的 change 回调这里不取事件参数，重新查询当前 matches。
                if let Some(window) = web_sys::window() {
                    if let Ok(Some(media)) = window.match_media("(prefers-color-scheme: dark)") {
                        system_dark.set(media.matches());
                    }
                }
            });
            let _ =
                media.add_event_listener_with_callback("change", handler.as_ref().unchecked_ref());
            handler.forget();
        });
    }

    provide_context(theme);
    provide_context(resolved);
    theme
}

/// 读取当前主题意图 Signal。
///
/// 需在 `provide_theme` 之后的组件树中使用。
pub fn use_theme() -> RwSignal<Theme> {
    expect_context::<RwSignal<Theme>>()
}

/// 读取实际生效明暗主题。
///
/// 下游消费者（CodeMirror、图标判定）应优先用此函数：System 模式下系统
/// 偏好变化时，返回值会自动更新。
pub fn use_resolved_theme() -> Memo<ResolvedTheme> {
    expect_context::<Memo<ResolvedTheme>>()
}

const THEME_PRELOAD_SCRIPT: &str = r#"
(function() {
    var theme = 'system';
    try {
        var saved = localStorage.getItem('yggdrasil-theme');
        if (saved === 'dark' || saved === 'light') theme = saved;
    } catch (e) {}
    document.documentElement.setAttribute('data-theme-mode', theme);
    document.documentElement.classList.toggle('dark',
        theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches));
})();
"#;

// 主题分段控件的首屏选中态是必需样式，随 SSR 输出：由预载脚本写入的
// data-theme-mode 决定，不等 WASM，也不受外部 CSS 旧缓存影响。它不进 layer，
// 优先级高于 ui.css 里「未测量时当前项浮起」的兜底规则：先取消 SSR 默认选中项
// （System）的浮起，再按 data-theme-mode 浮起对应位置的按钮。
// 按钮顺序与 ThemeToggle 的 items 顺序耦合：1 浅色、2 深色、3 跟随系统。
const THEME_SEGMENT_STYLE: &str = r#"
.theme-seg .segmented:not([data-ready]) > .segmented-item[data-active] {
    background-color: transparent; box-shadow: none; color: var(--color-fg-2); font-weight: 400;
}
html[data-theme-mode="light"] .theme-seg .segmented:not([data-ready]) > button:nth-of-type(1),
html[data-theme-mode="dark"] .theme-seg .segmented:not([data-ready]) > button:nth-of-type(2),
html[data-theme-mode="system"] .theme-seg .segmented:not([data-ready]) > button:nth-of-type(3),
html:not([data-theme-mode]) .theme-seg .segmented:not([data-ready]) > button:nth-of-type(3) {
    background-color: var(--color-surface-2); color: var(--color-fg); font-weight: 500;
    box-shadow: 0 0 0 var(--hairline) var(--color-hairline), var(--shadow-sm);
}
html.dark[data-theme-mode="light"] .theme-seg .segmented:not([data-ready]) > button:nth-of-type(1),
html.dark[data-theme-mode="dark"] .theme-seg .segmented:not([data-ready]) > button:nth-of-type(2),
html.dark[data-theme-mode="system"] .theme-seg .segmented:not([data-ready]) > button:nth-of-type(3),
html.dark:not([data-theme-mode]) .theme-seg .segmented:not([data-ready]) > button:nth-of-type(3) {
    background-color: var(--color-surface-3);
}
"#;

/// 首屏主题预加载脚本组件（放在外壳 `<head>` 里，只由 SSR 输出）。
///
/// 在页面渲染前读取 localStorage / 系统偏好，设置 `dark` class 和控制分段
/// 控件选中态的 `data-theme-mode`，防止配色和选中项在 hydration 前后错位。
#[component]
pub fn ThemePreload() -> impl IntoView {
    view! {
        <style inner_html=THEME_SEGMENT_STYLE></style>
        <script inner_html=THEME_PRELOAD_SCRIPT></script>
    }
}

/// 视口中心：键盘切换外观时，圆形扩散以此为圆心。
fn viewport_center() -> (f64, f64) {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .map(|w| {
                let width = w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
                let height = w
                    .inner_height()
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                (width / 2.0, height / 2.0)
            })
            .unwrap_or((0.0, 0.0))
    }
    #[cfg(not(target_arch = "wasm32"))]
    (0.0, 0.0)
}

/// 切换到指定外观：只有**实际明暗**翻转时才以 `origin`（视口坐标）为圆心做圆形扩散。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub fn choose_theme(theme: RwSignal<Theme>, mode: Theme, origin: (f64, f64)) {
    let current = theme.get_untracked();
    if current == mode {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let prev_resolved = current.resolve(read_system_dark());
        let new_resolved = mode.resolve(read_system_dark());
        if prev_resolved != new_resolved {
            // 实际明暗翻转 → 圆形扩散 VT。JS 从 DOM 现状推导目标主题。
            call_window_fn(
                "__startThemeTransition",
                &[origin.0.into(), origin.1.into()],
            );
        }
    }
    theme.set(mode);
}

/// 主题切换：浅色 / 深色 / 跟随系统三段式分段控件，基于 [`SegmentedControl`]（Radio）。
///
/// 首屏选中态由 `<html data-theme-mode>` 经 [`THEME_SEGMENT_STYLE`] 决定，首屏无需
/// 等待 WASM。点选后只有**实际明暗**（ResolvedTheme）翻转时才以点击点为圆心
/// 做圆形扩散；明暗不变（如切到跟随系统且结果相同）、或由键盘切换时取视口中心。
///
/// Props：
/// - `icon_only`：只显示图标（页头固定窄位）；缺省由容器宽度决定是否显示文字
#[component]
pub fn ThemeToggle(#[prop(optional)] icon_only: bool) -> impl IntoView {
    let theme = use_theme();
    // 指针按下的位置：点击切换时作为圆形扩散的圆心；键盘切换没有，退回视口中心。
    // 只在回调里读写，不参与渲染，用 StoredValue。
    let pointer = StoredValue::new(None::<(f64, f64)>);
    let items = vec![
        SegmentedItem::new(Theme::Light, mode_label(Theme::Light)).icon(IconName::Sun),
        SegmentedItem::new(Theme::Dark, mode_label(Theme::Dark)).icon(IconName::Moon),
        SegmentedItem::new(Theme::System, mode_label(Theme::System)).icon(IconName::Monitor),
    ];
    let on_change = move |mode: Theme| {
        let origin = pointer.get_value().unwrap_or_else(viewport_center);
        choose_theme(theme, mode, origin);
        pointer.set_value(None);
    };
    view! {
        <div
            class="theme-seg"
            on:pointerdown=move |ev| {
                pointer.set_value(Some((f64::from(ev.client_x()), f64::from(ev.client_y()))));
            }
        >
            <SegmentedControl
                items=items
                value=theme
                kind=SegmentedKind::Radio
                size=SegmentedSize::Sm
                aria_label="外观"
                icon_only=icon_only
                on_change=on_change
            />
        </div>
    }
}

/// 主题意图的中文标签，用于 aria-label / title。
fn mode_label(theme: Theme) -> &'static str {
    match theme {
        Theme::Light => "浅色",
        Theme::Dark => "深色",
        Theme::System => "跟随系统",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_light_and_dark_ignore_system() {
        // Light / Dark 的 resolve 与系统偏好无关。
        assert_eq!(Theme::Light.resolve(true), ResolvedTheme::Light);
        assert_eq!(Theme::Light.resolve(false), ResolvedTheme::Light);
        assert_eq!(Theme::Dark.resolve(true), ResolvedTheme::Dark);
        assert_eq!(Theme::Dark.resolve(false), ResolvedTheme::Dark);
    }

    #[test]
    fn resolve_system_follows_system_dark() {
        // System 的 resolve 由 system_dark 决定。
        assert_eq!(Theme::System.resolve(true), ResolvedTheme::Dark);
        assert_eq!(Theme::System.resolve(false), ResolvedTheme::Light);
    }

    #[test]
    fn mode_label_covers_all_variants() {
        assert_eq!(mode_label(Theme::Light), "浅色");
        assert_eq!(mode_label(Theme::Dark), "深色");
        assert_eq!(mode_label(Theme::System), "跟随系统");
    }

    #[test]
    fn theme_preload_script_reads_local_storage() {
        // 预加载脚本必须读取 yggdrasil-theme 键，与 THEME_KEY 保持一致。
        assert!(THEME_PRELOAD_SCRIPT.contains("localStorage.getItem('yggdrasil-theme')"));
        assert_eq!(THEME_KEY, "yggdrasil-theme");
    }

    #[test]
    fn theme_preload_script_sets_dark_class_with_fallback() {
        // 设置 dark class；无保存值时回退系统偏好；禁用 localStorage 时不抛错。
        assert!(THEME_PRELOAD_SCRIPT.contains("classList.toggle('dark',"));
        assert!(THEME_PRELOAD_SCRIPT.contains("prefers-color-scheme: dark"));
        assert!(THEME_PRELOAD_SCRIPT.contains("try"));
        assert!(THEME_PRELOAD_SCRIPT.contains("catch"));
    }
}
