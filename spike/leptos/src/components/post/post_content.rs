//! 文章内容组件
//!
//! 渲染由服务端生成的文章 HTML 内容，并在 WASM 前端初始化交互脚本。
//!
//! 原版把 `content_html` 拆成「HTML 片段 + CodeRunner」序列，让可运行代码块
//! （`<pre data-runnable="true">`）作为一等组件渲染。CodeRunner 不在原型范围内：
//! 可运行块保留服务端高亮好的 `<pre>` 原样输出，因此整篇正文就是单个 HTML 片段，
//! DOM 与原版无可运行块时一致（`.post-content > div > 正文`）。原版按内容哈希
//! 缓存片段解析（规避 Dioxus 重渲染重复解析）；Leptos 组件体只运行一次，无需缓存。

use leptos::prelude::*;

use crate::components::post::enhance_assets::{EnhanceScript, LightboxAssets};

/// 文章内容组件。
///
/// Props：
/// - `content_html`：服务端渲染的文章 HTML 字符串
/// - `scope_id`：可选的局部正文 ID；图鉴只增强该实例，正式页面使用默认范围。
///
/// 关键行为：
/// - 在 `target_arch = "wasm32"` 环境下调用 `window.__initPostContent` 初始化代码块
///   复制按钮；正文增强在 `yggdrasil-enhance.js`、灯箱在 `lightbox.js`，
///   均由本组件按需声明注入（见 [`crate::components::post::enhance_assets`]），
///   这里仅设置初始化配置 `__lightboxSelectors` 并兜底调用。
/// - 调用方在文章切换时整体重建本组件（详情页的 `Suspend` 随文章重新产出视图），
///   effect 随新文章重新初始化。
#[component]
pub fn PostContent(
    #[prop(into)] content_html: String,
    #[prop(optional)] scope_id: Option<&'static str>,
) -> impl IntoView {
    #[cfg(target_arch = "wasm32")]
    {
        use crate::utils::js::{invoke_optional_global, is_theme_transitioning};

        // mermaid 流程图主题需随当前生效主题（light/dark）切换。effect 内读取
        // resolved_theme 建立订阅：主题变化时 effect 重跑，重调 __initMermaid 传入新
        // theme；mermaid.ts 用 dataset.mermaidTheme 记住上次渲染主题，检测到变化时按
        // 缓存源码重渲染（mermaid 颜色烤进 SVG 内联样式，无法靠 CSS 原地切主题）。
        let resolved_theme = crate::theme::use_resolved_theme();

        // scrollToHash 的一次性守卫：仅首次 effect 运行时调用。effect 因读取主题而在
        // 主题切换时重跑，但 hash 滚动是首屏补救措施，不应随主题切换再次触发。
        let did_scroll = StoredValue::new(false);

        Effect::new(move |_| {
            let Some(window) = web_sys::window() else {
                return;
            };

            // 图鉴实例只增强自己的正文，正式页面仍按原选择器处理。
            let selector =
                scope_id.map_or_else(|| ".post-content".to_string(), |id| format!("#{id}"));
            // 调用 window.__initPostContent：函数不存在时入队或静默跳过。
            invoke_optional_global(&window, "__initPostContent", &[selector.clone().into()]);

            // mermaid 流程图懒加载渲染：扫描 .post-content 下的 language-mermaid 代码块，
            // IntersectionObserver 视口可见时动态 import /mermaid/mermaid.js 渲染成 SVG。
            let theme_str = if resolved_theme.get() == crate::theme::ResolvedTheme::Dark {
                "dark"
            } else {
                "light"
            };
            // VT 动画期间跳过：手动切换主题时 __startThemeTransition 的 VT 回调已通过
            // onThemeChange registry 同步触发 mermaid 重渲染（出现在 NEW 快照里）；本 effect
            // 早于 VT 回调执行，抢先改实时 DOM 会穿透伪元素快照表现为瞬切。动画结束后
            // 由 registry 负责，此处在下一次主题信号变化时做幂等兜底。
            if !is_theme_transitioning() {
                invoke_optional_global(
                    &window,
                    "__initMermaid",
                    &[selector.clone().into(), theme_str.into()],
                );
            }

            // 双保险契约：先设配置，若 lightbox.js 已加载则立即调用；
            // 否则 lightbox.js 加载完后其 IIFE 尾部读到配置自启动。
            let selectors = js_sys::Array::new();
            selectors.push(&selector.into());
            if scope_id.is_none() {
                selectors.push(&".entry-cover".into());
            }
            let selectors_val: wasm_bindgen::JsValue = selectors.into();
            if scope_id.is_none() {
                let _ =
                    js_sys::Reflect::set(&window, &"__lightboxSelectors".into(), &selectors_val);
            }
            invoke_optional_global(&window, "__initLightbox", &[selectors_val]);

            // 安装 hash 锚点点击拦截器（幂等）：在 capture 阶段接管正文内的 `#id` 锚点，
            // 自行平滑滚动，避免被前端路由当作导航处理。
            invoke_optional_global(&window, "__initAnchorClick", &[]);

            // 内容挂载后若 URL 带 hash，滚动到对应标题：骨架屏阶段标题 DOM 缺失，
            // 浏览器原生 fragment-scroll 失效，此处标题已就绪，补一次。
            if scope_id.is_none() && !did_scroll.get_value() {
                did_scroll.set_value(true);
                invoke_optional_global(&window, "__scrollToHash", &[]);
            }
        });

        on_cleanup(move || {
            if let (Some(window), Some(owner)) = (web_sys::window(), scope_id) {
                invoke_optional_global(&window, "__closeLightboxFor", &[owner.into()]);
            }
        });
    }

    let body =
        (!content_html.trim().is_empty()).then(|| view! { <div inner_html=content_html></div> });

    view! {
        // P5 §8.2：正文增强与灯箱按需注入。SSR 直达烤进 head 带 defer，先于
        // hydration；SPA 首跳脚本晚于 effect 时由增强调用队列兜底。
        <EnhanceScript/>
        <LightboxAssets/>
        <div id=scope_id data-showcase-lightbox-owner=scope_id class="post-content md-content">
            {body}
        </div>
    }
}
