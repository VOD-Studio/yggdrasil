//! 文章内容组件
//!
//! 渲染由服务端生成的文章 HTML 内容，并在 WASM 前端初始化交互脚本。
//!
//! 可运行代码块（markdown 围栏 ` ```lang runnable `）在服务端渲染为带
//! `data-runnable` / `data-lang` / `data-overrides` / `data-source` 的 `<pre>`。
//! 本组件在渲染前把 `content_html` 拆成片段序列：普通 HTML 文本片段 +
//! [`crate::components::code_runner::CodeRunner`] 组件，使可运行块作为
//! Leptos 视图内的一等元素渲染（而非手动篡改 DOM，避免 hydration 冲突）。

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use leptos::prelude::*;

use crate::components::code_runner::CodeRunner;
use crate::components::enhance_assets::{EnhanceScript, LightboxAssets};
use crate::infra::runner_config::ResourceLimits;
#[cfg(target_arch = "wasm32")]
use crate::utils::js::invoke_optional_global;

/// 内容片段：普通 HTML 文本，或一个可运行代码块。
#[derive(Clone, PartialEq, Debug)]
enum ContentFragment {
    /// 原始 HTML 片段（含语法高亮等），直接以 `inner_html` 渲染。
    Html(String),
    /// 可运行代码块：语言、源码、可选资源覆盖，以及服务端高亮好的代码 HTML
    /// （编辑器挂载前原样显示，SSR 与 hydration 输出一致）。
    Runnable {
        lang: String,
        source: String,
        overrides: Option<ResourceLimits>,
        highlighted: String,
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
            .and_then(|s| serde_json::from_slice::<ResourceLimits>(s.as_bytes()).ok());
        let source = extract_attr(pre_block, "data-source").unwrap_or_default();
        let highlighted = code_inner_html(pre_block).to_string();

        fragments.push(ContentFragment::Runnable {
            lang,
            source,
            overrides,
            highlighted,
        });
    }

    if !rest.trim().is_empty() {
        fragments.push(ContentFragment::Html(rest.to_string()));
    }

    fragments
}

/// `<pre ...><code ...>内容</code></pre>` 中 `<code>` 的内部 HTML；结构不符时为空。
fn code_inner_html(block: &str) -> &str {
    let Some(code_start) = block.find("<code") else {
        return "";
    };
    let after_tag = &block[code_start..];
    let Some(open_end) = after_tag.find('>') else {
        return "";
    };
    let inner = &after_tag[open_end + 1..];
    inner.rfind("</code>").map_or("", |end| &inner[..end])
}

