//! 文章内容组件
//!
//! 渲染由服务端生成的文章 HTML 内容，并在 WASM 前端初始化交互脚本。
//!
//! 可运行代码块（markdown 围栏 ` ```lang runnable `）在服务端渲染为带
//! `data-runnable` / `data-lang` / `data-overrides` / `data-source` 的 `<pre>`。
//! 本组件在渲染前把 `content_html` 拆成片段序列：普通 HTML 文本片段 +
//! [`crate::components::code_runner::CodeRunner`] 组件，使可运行块作为
//! Dioxus vdom 内的一等元素渲染（而非手动篡改 DOM，避免 hydration 冲突）。

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use dioxus::prelude::*;

use crate::components::code_runner::CodeRunner;
use crate::infra::runner_config::ResourceLimits;
#[cfg(target_arch = "wasm32")]
use crate::utils::js::invoke_optional_global;

/// 内容片段：普通 HTML 文本，或一个可运行代码块。
#[derive(Clone, PartialEq, Debug)]
enum ContentFragment {
    /// 原始 HTML 片段（含语法高亮等），直接以 `dangerous_inner_html` 渲染。
    Html(String),
    /// 可运行代码块：语言、源码、可选资源覆盖，以及服务端渲染的完整 `<pre>`
    /// 高亮 HTML（供 CodeRunner 激活前静态展示，SSR 输出因此包含可读代码文本）。
    Runnable {
        lang: String,
        source: String,
        overrides: Option<ResourceLimits>,
        static_html: String,
    },
}

/// 把服务端渲染的文章 HTML 拆成 `Html` / `Runnable` 片段序列。
///
/// 仅识别带 `data-runnable="true"` 的 `<pre>`；其余内容原样作为 Html 片段返回。
/// HTML 实体（`&quot;` `&#x27;` `&amp;` `&lt;` `&gt;`）会被解码还原为原始字符。
fn split_content_fragments(html: &str) -> Vec<ContentFragment> {
    let mut fragments = Vec::new();
    let mut rest = html;

    while let Some(start) = rest.find(r#"<pre data-runnable="true""#) {
        // start 之前的内容作为 Html 片段（非空才推入）。
        let (head, tail) = rest.split_at(start);
        if !head.trim().is_empty() {
            fragments.push(ContentFragment::Html(head.to_string()));
        }

        // 找到对应 </pre> 闭合。
        let Some(end_offset) = tail.find("</pre>") else {
            // 缺失闭合：剩余整体作为 Html 片段兜底，避免丢内容。
            fragments.push(ContentFragment::Html(tail.to_string()));
            rest = "";
            break;
        };
        let pre_block = &tail[..end_offset + "</pre>".len()];
        rest = &tail[end_offset + "</pre>".len()..];

        // 从 pre_block 提取属性。
        let lang = extract_attr(pre_block, "data-lang").unwrap_or_default();
        let overrides = extract_attr(pre_block, "data-overrides")
            .filter(|s| !s.is_empty())
            .and_then(|s| serde_json::from_str::<ResourceLimits>(&s).ok());
        let source = extract_attr(pre_block, "data-source").unwrap_or_default();

        fragments.push(ContentFragment::Runnable {
            lang,
            source,
            overrides,
            // 保留服务端高亮 `<pre>` 原文：CodeRunner 激活前原样静态展示，
            // 编辑器挂载后再由 vdom 移除（CodeMirror 是追加挂载，不冲突）。
            static_html: pre_block.to_string(),
        });
    }

    if !rest.trim().is_empty() {
        fragments.push(ContentFragment::Html(rest.to_string()));
    }

    fragments
}

// ============================================================================
// 片段解析缓存（P4.2）。
//
// 直接在 render 内调用 split_content_fragments 是纯函数、符合渲染纯净性，但
// PostContent 会因主题切换（use_resolved_theme 订阅）等原因重渲染，每次都对
// 整篇 content_html 重新扫描并分配全部片段字符串——长文 + 多运行块时是明显的
// 主线程尖峰。这里按内容哈希做进程内有界缓存：
// - 不用 use_memo：memo 只在闭包读取的 signal 变化时重算，content_html 是普通
//   String prop，读取不建立订阅——memo 会永久缓存首次解析结果，上下篇切换时
//   返回旧 fragments（见下方渲染处的历史注释）。
// - 命中需通过全文相等核对（条目保留 Arc<str> 原文），哈希碰撞不会错渲染。
// - 有界（8 篇，插入序淘汰）：服务端 SSR 渲染不同文章、浏览器切页都不会无界
//   增长；keyed remount 语义不受影响（缓存只跳过解析，输出逐字节相同）。
// ============================================================================
const FRAGMENT_CACHE_CAP: usize = 8;

