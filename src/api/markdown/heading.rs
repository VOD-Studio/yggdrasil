//! 标题：层级映射、锚点 slug 与目录（TOC）HTML。

/// 将标题纯文本转义，用于安全地拼进 TOC 的 `aria-label="..."` 与 `<a>` 正文。
///
/// 复用 `utils::html::escape_html`（转义 `& < > " '`），避免在仓库内
/// 维护第二份转义实现。原先用 `clean_html` 处理属性上下文会漏掉 `"`，标题形如
/// `" onmouseover="alert(1)` 会越出属性边界。
fn escape_heading_text(s: &str) -> String {
    crate::utils::html::escape_html(s)
}

/// 根据 Markdown 标题层级返回对应的 HTML 标签名。
pub(super) fn heading_tag(level: pulldown_cmark::HeadingLevel) -> &'static str {
    match level {
        pulldown_cmark::HeadingLevel::H1 => "h1",
        pulldown_cmark::HeadingLevel::H2 => "h2",
        pulldown_cmark::HeadingLevel::H3 => "h3",
        pulldown_cmark::HeadingLevel::H4 => "h4",
        pulldown_cmark::HeadingLevel::H5 => "h5",
        pulldown_cmark::HeadingLevel::H6 => "h6",
    }
}

/// Markdown 标题层级转数字（H1 → 1 … H6 → 6），供 TOC 层级比较。
pub(super) fn heading_level(level: pulldown_cmark::HeadingLevel) -> u8 {
    match level {
        pulldown_cmark::HeadingLevel::H1 => 1,
        pulldown_cmark::HeadingLevel::H2 => 2,
        pulldown_cmark::HeadingLevel::H3 => 3,
        pulldown_cmark::HeadingLevel::H4 => 4,
        pulldown_cmark::HeadingLevel::H5 => 5,
        pulldown_cmark::HeadingLevel::H6 => 6,
    }
}

/// 根据标题层级生成嵌套目录 HTML。
pub(super) fn generate_toc_html(headings: &[(u8, String, String)]) -> String {
    use std::fmt::Write as _;

    if headings.is_empty() {
        return String::new();
    }

    // TOC 大小按标题数估算（每个 li+a 约 64 字节起），避免 String::new 的多次 realloc。
    let mut html = String::with_capacity(headings.len() * 64 + 16);
    html.push_str("<ul>");
    let mut stack: Vec<u8> = vec![headings[0].0];

    for (i, (level, text, id)) in headings.iter().enumerate() {
        let level = *level;

        if i > 0 {
            let prev_level = headings[i - 1].0;
            if level > prev_level {
                // 标题层级升高：打开新的嵌套列表。
                for _ in prev_level..level {
                    html.push_str("<ul>");
                    stack.push(level);
                }
            } else if level < prev_level {
                // 标题层级降低：关闭多余的嵌套列表。
                while let Some(top) = stack.last() {
                    if *top > level {
                        html.push_str("</li></ul>");
                        stack.pop();
                    } else {
                        break;
                    }
                }
                html.push_str("</li>");
            } else {
                html.push_str("</li>");
            }
        }

        // 标题 text 是 pulldown-cmark 收集的纯文本（Text/Code 字面字符），不是 HTML 片段，
        // 因此正文与属性两处都走 escape_heading_text（转义 & < > " '）。原先用 clean_html
        // 处理属性上下文会漏掉 `"`，标题中的双引号会越出 aria-label 边界。
        let escaped_text = escape_heading_text(text);
        let _ = write!(
            html,
            "<li><a href=\"#{id}\" aria-label=\"{escaped_text}\">{escaped_text}</a>"
        );
    }

    // Close remaining lists
    // 闭合所有残留的嵌套列表。
    while stack.len() > 1 {
        html.push_str("</li></ul>");
        stack.pop();
    }
    html.push_str("</li></ul>");

    html
}

