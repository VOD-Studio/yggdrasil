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

use crate::components::ui::{Icon, IconName};
use dioxus::prelude::*;
// InteractionLocation 提供 client_coordinates(),用于读取鼠标点击的视口坐标。
// 该 trait 不在 dioxus::prelude(后者只 re-export events::*),需单独从 dioxus::html 引入。
// 仅 WASM 前端用到(ThemeToggle 的圆形展开动画取点击坐标),服务端构建剥离。
#[cfg(target_arch = "wasm32")]
use dioxus::html::InteractionLocation;

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

/// 提供主题上下文的 Hook。
///
/// 同时提供两个上下文：
/// - `Signal<Theme>`：用户选择的主题意图（Light/Dark/System）。
/// - `Memo<ResolvedTheme>`：实际生效明暗（System 模式下会随系统偏好变化）。
///
/// 持久化策略：`Light`/`Dark` 写入 localStorage；`System` **移除** localStorage，
/// 使首屏防闪烁脚本回退到 `prefers-color-scheme`。
///
/// WASM 端注册 `matchMedia('(prefers-color-scheme: dark)')` 的 `change` 监听，
/// 系统偏好变化时更新 `system_dark`，派生的 `resolved` memo 自动重算，下游
/// （CodeMirror 等）通过读取 `resolved` 即可实时跟随系统。
///
/// `<html>` 的 `dark` class 不在此处管理。WASM 端由 `ThemeToggle` 的 onclick
/// 通过 `yggdrasil-core.js` 的圆形展开动画在 View Transition 回调里同步 toggle。
/// 初始 class 由 `ThemePreload` 首屏脚本设置,避免闪烁。
pub fn use_theme_provider() -> Signal<Theme> {
    // hydration 不会修补首轮 VDOM 与 SSR 的差异，必须使用一致的初始模式。
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut theme = use_signal(|| Theme::System);
    // system_dark 仅在 wasm32 监听闭包里 .set()；非 wasm 构建剥离该闭包，
    // 此处统一标注 mut 以满足 wasm32 的借用检查（非 wasm 端会触发 unused_mut，
    // 由下方 cfg_attr 抑制）。
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut system_dark = use_signal(read_system_dark);
    // resolved 是 theme 与 system_dark 的派生态：任一变化都自动重算。
    let resolved = use_memo(move || theme().resolve(system_dark()));

    #[cfg(target_arch = "wasm32")]
    {
        let mut initialized = use_signal(|| false);
        use_effect(move || {
            theme.set(detect_initial_theme());
            initialized.set(true);
        });

        // 恢复选择后才允许持久化，避免初始 System 清掉已保存的 Light/Dark。
        use_effect(move || {
            if !initialized() {
                return;
            }
            let current = theme();
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
    // 这是原始实现的关键缺口：系统偏好变化时 system_dark signal 与 resolved memo 都
    // 会更新，下游（CodeMirror 等）能跟随，但 <html> 的 dark class 原本只有「首屏预
    // 加载脚本」与「手动点击 ThemeToggle」两个写入点，自动场景下无人同步，页面配色
    // 纹丝不动。
    //
    // 仅在 System 模式（theme == System）且 resolved 实际翻转时触发：Light/Dark 显式
    // 模式下 resolved 由 theme 决定，DOM 已由 ThemeToggle::onclick 全权处理，effect
    // 不介入，避免与手动点击的 VT 动画重复触发。
    //
    // 采用无动画的瞬切（__applyResolvedTheme，设置语义）而非 VT 圆形展开：系统偏好
    // 变化是后台事件，实测 View Transitions 在此上下文下动画不显示（VT 流程虽正常
    // 完成、CSS 也正确挂上 tt-expand，但视觉上仅瞬切），故跟随系统走瞬切更可靠；
    // 手动点击仍走 __startThemeTransition 保留圆形展开动画。
    //
    // 首次挂载也对齐 DOM：系统偏好可能在 ThemePreload 执行后、hydration 前变化。
    #[cfg(target_arch = "wasm32")]
    {
        let mut prev_resolved: Signal<Option<ResolvedTheme>> = use_signal(|| None);
        use_effect(move || {
            // 仅 System 模式需要自动同步；Light/Dark 由 ThemeToggle::onclick 负责。
            if theme() != Theme::System {
                return;
            }
            let current = resolved();
            // 未翻转（如 signal 因无关读取重算）直接跳过。
            if prev_resolved() == Some(current) {
                return;
            }
            prev_resolved.set(Some(current));
            // resolved 翻转 → 瞬切同步 dark class（设置语义，非翻转）。
            let Some(window) = web_sys::window() else {
                return;
            };
            let key = "__applyResolvedTheme".into();
            if let Ok(fn_val) = js_sys::Reflect::get(&window, &key) {
                if !fn_val.is_undefined() && !fn_val.is_null() {
                    use wasm_bindgen::JsCast;
                    let fn_obj = fn_val.unchecked_into::<js_sys::Function>();
                    let is_dark = current == ResolvedTheme::Dark;
                    let _ = fn_obj.call1(&window, &is_dark.into());
                }
            }
        });
    }

    // WASM 端监听系统颜色偏好变化（仅 System 模式有意义，但无论何种模式都更新
    // system_dark signal；resolved memo 会决定是否真正改变 ResolvedTheme）。
    // 注册 / 卸载清理由 use_event_listener 统一负责，target 在其内部 use_effect
    // 首次运行时通过 acquire 闭包获取（此时 DOM 一定可用）。
    #[cfg(target_arch = "wasm32")]
    {
        use crate::hooks::event_listener::use_event_listener;

        use_event_listener(
            move || {
                let window = web_sys::window()?;
                let media = window
                    .match_media("(prefers-color-scheme: dark)")
                    .ok()
                    .flatten()?;
                // 补齐首轮渲染到监听器挂载之间发生的系统配色变化。
                system_dark.set(media.matches());
                Some(media)
            },
            "change",
            move || {
                // handler 需要重新读取当前 matches 值（MediaQueryList 的事件回调
                // 不带参，只能自行重新查询）。
                if let Some(window) = web_sys::window() {
                    if let Ok(Some(media)) = window.match_media("(prefers-color-scheme: dark)") {
                        system_dark.set(media.matches());
                    }
                }
            },
        );
    }

    use_context_provider(|| theme);
    use_context_provider(|| resolved);
    theme
}

/// 读取当前主题意图 Signal 的 Hook。
///
/// 需在 `use_theme_provider` 之后的组件树中使用。
pub fn use_theme() -> Signal<Theme> {
    use_context::<Signal<Theme>>()
}

/// 读取实际生效明暗主题的 Hook。
///
/// 下游消费者（CodeMirror、图标判定）应优先用此 Hook：System 模式下系统
/// 偏好变化时，返回值会自动更新。
pub fn use_resolved_theme() -> Memo<ResolvedTheme> {
    use_context::<Memo<ResolvedTheme>>()
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

// 分段控件的选中态是首屏必需样式，随 SSR 输出：由预载脚本写入的
// data-theme-mode 决定，不等 WASM，也不受外部 CSS 旧缓存影响。
const THEME_SEGMENT_STYLE: &str = r#"
html[data-theme-mode="light"] .theme-seg [data-mode="light"],
html[data-theme-mode="dark"] .theme-seg [data-mode="dark"],
html[data-theme-mode="system"] .theme-seg [data-mode="system"],
html:not([data-theme-mode]) .theme-seg [data-mode="system"] {
    background-color: var(--color-surface-2);
    color: var(--color-fg);
    box-shadow: 0 0 0 1px var(--color-hairline), var(--shadow-sm);
}
html.dark[data-theme-mode="light"] .theme-seg [data-mode="light"],
html.dark[data-theme-mode="dark"] .theme-seg [data-mode="dark"],
html.dark[data-theme-mode="system"] .theme-seg [data-mode="system"],
html.dark:not([data-theme-mode]) .theme-seg [data-mode="system"] {
    background-color: var(--color-surface-3);
}
"#;

/// 首屏主题预加载脚本组件。
///
/// 在页面渲染前读取 localStorage / 系统偏好，设置 `dark` class 和控制分段
/// 控件选中态的 `data-theme-mode`，防止配色和选中项在 hydration 前后错位。
#[component]
pub fn ThemePreload() -> Element {
    rsx! {
        style { dangerous_inner_html: "{THEME_SEGMENT_STYLE}" }
        script { dangerous_inner_html: "{THEME_PRELOAD_SCRIPT}" }
    }
}

/// 主题切换：浅色 / 深色 / 跟随系统三段式分段控件。
///
/// 选中态由 `<html data-theme-mode>` 经 [`THEME_SEGMENT_STYLE`] 决定，首屏无需
/// 等待 WASM。点选后只有**实际明暗**（ResolvedTheme）翻转时才以点击点为圆心
/// 做圆形扩散；明暗不变（如切到跟随系统且结果相同）时直接切换。
///
/// Props：
/// - `labels`：显示文字（手机抽屉里用）；缺省只显示图标，悬停提示写明模式
/// 切换到指定外观：只有**实际明暗**翻转时才以 `origin`（视口坐标）为圆心做圆形扩散。
///
/// 主题分段控件与搜索面板的「切换外观」命令共用。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
pub fn choose_theme(mut theme: Signal<Theme>, mode: Theme, origin: (f64, f64)) {
    if *theme.peek() == mode {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let prev_resolved = theme.peek().resolve(read_system_dark());
        let new_resolved = mode.resolve(read_system_dark());
        if prev_resolved != new_resolved {
            use wasm_bindgen::JsCast;
            // 实际明暗翻转 → 圆形扩散 VT。JS 从 DOM 现状推导目标主题。
            if let Some(window) = web_sys::window() {
                let key = "__startThemeTransition".into();
                if let Ok(fn_val) = js_sys::Reflect::get(&window, &key) {
                    if !fn_val.is_undefined() && !fn_val.is_null() {
                        let fn_obj = fn_val.unchecked_into::<js_sys::Function>();
                        let _ = fn_obj.call2(&window, &origin.0.into(), &origin.1.into());
                    }
                }
            }
        }
    }
    theme.set(mode);
}

// evt 仅在 wasm32 用于取点击坐标,服务端构建剥离,故允许非 wasm 的 unused_variables。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
#[component]
pub fn ThemeToggle(#[props(default)] labels: bool) -> Element {
    let theme = use_theme();
    let current = theme();
    let modes = [
        (Theme::Light, "light", IconName::Sun),
        (Theme::Dark, "dark", IconName::Moon),
        (Theme::System, "system", IconName::Monitor),
    ];
    rsx! {
        div { class: "theme-seg", role: "radiogroup", aria_label: "外观",
            for (mode , key , icon) in modes {
                button {
                    key: "{key}",
                    class: "theme-seg-item",
                    r#type: "button",
                    role: "radio",
                    "data-mode": key,
                    aria_checked: "{current == mode}",
                    aria_label: if labels { None } else { Some(mode_label(mode)) },
                    title: if labels { "" } else { mode_label(mode) },
                    onclick: move |evt| {
                        #[cfg(target_arch = "wasm32")]
                        let origin = {
                            let coords = evt.client_coordinates();
                            (coords.x, coords.y)
                        };
                        #[cfg(not(target_arch = "wasm32"))]
                        let origin = (0.0, 0.0);
                        choose_theme(theme, mode, origin);
                    },
                    Icon { name: icon, size: 14 }
                    if labels {
                        span { "{mode_label(mode)}" }
                    }
                }
            }
        }
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
    fn theme_derives_equality() {
        // Theme 派生了 PartialEq，相同变体必须相等。
        assert_eq!(Theme::Light, Theme::Light);
        assert_eq!(Theme::Dark, Theme::Dark);
        assert_eq!(Theme::System, Theme::System);
        assert_ne!(Theme::Light, Theme::Dark);
        assert_ne!(Theme::Dark, Theme::System);
        assert_ne!(Theme::System, Theme::Light);
    }

    #[test]
    fn resolved_theme_derives_equality() {
        assert_eq!(ResolvedTheme::Light, ResolvedTheme::Light);
        assert_eq!(ResolvedTheme::Dark, ResolvedTheme::Dark);
        assert_ne!(ResolvedTheme::Light, ResolvedTheme::Dark);
    }

    #[test]
    fn mode_label_covers_all_variants() {
        assert_eq!(mode_label(Theme::Light), "浅色");
        assert_eq!(mode_label(Theme::Dark), "深色");
        assert_eq!(mode_label(Theme::System), "跟随系统");
    }

    #[test]
    fn theme_preload_script_sets_dark_class() {
        // 预加载脚本必须包含给 documentElement 设置 dark class 的逻辑。
        assert!(THEME_PRELOAD_SCRIPT.contains("classList.toggle('dark',"));
    }

    #[test]
    fn theme_preload_script_reads_local_storage() {
        // 预加载脚本必须读取 yggdrasil-theme 键，与 THEME_KEY 保持一致。
        assert!(THEME_PRELOAD_SCRIPT.contains("localStorage.getItem('yggdrasil-theme')"));
        assert_eq!(THEME_KEY, "yggdrasil-theme");
    }

    #[test]
    fn theme_preload_script_falls_back_to_prefers_color_scheme() {
        // 当 localStorage 中无主题时，脚本应回退到系统颜色偏好。
        // System 模式移除 localStorage 后首屏即走此分支，保证零闪烁跟随系统。
        assert!(THEME_PRELOAD_SCRIPT.contains("prefers-color-scheme: dark"));
    }

    #[test]
    fn theme_preload_script_swallows_errors() {
        // 预加载脚本必须包裹在 try/catch 中，避免禁用 localStorage 时抛错。
        assert!(THEME_PRELOAD_SCRIPT.contains("try"));
        assert!(THEME_PRELOAD_SCRIPT.contains("catch"));
    }
}
