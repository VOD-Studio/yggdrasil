//! 围栏代码块：runnable 标记解析与 `<pre><code>` 输出（mermaid / syntect 高亮 / 可运行）。

use std::fmt::Write as _;

/// 解析围栏 info：识别 `runnable` 标记与可选 ResourceLimits JSON 覆盖。
///
/// 仅在「标记为 runnable 且语言受支持」时返回 `(lang, html-escaped overrides JSON)`，
/// 供阅读器扫描 data-* 挂载运行器；其余情况返回 `None`。
pub(super) fn runnable_attrs(info: &str) -> Option<(String, String)> {
    let (lang, runnable, overrides) = crate::api::code_runner::languages::parse_fence_info(info);
    if runnable && crate::api::code_runner::languages::is_supported_lang(&lang) {
        // overrides 序列化为 JSON 后 HTML 转义，避免属性注入。
        let ov_json = overrides
            .map(|o| serde_json::to_string(&o).unwrap_or_default())
            .unwrap_or_default();
        let overrides_escaped = crate::utils::html::escape_html(&ov_json);
        Some((lang, overrides_escaped))
    } else {
        None
    }
}

/// 输出一个完整的围栏代码块 `<pre><code>…</code></pre>`。
///
/// `info` 为围栏 info 字符串，`runnable` 为 [`runnable_attrs`] 的结果，`source` 为原始源码。
pub(super) fn push_code_block(
    html: &mut String,
    info: Option<&str>,
    runnable: Option<(String, String)>,
    source: &str,
) {
    // mermaid 代码块：前端 yggdrasil-core 扫描 language-mermaid 渲染成 SVG，
    // 源码不应被 syntect 高亮（无语法定义，且会包 <span> 污染 textContent 提取）。
    // 直接输出转义后的纯源码，前端 textContent 无损拿到原始 mermaid 文本。
    let is_mermaid = info
        .map(|l| l.split_whitespace().next() == Some("mermaid"))
        .unwrap_or(false);
    if is_mermaid {
        let escaped = crate::utils::html::escape_html(source);
        html.push_str(r#"<pre><code class="language-mermaid">"#);
        html.push_str(&escaped);
        html.push_str("</code></pre>");
        return;
    }
    // 使用 syntect 对代码块进行服务端语法高亮。围栏信息可能带修饰
    //（`python runnable {...}`），只取首个语言 token。
    let highlight_lang = info.and_then(|lang| lang.split_whitespace().next());
    let highlighted = crate::highlight::server::highlight_code(source, highlight_lang);
    // 可运行代码块：在 <pre> 上挂 data-runnable / data-lang / data-overrides / data-source，
    // 阅读者（post_content.rs）客户端扫描这些标记原地挂载 CodeRunner 组件。
    // data-source 为 HTML 转义后的原始源码，供阅读器无损提取（避免反解高亮 HTML）。
    if let Some((lang, overrides_escaped)) = runnable {
        let source_escaped = crate::utils::html::escape_html(source);
        let _ = write!(
            html,
            r#"<pre data-runnable="true" data-lang="{lang}" data-overrides="{overrides_escaped}" data-source="{source_escaped}"><code class="language-{lang}">"#
        );
    } else {
        html.push_str("<pre><code");
        if let Some(lang) = info {
            // 围栏语言可能含 info 修饰（如 `python runnable {...}`），
            // 高亮的 language-xxx 取纯语言 token（首个空白前）。
            let clean_lang = lang.split_whitespace().next().unwrap_or("");
            if !clean_lang.is_empty() {
                let _ = write!(html, " class=\"language-{clean_lang}\"");
            }
        }
        html.push('>');
    }
    html.push_str(&highlighted);
    html.push_str("</code></pre>");
}