struct FragmentCacheEntry {
    content: Arc<str>,
    fragments: Arc<Vec<ContentFragment>>,
}

fn fragment_cache() -> &'static Mutex<VecDeque<(u64, FragmentCacheEntry)>> {
    static CACHE: OnceLock<Mutex<VecDeque<(u64, FragmentCacheEntry)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(VecDeque::with_capacity(FRAGMENT_CACHE_CAP)))
}

/// 带内容缓存的片段解析：内容相同（含更新）时复用上次解析结果。
fn split_content_fragments_cached(html: &str) -> Arc<Vec<ContentFragment>> {
    let key = {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        html.hash(&mut hasher);
        hasher.finish()
    };
    {
        let cache = fragment_cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, entry)) = cache
            .iter()
            .find(|(k, entry)| *k == key && &*entry.content == html)
        {
            return entry.fragments.clone();
        }
    }
    let entry = FragmentCacheEntry {
        content: Arc::from(html),
        fragments: Arc::new(split_content_fragments(html)),
    };
    let fragments = entry.fragments.clone();
    let mut cache = fragment_cache().lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() >= FRAGMENT_CACHE_CAP {
        cache.pop_front();
    }
    cache.push_back((key, entry));
    fragments
}

/// 从 HTML 片段中提取首个 `name="value"` 属性值，并解码 HTML 实体。
/// 仅在单个 `<pre>` 块内查找，足够本场景使用。
fn extract_attr(block: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = block.find(&needle)? + needle.len();
    let rest = &block[start..];
    let end = rest.find('"')?;
    Some(decode_html_entities(&rest[..end]))
}

