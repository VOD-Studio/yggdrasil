//! 评论 Markdown 渲染与 HTML 清洗。
//!
//! 对评论内容做轻量 Markdown 解析，限制标签白名单并转义危险字符。
//! 仅在 `feature = "server"` 启用的服务端构建中实际执行渲染。

#![allow(clippy::unused_unit, deprecated)]

/// 清洗评论 HTML，移除危险标签与属性。
///
/// 实际委托给 `crate::api::sanitizer::clean_comment_html` 实现。
#[cfg(feature = "server")]
pub(crate) fn clean_comment_html(input: &str) -> String {
    crate::api::sanitizer::clean_comment_html(input)
}

/// 将评论 Markdown 渲染为安全的 HTML。
///
/// 支持表格与删除线；标题统一渲染为 `<strong>` 以避免层级混乱；
/// 代码块若指定语言则调用服务端高亮，否则转义 HTML；
/// 最终调用 `clean_comment_html` 过滤危险内容。
#[cfg(feature = "server")]
pub fn render_comment_markdown(md: &str) -> String {
    use pulldown_cmark::{CodeBlockKind, Event, Options, Tag, TagEnd};

    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_MATH;
    let parser = pulldown_cmark::Parser::new_ext(md, opts);

    let mut events: Vec<Event> = Vec::new();
    let mut in_codeblock = false;
    let mut code_lang: Option<String> = None;
    let mut code_buffer = String::new();

    // 逐事件处理 Markdown AST，转换标题并收集代码块内容。
    for event in parser {
        match event {
            Event::InlineMath(tex) => {
                // 内联公式直接渲染成 HTML 注入事件流（评论不使用块级段落包裹）。
                let html = crate::api::katex::render_inline(&tex);
                events.push(Event::Html(html.into()));
            }
            Event::DisplayMath(tex) => {
                // 评论里的块级公式：KaTeX 输出本身已含 .katex-display 居中样式，
                // 无需额外 <p>（评论渲染较紧凑，避免引入多余段间距）。
                let html = crate::api::katex::render_display(&tex);
                events.push(Event::Html(html.into()));
            }
            Event::Start(Tag::Heading { .. }) => {
                // 评论中不保留标题层级，统一加粗。
                events.push(Event::Start(Tag::Strong));
            }
            Event::End(TagEnd::Heading(_)) => {
                events.push(Event::End(TagEnd::Strong));
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_codeblock = true;
                code_lang = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                    _ => None,
                };
                code_buffer.clear();
            }
            Event::Text(text) if in_codeblock => {
                code_buffer.push_str(&text);
            }
            Event::End(TagEnd::CodeBlock) => {
                // 根据是否有语言信息决定高亮或转义。
                let html = if let Some(ref lang) = code_lang {
                    let highlighted =
                        crate::highlight::server::highlight_code(&code_buffer, Some(lang));
                    format!("<pre><code>{}</code></pre>", highlighted)
                } else {
                    format!(
                        "<pre><code>{}</code></pre>",
                        crate::utils::html::escape_html(&code_buffer)
                    )
                };
                events.push(Event::Html(html.into()));
                in_codeblock = false;
            }
            _ if !in_codeblock => {
                events.push(event);
            }
            _ => {}
        }
    }

    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events.into_iter());
    clean_comment_html(&html)
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn render_comment_heading_converted_to_strong() {
        let result = render_comment_markdown("## Hello World");
        assert!(result.contains("<strong>Hello World</strong>"));
        assert!(!result.contains("<h2>"));
    }

    #[test]
    fn render_comment_heading_all_levels() {
        for md in &[
            "# H1",
            "## H2",
            "### H3",
            "#### H4",
            "##### H5",
            "###### H6",
        ] {
            let result = render_comment_markdown(md);
            assert!(
                result.contains("<strong>"),
                "heading not converted for: {}",
                md
            );
        }
    }

    #[test]
    fn render_comment_paragraph() {
        let result = render_comment_markdown("Hello **world**");
        assert!(result.contains("<strong>world</strong>"));
    }

    #[test]
    fn render_comment_code_block_with_language() {
        let result = render_comment_markdown("```rust\nfn main() {}\n```");
        assert!(result.contains("<pre><code>"));
        assert!(result.contains("main"));
    }

    #[test]
    fn render_comment_code_block_without_language() {
        let result = render_comment_markdown("```\nplain text\n```");
        assert!(result.contains("<pre><code>"));
        assert!(result.contains("plain text"));
    }

    #[test]
    fn render_comment_code_block_without_language_escapes_html() {
        let result = render_comment_markdown("```\n<div>alert('xss')</div>\n```");
        assert!(result.contains("&lt;div&gt;"));
        assert!(!result.contains("<div>"));
    }

    #[test]
    fn render_comment_strips_script() {
        let result = render_comment_markdown("<script>alert('xss')</script>");
        assert!(!result.contains("script"));
    }

    #[test]
    fn render_comment_renders_img() {
        // 评论支持图片：站内上传路径与 https 图床都渲染为 <img>，src/alt 保留。
        let result = render_comment_markdown("![截图](/uploads/2026/08/a.webp)");
        assert!(result.contains("<img"), "应渲染 img: {result}");
        assert!(
            result.contains(r#"src="/uploads/2026/08/a.webp""#),
            "src 保留: {result}"
        );
        assert!(result.contains(r#"alt="截图""#), "alt 保留: {result}");

        let result = render_comment_markdown("![alt](https://example.com/img.png)");
        assert!(
            result.contains(r#"src="https://example.com/img.png""#),
            "https 图床保留: {result}"
        );
    }

    #[test]
    fn render_comment_img_unsafe_src_removed() {
        // 危险 src（javascript:/data:）被 sanitizer 剥除。
        let result = render_comment_markdown("![x](javascript:alert(1))");
        assert!(
            !result.contains("javascript:"),
            "危险 src 必须清除: {result}"
        );
    }

    #[test]
    fn render_comment_link_has_nofollow() {
        let result = render_comment_markdown("[link](https://example.com)");
        assert!(result.contains("nofollow"));
        assert!(result.contains("noopener"));
    }

    #[test]
    fn render_comment_link_javascript_removed() {
        let result = render_comment_markdown("[click](javascript:alert(1))");
        assert!(result.contains("click"));
        assert!(!result.contains("javascript:"));
    }

    #[test]
    fn render_comment_onerror_attribute_removed() {
        let result = render_comment_markdown("<div onerror=\"alert(1)\">text</div>");
        assert!(result.contains("text"));
        assert!(!result.contains("onerror"));
    }

    #[test]
    fn render_comment_link_data_uri_removed() {
        let result = render_comment_markdown("[click](data:text/html,<script>alert(1)</script>)");
        assert!(result.contains("click"));
        assert!(!result.contains("data:"));
    }

    #[test]
    fn render_comment_code_block_escapes_html_entities() {
        let result = render_comment_markdown("```\n&amp;\n```");
        assert!(result.contains("&amp;amp;"));
    }

    #[test]
    fn render_comment_no_id_attribute() {
        let result = render_comment_markdown("<div id=\"test\">text</div>");
        assert!(!result.contains("id="));
    }

    #[test]
    fn render_comment_table() {
        let result = render_comment_markdown("| a | b |\n|---|---|\n| 1 | 2 |");
        assert!(result.contains("<table>"));
    }

    #[test]
    fn render_comment_strikethrough() {
        let result = render_comment_markdown("~~deleted~~");
        assert!(result.contains("<del>deleted</del>"));
    }

    #[test]
    fn render_comment_inline_code() {
        let result = render_comment_markdown("Use `println!` to print");
        assert!(result.contains("<code>println!</code>"));
    }

    #[test]
    fn clean_comment_html_removes_details_summary() {
        let result =
            clean_comment_html("<details><summary>Click</summary><p>Content</p></details>");
        assert!(!result.contains("details"));
        assert!(!result.contains("summary"));
    }

    #[test]
    fn clean_comment_html_removes_data_uri() {
        let result =
            clean_comment_html("<a href=\"data:text/html,<script>alert(1)</script>\">click</a>");
        assert!(!result.contains("data:"));
    }

    #[test]
    fn render_comment_empty() {
        let result = render_comment_markdown("");
        assert!(result.is_empty());
    }

    #[test]
    fn render_comment_heading_with_inline_code() {
        let result = render_comment_markdown("## Using `foo()`");
        assert!(result.contains("<strong>"));
        assert!(result.contains("<code>foo()</code>"));
        assert!(!result.contains("<h2>"));
    }

    #[test]
    fn render_comment_inline_math() {
        // 评论里的 $...$ 内联公式：ENABLE_MATH 解析 → katex 渲染 → sanitizer 放行 span。
        let result = render_comment_markdown("方程 $a^2 + b^2 = c^2$ 是勾股定理");
        assert!(
            result.contains("katex"),
            "评论内联公式应渲染为 katex span, got: {}",
            result
        );
        assert!(result.contains("方程"));
        assert!(result.contains("勾股定理"));
    }

    #[test]
    fn render_comment_display_math() {
        // 评论里的 $$...$$ 块级公式。
        let result = render_comment_markdown("$$\\int_0^1 x\\,dx$$");
        assert!(
            result.contains("katex-display"),
            "评论块级公式应含 katex-display, got: {}",
            result
        );
    }

    #[test]
    fn render_comment_math_span_style_preserved() {
        // KaTeX 内联 style（垂直对齐）必须经 clean_comment_html 保留,
        // 否则公式在评论里排版错位。验证 span 的 style 属性未被剥离。
        let result = render_comment_markdown("$x^2$");
        assert!(
            result.contains("style=\""),
            "评论 katex span 的 style 应保留, got: {}",
            result
        );
    }
}
