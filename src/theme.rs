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
    /// 三态循环切换：Dark → Light → System → Dark。
    pub fn cycle(&self) -> Self {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::System,
            Theme::System => Theme::Dark,
        }
    }

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

// 图标显隐是首屏必需样式，随 SSR 输出，避免外部 CSS 旧缓存影响模式显示。
const THEME_ICON_STYLE: &str = r#"
.theme-toggle svg { display: none; }
html[data-theme-mode="light"] .theme-toggle .theme-icon-light,
html[data-theme-mode="dark"] .theme-toggle .theme-icon-dark,
html[data-theme-mode="system"] .theme-toggle .theme-icon-system,
html:not([data-theme-mode]) .theme-toggle .theme-icon-system { display: block; }
"#;

/// 首屏主题预加载脚本组件。
///
/// 在页面渲染前读取 localStorage / 系统偏好，设置 `dark` class 和控制图标的
/// `data-theme-mode`，防止配色和图标在 hydration 前后错位。
#[component]
pub fn ThemePreload() -> Element {
    rsx! {
        style { dangerous_inner_html: "{THEME_ICON_STYLE}" }
        script { dangerous_inner_html: "{THEME_PRELOAD_SCRIPT}" }
    }
}

/// 主题切换按钮组件（三态循环：Dark → Light → System → Dark）。
///
/// 三个图标使用固定 SSR 结构，由 `data-theme-mode` 决定哪个可见。
/// 首屏无需等待 WASM；切换模式时通过 CSS 显示对应图标并播放进入动画。
///
/// 仅当**实际生效明暗**（ResolvedTheme）因切换而翻转时，才额外触发圆形展开
/// VT 动画（颜色过渡）；明暗不变时（如 Light → System 且系统浅色）只换图标。
///
// evt 仅在 wasm32 用于取点击坐标,服务端构建剥离,故允许非 wasm 的 unused_variables。
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
#[component]
pub fn ThemeToggle() -> Element {
    // theme 在 wasm 与 server 两侧都需要 mut（onclick 内 theme.set）。
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut))]
    let mut theme = use_theme();
    // resolved 用于判断切换前后实际明暗是否翻转（决定是否触发 VT）。
    #[cfg(target_arch = "wasm32")]
    let resolved = use_resolved_theme();

    rsx! {
        button {
            class: "theme-toggle p-2 rounded-full cursor-pointer hover:text-paper-accent transition-colors duration-200 text-paper-secondary",
            r#type: "button",
            onclick: move |evt| {
                let next = theme().cycle();
                #[cfg(target_arch = "wasm32")]
                {
                    let prev_resolved = resolved();
                    let new_resolved = next.resolve(read_system_dark());
                    if prev_resolved != new_resolved {
                        use wasm_bindgen::JsCast;
                        // 实际明暗翻转 → 触发圆形展开 VT 动画（颜色过渡）。
                        // JS 从 DOM 现状推导目标主题(不传 isDark),避免与 Signal 状态不同步。
                        // 用 Reflect::get 取 window.__startThemeTransition 再 call2 调用,
                        // 替代旧版 format!-into-eval 字符串拼贴(无注入面、与 bridge 风格一致)。
                        let coords = evt.client_coordinates();
                        let x = coords.x;
                        let y = coords.y;
                        let window = web_sys::window()
                            .expect(
                                "主题切换回调仅在 WASM 浏览器上下文执行：无 window",
                            );
                        let key = "__startThemeTransition".into();
                        if let Ok(fn_val) = js_sys::Reflect::get(&window, &key) {
                            if !fn_val.is_undefined() && !fn_val.is_null() {
                                let fn_obj = fn_val.unchecked_into::<js_sys::Function>();
                                let _ = fn_obj.call2(&window, &x.into(), &y.into());
                            }
                        }
                    }
                    theme.set(next);
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    theme.set(next);
                }
            },
            // 隐藏的 SVG 不参与无障碍名称计算，可见图标提供当前模式和提示。
            svg {
                class: "theme-icon-dark",
                role: "img",
                "aria-label": "主题切换（当前：{mode_label(Theme::Dark)}）",
                xmlns: "http://www.w3.org/2000/svg",
                height: "24px",
                view_box: "0 -960 960 960",
                width: "24px",
                fill: "currentColor",
                title { "当前：{mode_label(Theme::Dark)}（点击切换）" }
                path { d: "M484-80q-84 0-157.5-32t-128-86.5Q144-253 112-326.5T80-484q0-146 93-257.5T410-880q-18 99 11 193.5T521-521q71 71 165.5 100T880-410q-26 144-138 237T484-80Zm0-80q88 0 163-44t118-121q-86-8-163-43.5T464-465q-61-61-97-138t-43-163q-77 43-120.5 118.5T160-484q0 135 94.5 229.5T484-160Zm-20-305Z" }
            }
            svg {
                class: "theme-icon-light",
                role: "img",
                "aria-label": "主题切换（当前：{mode_label(Theme::Light)}）",
                xmlns: "http://www.w3.org/2000/svg",
                height: "24px",
                view_box: "0 -960 960 960",
                width: "24px",
                fill: "currentColor",
                title { "当前：{mode_label(Theme::Light)}（点击切换）" }
                path { d: "M440-800v-120h80v120h-80Zm0 760v-120h80v120h-80Zm360-400v-80h120v80H800Zm-760 0v-80h120v80H40Zm708-252-56-56 70-72 58 58-72 70ZM198-140l-58-58 72-70 56 56-70 72Zm564 0-70-72 56-56 72 70-58 58ZM212-692l-72-70 58-58 70 72-56 56Zm98 382q-70-70-70-170t70-170q70-70 170-70t170 70q70 70 70 170t-70 170q-70 70-170 70t-170-70Zm283.5-56.5Q640-413 640-480t-46.5-113.5Q547-640 480-640t-113.5 46.5Q320-547 320-480t46.5 113.5Q413-320 480-320t113.5-46.5ZM480-480Z" }
            }
            svg {
                class: "theme-icon-system",
                role: "img",
                "aria-label": "主题切换（当前：{mode_label(Theme::System)}）",
                xmlns: "http://www.w3.org/2000/svg",
                height: "24px",
                view_box: "0 -960 960 960",
                width: "24px",
                fill: "currentColor",
                title { "当前：{mode_label(Theme::System)}（点击切换）" }
                path { d: "M40-120v-80h880v80H40Zm120-120q-33 0-56.5-23.5T80-320v-440q0-33 23.5-56.5T160-840h640q33 0 56.5 23.5T880-760v440q0 33-23.5 56.5T800-240H160Zm0-80h640v-440H160v440Zm0 0v-440 440Z" }
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
    fn cycle_rotates_three_states() {
        // 三态循环：Dark → Light → System → Dark。
        assert_eq!(Theme::Dark.cycle(), Theme::Light);
        assert_eq!(Theme::Light.cycle(), Theme::System);
        assert_eq!(Theme::System.cycle(), Theme::Dark);
    }

    #[test]
    fn cycle_returns_to_origin_after_full_loop() {
        // 走完一圈（三次）应回到起点。
        assert_eq!(Theme::Light.cycle().cycle().cycle(), Theme::Light);
        assert_eq!(Theme::Dark.cycle().cycle().cycle(), Theme::Dark);
        assert_eq!(Theme::System.cycle().cycle().cycle(), Theme::System);
    }

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