/// 解码本场景出现的 HTML 实体（属性值经 escape_html 转义产生）。
fn decode_html_entities(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// 文章内容组件。
///
/// Props：
/// - `content_html`：服务端渲染的文章 HTML 字符串
/// - `scope_id`：可选的局部正文 ID；图鉴只增强该实例，正式页面使用默认范围。
///
/// 关键行为：
/// - 把可运行代码块拆成 [`CodeRunner`] 组件穿插渲染，其余 HTML 片段照旧。
/// - 在 `target_arch = "wasm32"` 环境下调用 `window.__initPostContent` 初始化代码块
///   复制按钮（`yggdrasil-core.js` 已由 `Dioxus.toml` 全局注入）。
///   灯箱（图片灯箱 + 懒加载）改由 `Dioxus.toml` 全局注入 `lightbox.js`，
///   这里仅设置其初始化配置 `__lightboxSelectors` 并兜底调用。
#[component]
pub fn PostContent(
    content_html: String,
    #[props(default)] scope_id: Option<&'static str>,
) -> Element {
    // 直接在 render 内解析片段：split_content_fragments_cached 是纯函数，结果
    // 按内容哈希缓存（见上方说明），重复渲染零解析开销，且随 content_html 变化
    // 自动失效——上下篇切换、同路由内容更新都能拿到新片段（keyed remount 与
    // dangerous_inner_html diff 不受影响）。
    let fragments = split_content_fragments_cached(&content_html);

    // mermaid 流程图主题需随当前生效主题（light/dark）切换。读 use_resolved_theme()
    // 建立订阅：主题变化时下方 use_effect 重跑，重调 __initMermaid 传入新 theme；
    // mermaid.ts 用 dataset.mermaidTheme 记住上次渲染主题，检测到主题变化时按缓存
    // 源码重渲染（mermaid 颜色烤进 SVG 内联样式，无法靠 CSS 原地切主题）。
    #[cfg(target_arch = "wasm32")]
    let resolved_theme = crate::theme::use_resolved_theme();

    // scrollToHash 的一次性守卫：仅首次 effect 运行时调用。
    // 下方 use_effect 因读取 resolved_theme() 建立订阅，主题切换时会重跑（为重跑
    // mermaid 主题）。但 scrollToHash 是首屏异步取数后的补救滚动，不应随主题切换
    // 再次触发——否则切换主题会把页面跳回 URL hash 位置（即使 hash 已不在用户视野）。
    #[cfg(target_arch = "wasm32")]
    let mut did_scroll = use_signal(|| false);

    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let window = web_sys::window()
            .expect("post_content use_effect 仅在 WASM 浏览器上下文执行：无 window");

        // 图鉴实例只增强自己的正文，正式页面仍按原选择器处理。
        let selector = scope_id.map_or_else(|| ".post-content".to_string(), |id| format!("#{id}"));
        // 调用 window.__initPostContent：函数不存在时静默跳过
        // (与旧 eval 中的 if 守卫语义一致)。
        invoke_optional_global(&window, "__initPostContent", &[selector.clone().into()]);

        // mermaid 流程图懒加载渲染：扫描 .post-content 下的 language-mermaid 代码块,
        // IntersectionObserver 视口可见时动态 import /mermaid/mermaid.js 渲染成 SVG。
        // 读取 resolved_theme() 既是为了传主题,也建立订阅让主题切换重跑此 effect。
        let theme_str: String = if resolved_theme() == crate::theme::ResolvedTheme::Dark {
            "dark".into()
        } else {
            "light".into()
        };
        // VT 动画期间跳过:手动点击主题按钮时,__startThemeTransition 的 VT 回调内已通过
        // onThemeChange registry 同步触发 mermaid 重渲染(被 VT 等待,出现在 NEW 快照里)。
        // 但本 effect 在 theme.set(next) 后立即触发——早于 VT 回调的异步执行,会抢先改
        // 实时 DOM。VT 动画播的是伪元素快照,实时 DOM 改动会穿透伪元素,表现为「圆形
        // 还没展开到流程图,流程图就瞬切」。is-theme-transitioning 期间跳过,让 VT 回调
        // 内的 registry 重渲染负责;动画结束后此 effect 因 resolved 信号变化重跑(此时
        // is-theme-transitioning 已移除),做幂等兜底。照搬 code_runner/runner.rs 的守卫。
        let transitioning = window
            .document()
            .and_then(|d| d.document_element())
            .map(|el| el.class_list().contains("is-theme-transitioning"))
            .unwrap_or(false);
        if !transitioning {
            invoke_optional_global(
                &window,
                "__initMermaid",
                &[selector.clone().into(), theme_str.into()],
            );
        }

        // lightbox 改由 Dioxus.toml 全局 <script src> 加载（不再 include_str!）。
        // 双保险契约：先设配置,若 lightbox.js 已加载则立即调用;
        // 否则 lightbox.js 加载完后其 IIFE 尾部读到配置自启动。
        let selectors = js_sys::Array::new();
        selectors.push(&selector.into());
        if scope_id.is_none() {
            selectors.push(&".entry-cover".into());
        }
        let selectors_val = js_sys::Object::from(selectors).into();
        if scope_id.is_none() {
            let _ = js_sys::Reflect::set(&window, &"__lightboxSelectors".into(), &selectors_val);
        }
        invoke_optional_global(&window, "__initLightbox", &[selectors_val]);

        // 安装 hash 锚点点击拦截器（幂等）。
        // Dioxus hydration 后其事件委托会接管所有 <a> click（见 handleClickNavigate），
        // 把 hash 锚点当外部 URL 整页刷新。拦截器在 capture 阶段阻止事件到达 Dioxus，
        // 自行 scrollIntoView。initAnchorClick 内部幂等，PostContent 多次挂载也安全。
        invoke_optional_global(&window, "__initAnchorClick", &[]);

        // 内容挂载后若 URL 带 hash，滚动到对应标题。
        // 解决骨架屏阶段标题 DOM 缺失导致浏览器原生 fragment-scroll 失效的问题：
        // PostDetail 用 use_server_future 异步取数，首屏渲染骨架屏，此时标题 DOM
        // 不存在；浏览器尝试滚动到 #hash 找不到目标留在顶部。此处标题已就绪，补一次。
        //
        // 仅首次运行：此 effect 因读取 resolved_theme() 而在主题切换时重跑，但
        // hash 滚动是首屏补救措施，重跑会导致切主题时页面跳回 URL hash 位置。
        if scope_id.is_none() && !did_scroll() {
            did_scroll.set(true);
            invoke_optional_global(&window, "__scrollToHash", &[]);
        }
    });

    #[cfg(target_arch = "wasm32")]
    use_drop(move || {
        if let (Some(window), Some(owner)) = (web_sys::window(), scope_id) {
            invoke_optional_global(&window, "__closeLightboxFor", &[owner.into()]);
        }
    });

    rsx! {
        div { id: scope_id, "data-showcase-lightbox-owner": scope_id, class: "post-content md-content",
            for (i, fragment) in fragments.iter().enumerate() {
                {
                    match fragment {
                        ContentFragment::Html(html) => rsx! {
                            div { key: "html-{i}", dangerous_inner_html: "{html}" }
                        },
                        ContentFragment::Runnable { lang, source, overrides, static_html } => rsx! {
                            CodeRunner {
                                key: "runner-{i}",
                                source: source.clone(),
                                language: lang.clone(),
                                overrides: overrides.clone(),
                                static_html: static_html.clone(),
                                // i 是片段序列中的确定性索引（来自纯函数 split_content_fragments
                                // 对同一 content_html 的解析），SSR 与 hydration 一致，用作容器
                                // id 后缀保证 hydration 时 CodeMirror 能找到 SSR 渲染的容器。
                                instance_id: i,
                            }
                        },
                    }
                }
            }
        }
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn split_plain_html_has_no_runnable() {
        let frags = split_content_fragments("<p>hello</p>");
        assert_eq!(frags.len(), 1);
        assert!(matches!(frags[0], ContentFragment::Html(_)));
    }

    #[test]
    fn split_extracts_runnable_block() {
        let html = r#"<p>intro</p><pre data-runnable="true" data-lang="python" data-overrides="" data-source="print(&#x27;hi&#x27;)"><code class="language-python">print('hi')</code></pre><p>outro</p>"#;
        let frags = split_content_fragments(html);
        // intro + runnable + outro = 3
        assert_eq!(frags.len(), 3);
        match &frags[1] {
            ContentFragment::Runnable {
                lang,
                source,
                overrides,
                static_html,
            } => {
                assert_eq!(lang, "python");
                assert_eq!(source, "print('hi')");
                assert!(overrides.is_none());
                // 静态 HTML 保留完整 <pre> 原文（含高亮与标记），供激活前展示。
                assert!(static_html.starts_with(r#"<pre data-runnable="true""#));
                assert!(static_html.ends_with("</pre>"));
                assert!(static_html.contains("language-python"));
            }
            other => panic!("expected Runnable, got {other:?} (Html)"),
        }
    }

    #[test]
    fn split_parses_overrides_json() {
        let html = r#"<pre data-runnable="true" data-lang="node" data-overrides="{&quot;timeout_secs&quot;:10,&quot;memory_mb&quot;512,&quot;allow_network&quot;:false,&quot;cpu_cores&quot;:1.0,&quot;output_bytes&quot;:1024}" data-source="console.log(1)"><code>x</code></pre>"#;
        // 注意：上面 overrides 故意写成畸形 JSON（缺冒号）→ 解析失败 → overrides 为 None
        let frags = split_content_fragments(html);
        assert_eq!(frags.len(), 1);
        match &frags[0] {
            ContentFragment::Runnable { overrides, .. } => {
                assert!(overrides.is_none(), "畸形 JSON 应解析失败为 None");
            }
            _ => panic!("expected Runnable"),
        }
    }

    #[test]
    fn split_valid_overrides_json() {
        let html = r#"<pre data-runnable="true" data-lang="node" data-overrides="{&quot;timeout_secs&quot;:10,&quot;memory_mb&quot;:512,&quot;allow_network&quot;:false,&quot;cpu_cores&quot;:1.0,&quot;output_bytes&quot;:1024}" data-source="console.log(1)"><code>x</code></pre>"#;
        let frags = split_content_fragments(html);
        match &frags[0] {
            ContentFragment::Runnable { overrides, .. } => {
                let ov = overrides.as_ref().expect("overrides 应解析成功");
                assert_eq!(ov.timeout_secs, 10);
                assert_eq!(ov.memory_mb, 512);
            }
            _ => panic!("expected Runnable"),
        }
    }

    #[test]
    fn split_unclosed_pre_falls_back_to_html() {
        let html = r#"<pre data-runnable="true" data-lang="python""#;
        let frags = split_content_fragments(html);
        assert_eq!(frags.len(), 1);
        assert!(matches!(frags[0], ContentFragment::Html(_)));
    }

    #[test]
    fn decode_html_entities_roundtrip() {
        assert_eq!(decode_html_entities("print(&#x27;hi&#x27;)"), "print('hi')");
        assert_eq!(decode_html_entities("&quot;&lt;&gt;&amp;"), "\"<>&");
    }

    #[test]
    fn fragment_cache_follows_content_changes() {
        let a = split_content_fragments_cached("<p>alpha</p>");
        let a2 = split_content_fragments_cached("<p>alpha</p>");
        assert!(Arc::ptr_eq(&a, &a2), "相同内容应命中缓存复用同一解析结果");
        let b = split_content_fragments_cached("<p>beta</p>");
        assert!(!Arc::ptr_eq(&a, &b), "不同内容不得复用缓存");
        // 超出容量后旧条目被淘汰；重新解析的输出与首次逐值一致。
        for i in 0..(FRAGMENT_CACHE_CAP + 2) {
            split_content_fragments_cached(&format!("<p>filler-{i}</p>"));
        }
        let a3 = split_content_fragments_cached("<p>alpha</p>");
        assert_eq!(a3, a, "淘汰后重新解析输出应与首次一致");
    }
}