/// 将标题文本转换为可用于锚点的 slug。
///
/// 汉字转无声调拼音（每字成词，用 `-` 分隔），与 [`crate::api::slug::slugify`]
/// 保持一致；ASCII 字母数字保留，其余字符作为词分隔。结果为空时回退 `heading`。
///
/// 与 `slugify` 有意不合并：这里保留非 ASCII 字母数字（如 `é`、假名）并做 Unicode
/// 小写、`_` 视为分隔符、不截断；改动任一规则都会改变已发布文章的标题锚点。
pub(super) fn slugify_heading(text: &str) -> String {
    use pinyin::ToPinyin;

    let mut slug = String::new();
    let mut prev_dash = true;

    for c in text.to_lowercase().chars() {
        // 汉字优先转拼音；非汉字（含 ascii）返回 None。
        // 每个汉字成词，拼音后补 `-` 与后续内容分隔（连续汉字也会被分开）。
        if let Some(py) = c.to_pinyin() {
            slug.push_str(py.plain());
            slug.push('-');
            prev_dash = true;
        } else if c.is_alphanumeric() {
            slug.push(c);
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }

    if slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        slug.push_str("heading");
    }

    slug
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_heading_simple() {
        assert_eq!(slugify_heading("Hello World"), "hello-world");
    }

    #[test]
    fn slugify_heading_special_chars() {
        assert_eq!(slugify_heading("What's new? (2024)"), "what-s-new-2024");
    }

    #[test]
    fn slugify_heading_chinese() {
        // 汉字逐字转拼音，用 `-` 分隔。
        assert_eq!(slugify_heading("你好世界"), "ni-hao-shi-jie");
    }

    #[test]
    fn slugify_heading_mixed_chinese_ascii() {
        // Rust 入门指南 → rust + ru-men-zhi-nan，词之间用 `-` 分隔。
        assert_eq!(slugify_heading("Rust 入门指南"), "rust-ru-men-zhi-nan");
    }

    #[test]
    fn slugify_heading_collapses_dashes() {
        assert_eq!(slugify_heading("a--b"), "a-b");
    }

    #[test]
    fn slugify_heading_strips_trailing_dash() {
        assert_eq!(slugify_heading("hello!"), "hello");
    }

    #[test]
    fn slugify_heading_empty_returns_heading() {
        assert_eq!(slugify_heading(""), "heading");
    }

    #[test]
    fn generate_toc_html_empty() {
        assert_eq!(generate_toc_html(&[]), "");
    }

    #[test]
    fn generate_toc_html_single_heading() {
        let headings = vec![(2u8, "Title".to_string(), "title".to_string())];
        let html = generate_toc_html(&headings);
        assert!(html.contains("href=\"#title\""));
        assert!(html.contains("<ul>"));
        assert!(html.contains("</ul>"));
    }

    #[test]
    fn generate_toc_html_nested() {
        let headings = vec![
            (2u8, "Chapter".to_string(), "chapter".to_string()),
            (3u8, "Section".to_string(), "section".to_string()),
        ];
        let html = generate_toc_html(&headings);
        assert!(html.contains("href=\"#chapter\""));
        assert!(html.contains("href=\"#section\""));
        let ul_count = html.matches("<ul>").count();
        assert_eq!(ul_count, 2);
    }

    #[test]
    fn generate_toc_html_escapes_quote_in_attr() {
        // 标题中的双引号不得越出 aria-label 属性边界。
        let headings = vec![(
            2u8,
            "\" onmouseover=\"alert(1)".to_string(),
            "heading".to_string(),
        )];
        let html = generate_toc_html(&headings);
        // aria-label 中的双引号被转义为 &quot;，无法越出属性边界注入新属性。
        assert!(
            html.contains("aria-label=\"&quot; onmouseover=&quot;alert(1)\""),
            "aria-label 应转义内部双引号，got: {html}"
        );
        // 关键：不得出现「未被引号包裹、可被解析为真实属性」的 onmouseover= 片段。
        // 正文中作为纯文本出现 "onmouseover" 字符串是安全的（无 < 或属性结构）。
        let attr_injection = "\" onmouseover=\"";
        let injected = html.matches(attr_injection).count();
        // 原始输入里有 1 个裸双引号起头；转义后该模式不应再作为属性边界出现。
        // 注意 aria-label 内部的双引号已变成 &quot;，因此裸的 `" onmouseover="` 不应存在。
        assert_eq!(
            injected, 0,
            "不应存在未转义的属性边界 `\" onmouseover=\"`，got: {html}"
        );
    }

    #[test]
    fn generate_toc_html_escapes_ampersand_in_attr() {
        let headings = vec![(2u8, "A & B".to_string(), "heading".to_string())];
        let html = generate_toc_html(&headings);
        assert!(
            html.contains("aria-label=\"A &amp; B\""),
            "& 应在属性中转义，got: {html}"
        );
    }

    #[test]
    fn generate_toc_html_escapes_less_than_in_attr() {
        // `<` 在属性与正文中都应被转义，避免被误解析为标签起始。
        let headings = vec![(2u8, "a < b".to_string(), "heading".to_string())];
        let html = generate_toc_html(&headings);
        assert!(
            html.contains("aria-label=\"a &lt; b\""),
            "< 应在属性中转义，got: {html}"
        );
        assert!(!html.contains("a < b"));
    }
}