// ============================================================================
// 片段解析缓存（P4.2）。
//
// 组件函数体虽只运行一次，但同一进程内 SSR 会反复渲染同一批文章，浏览器切页也会重复
// 解析同一篇长文——整篇 content_html 的重新扫描并分配全部片段字符串，长文 + 多运行块时
// 是明显的主线程尖峰。这里按内容哈希做进程内有界缓存：
// - 命中需通过全文相等核对（条目保留 Arc<str> 原文），哈希碰撞不会错渲染。
// - 有界（8 篇，插入序淘汰）：服务端 SSR 渲染不同文章、浏览器切页都不会无界
//   增长；缓存只跳过解析，输出逐字节相同。
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
/// - `scope_id`：可选的局部正文 ID（省略即不传，不要传 `Some(..)`）；图鉴只增强该实例，
///   正式页面使用默认范围。
///
/// 关键行为：
/// - 把可运行代码块拆成 [`CodeRunner`] 组件穿插渲染，其余 HTML 片段照旧。
/// - 在 `target_arch = "wasm32"` 环境下调用 `window.__initPostContent` 初始化代码块
///   复制按钮；P5 §8.2 起正文增强在 `yggdrasil-enhance.js`、灯箱在 `lightbox.js`，
///   均由本组件按需声明注入（见 [`crate::components::enhance_assets`]），
///   这里仅设置初始化配置 `__lightboxSelectors` 并兜底调用。
///
/// 组件函数体只运行一次：同一实例内 `content_html` 不会更新，上下篇切换须由调用方按
/// slug 重建本组件（片段解析按内容哈希缓存，重建不重复解析）。
#[component]
pub fn PostContent(
    #[prop(into)] content_html: String,
    #[prop(optional)] scope_id: Option<&'static str>,
) -> impl IntoView {
    let fragments = split_content_fragments_cached(&content_html);

    // 正文挂载后初始化交互脚本：不读取任何信号，只运行一次。
    #[cfg(target_arch = "wasm32")]
    {
        Effect::new(move || {
            let Some(window) = web_sys::window() else {
                return;
            };

            // 图鉴实例只增强自己的正文，正式页面仍按原选择器处理。
            let selector =
                scope_id.map_or_else(|| ".post-content".to_string(), |id| format!("#{id}"));
            // 调用 window.__initPostContent：函数不存在时静默跳过。
            invoke_optional_global(&window, "__initPostContent", &[selector.clone().into()]);

            // Mermaid 图表已由服务端渲染成浅/深两份 SVG（CSS 按主题切换），这里只绑定点击放大。
            invoke_optional_global(&window, "__initMermaid", &[selector.clone().into()]);

            // 灯箱双保险契约：先设配置，若 lightbox.js 已加载则立即调用；
            // 否则 lightbox.js 加载完后其 IIFE 尾部读到配置自启动。
            let selectors = js_sys::Array::new();
            selectors.push(&selector.into());
            if scope_id.is_none() {
                selectors.push(&".entry-cover".into());
            }
            let selectors_val = js_sys::Object::from(selectors).into();
            if scope_id.is_none() {
                let _ =
                    js_sys::Reflect::set(&window, &"__lightboxSelectors".into(), &selectors_val);
            }
            invoke_optional_global(&window, "__initLightbox", &[selectors_val]);

            // 安装 hash 锚点点击拦截器（幂等，PostContent 多次挂载也安全）。
            invoke_optional_global(&window, "__initAnchorClick", &[]);

            // 内容挂载后若 URL 带 hash，滚动到对应标题：首屏渲染骨架屏阶段标题 DOM 缺失，
            // 浏览器原生 fragment-scroll 失效，此处标题已就绪，补一次。
            if scope_id.is_none() {
                invoke_optional_global(&window, "__scrollToHash", &[]);
            }
        });

        on_cleanup(move || {
            if let (Some(window), Some(owner)) = (web_sys::window(), scope_id) {
                invoke_optional_global(&window, "__closeLightboxFor", &[owner.into()]);
            }
        });
    }

    let body = fragments
        .iter()
        .enumerate()
        .map(|(i, fragment)| match fragment {
            ContentFragment::Html(html) => view! { <div inner_html=html.clone()></div> }.into_any(),
            ContentFragment::Runnable {
                lang,
                source,
                overrides,
                highlighted,
            } => view! {
                // i 是片段序列中的确定性索引（来自纯函数 split_content_fragments 对同一
                // content_html 的解析），SSR 与 hydration 一致，用作容器 id 后缀保证
                // hydration 时 CodeMirror 能找到 SSR 渲染的容器。
                <CodeRunner
                    source=source.clone()
                    language=lang.clone()
                    overrides=overrides.clone()
                    highlighted=Some(highlighted.clone()).filter(|h| !h.is_empty())
                    instance_id=i
                />
            }
            .into_any(),
        })
        .collect_view();

    view! {
        // P5 §8.2：正文增强与灯箱按需注入。SSR 直达烤进 head 带 defer，先于
        // hydration；SPA 首跳脚本晚于 Effect 时由增强调用队列兜底。
        <EnhanceScript/>
        <LightboxAssets/>
        <div id=scope_id data-showcase-lightbox-owner=scope_id class="post-content md-content">
            {body}
        </div>
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
                highlighted,
            } => {
                assert_eq!(lang, "python");
                assert_eq!(source, "print('hi')");
                assert!(overrides.is_none());
                assert_eq!(highlighted, "print('hi')");
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
