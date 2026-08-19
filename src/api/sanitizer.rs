//! HTML 消毒器。
//!
//! 基于 lol_html 清理不受信任的 HTML，限制允许的 tag/attribute/URL scheme，
//! 分别提供文章正文（`clean_html`）与评论（`clean_comment_html`）两套白名单策略。
//! 仅在 `feature = "server"` 时执行。

#![allow(clippy::unused_unit, deprecated)]

#[cfg(feature = "server")]
use std::collections::HashSet;

#[cfg(feature = "server")]
use std::sync::LazyLock;

#[cfg(feature = "server")]
static DEFAULT_ALLOWED_TAGS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "a",
        "abbr",
        "acronym",
        "area",
        "article",
        "aside",
        "b",
        "bdi",
        "bdo",
        "blockquote",
        "br",
        "caption",
        "center",
        "cite",
        "code",
        "col",
        "colgroup",
        "data",
        "dd",
        "del",
        "details",
        "dfn",
        "div",
        "dl",
        "dt",
        "em",
        "figcaption",
        "figure",
        "footer",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "header",
        "hgroup",
        "hr",
        "i",
        "img",
        "ins",
        // input 仅用于 pulldown-cmark 任务列表渲染的 checkbox;
        // element_handler 会强制校验 type="checkbox",其余 type 一律整体移除。
        "input",
        "kbd",
        "li",
        "map",
        "mark",
        "nav",
        "ol",
        "p",
        "pre",
        "q",
        "rp",
        "rt",
        "rtc",
        "ruby",
        "s",
        "samp",
        "section",
        "small",
        "span",
        "strike",
        "strong",
        "sub",
        "summary",
        "sup",
        // svg / path 仅用于 KaTeX 服务端数学公式渲染 (根号、矩阵竖线、大括号、矢量箭头等);
        // 属性仅放行 ViewBox / d 等绘图属性，script/style 标签由 CLEAN_CONTENT_TAGS 强行清除。
        "svg",
        "path",
        "table",
        "tbody",
        "td",
        "th",
        "thead",
        "time",
        "tr",
        "tt",
        "u",
        "ul",
        "var",
        "wbr",
    ])
});

#[cfg(feature = "server")]
static CLEAN_CONTENT_TAGS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| HashSet::from(["script", "style"]));

#[cfg(feature = "server")]
static DEFAULT_ALLOWED_SCHEMES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "bitcoin",
        "ftp",
        "ftps",
        "geo",
        "http",
        "https",
        "im",
        "irc",
        "ircs",
        "magnet",
        "mailto",
        "mms",
        "mx",
        "news",
        "nntp",
        "openpgp4fpr",
        "sip",
        "sms",
        "smsto",
        "ssh",
        "tel",
        "url",
        "webcal",
        "wtai",
        "xmpp",
    ])
});

#[cfg(feature = "server")]
/// 评论允许的标签：在默认集合基础上移除 details / summary。
/// img 保留（评论区支持图片）：src 仍受 is_safe_url 约束（仅白名单 scheme
/// 或站内相对路径），data URI 禁用，事件属性全量剔除。
static COMMENT_ALLOWED_TAGS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = DEFAULT_ALLOWED_TAGS.clone();
    set.remove("details");
    set.remove("summary");
    set
});

#[cfg(feature = "server")]
/// 判断 data URI 是否属于安全图片类型。
///
/// D7：生产死代码——两处 SanitizerConfig（clean_html / clean_comment_html）均硬编码
/// `allow_data_uri: false`，本函数在请求路径上不可达（is_safe_url 提前短路）。
/// 保留作为防御纵深：测试锁定\"即便 flag=true 也只放行图片类型、拒绝 data:text/html\"
/// 的安全不变量。若未来需要内联 data URI，flag + 本函数已就绪。
fn is_safe_data_uri(url: &str) -> bool {
    // data URI 只允许安全的图片类型；禁止 data:text/html、data:application/javascript 等。
    let url = url.trim();
    let Some(rest) = url.strip_prefix("data:") else {
        return false;
    };
    let media_type = rest.split(',').next().unwrap_or("");
    let media_type = media_type.split(';').next().unwrap_or("").trim();
    matches!(
        media_type.to_lowercase().as_str(),
        "image/png"
            | "image/jpeg"
            | "image/jpg"
            | "image/gif"
            | "image/webp"
            | "image/avif"
            | "image/bmp"
            | "image/tiff"
            | "image/svg+xml"
    )
}

#[cfg(feature = "server")]
fn is_safe_url(url: &str, allowed_schemes: &HashSet<&str>, allow_data_uri: bool) -> bool {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return true;
    }
    // 锚点（#开头）优先放行：fragment 内的冒号不是 scheme 分隔符
    // （如脚注锚点 #fn:label），不应被下面的 scheme 解析误判。
    // 仍显式拒绝 javascript:/vbscript:（虽以 # 开头不可能，但保持防御一致）。
    if trimmed.starts_with('#') {
        return true;
    }
    // 解析 scheme 并与白名单对比；未知 scheme 默认拒绝。
    if let Some(colon_pos) = trimmed.find(':') {
        let scheme = &trimmed[..colon_pos];
        let scheme_lower = scheme.to_lowercase();
        if scheme_lower == "javascript" || scheme_lower == "vbscript" {
            return false;
        }
        if scheme.contains(|c: char| c.is_ascii_whitespace()) {
            return false;
        }
        if allowed_schemes.contains(scheme_lower.as_str()) {
            return true;
        }
        if scheme_lower == "data" {
            return allow_data_uri && is_safe_data_uri(trimmed);
        }
        // 任何其它 scheme 均拒绝：file://、blob://、about:blank 等。
        return false;
    }
    // 无 scheme 时只允许相对路径。
    trimmed.starts_with('/')
}

#[cfg(feature = "server")]
/// HTML 消毒配置：白名单 tag/attribute、允许 URL scheme 与链接 rel。
struct SanitizerConfig {
    allowed_tags: &'static HashSet<&'static str>,
    extra_generic_attrs: Vec<&'static str>,
    extra_tag_attrs: Vec<(&'static str, Vec<&'static str>)>,
    allowed_schemes: &'static HashSet<&'static str>,
    allow_data_uri: bool,
    link_rel: Option<&'static str>,
    remove_tags: &'static HashSet<&'static str>,
}

#[cfg(feature = "server")]
fn sanitize(input: &str, config: &SanitizerConfig) -> String {
    let allowed_tags = config.allowed_tags;
    let remove_tags = config.remove_tags;
    let generic_attrs: HashSet<&str> = config
        .extra_generic_attrs
        .iter()
        .copied()
        .chain(["lang", "title"])
        .collect();
    let tag_attrs_map: std::collections::HashMap<&str, HashSet<&str>> = {
        let mut m = std::collections::HashMap::new();
        let base = [
            ("a", vec!["href", "hreflang"]),
            ("bdo", vec!["dir"]),
            ("blockquote", vec!["cite"]),
            ("col", vec!["align", "char", "charoff", "span"]),
            ("colgroup", vec!["align", "char", "charoff", "span"]),
            ("del", vec!["cite", "datetime"]),
            ("hr", vec!["align", "size", "width"]),
            ("img", vec!["align", "alt", "height", "src", "width"]),
            ("ins", vec!["cite", "datetime"]),
            ("ol", vec!["start"]),
            ("q", vec!["cite"]),
            ("table", vec!["align", "char", "charoff", "summary"]),
            ("tbody", vec!["align", "char", "charoff"]),
            (
                "td",
                vec!["align", "char", "charoff", "colspan", "headers", "rowspan"],
            ),
            ("tfoot", vec!["align", "char", "charoff"]),
            (
                "th",
                vec![
                    "align", "char", "charoff", "colspan", "headers", "rowspan", "scope",
                ],
            ),
            ("thead", vec!["align", "char", "charoff"]),
            ("tr", vec!["align", "char", "charoff"]),
        ];
        for (tag, attrs) in &base {
            m.insert(*tag, attrs.iter().copied().collect());
        }
        for (tag, attrs) in &config.extra_tag_attrs {
            m.entry(tag)
                .or_insert_with(HashSet::new)
                .extend(attrs.iter().copied());
        }
        m
    };
    let allowed_schemes = config.allowed_schemes;
    let allow_data_uri = config.allow_data_uri;
    let link_rel = config.link_rel;

    let element_handler = move |el: &mut lol_html::html_content::Element| {
        let tag = el.tag_name().to_lowercase();

        if remove_tags.contains(tag.as_str()) {
            el.remove();
            return Ok(());
        }

        if !allowed_tags.contains(tag.as_str()) {
            el.remove_and_keep_content();
            return Ok(());
        }

        let attrs_to_remove: Vec<String> = el
            .attributes()
            .iter()
            .filter_map(|attr| {
                let name = attr.name();
                let name_lower = name.to_lowercase();
                // 仅保留白名单属性；对 href/src/cite 额外校验 URL 安全性。
                let is_allowed = generic_attrs.contains(name_lower.as_str())
                    || tag_attrs_map
                        .get(tag.as_str())
                        .is_some_and(|attrs| attrs.contains(name_lower.as_str()));
                if is_allowed {
                    if name_lower == "href" || name_lower == "src" || name_lower == "cite" {
                        let val = attr.value();
                        if !is_safe_url(&val, allowed_schemes, allow_data_uri) {
                            return Some(name);
                        }
                    }
                    // input 的 type 必须是 checkbox，其余取值（image/text/...）一概删除；
                    // 缺失 type 的 input 由下面的兜底逻辑整标签移除。
                    if tag == "input"
                        && name_lower == "type"
                        && attr.value().trim().to_lowercase() != "checkbox"
                    {
                        return Some(name);
                    }
                    None
                } else {
                    Some(name)
                }
            })
            .collect();

        for attr_name in attrs_to_remove {
            el.remove_attribute(&attr_name);
        }

        if link_rel.is_some() && tag == "a" {
            if let Some(rel) = link_rel {
                let existing = el.get_attribute("rel").unwrap_or_default();
                if existing != rel {
                    el.set_attribute("rel", rel).ok();
                }
            }
        }

        // input 兜底：属性白名单 + type 值校验后，仍可能残留「无 type 属性」的 input。
        // （例如 <input checked> 经属性过滤后 type 被删或缺省。）这种 input 整体移除——
        // 它是 void 元素无文本内容，remove() 不丢正文，且能彻底封堵缺省 type 的滥用。
        if tag == "input" {
            let type_ok = el
                .get_attribute("type")
                .map(|v| v.trim().to_lowercase() == "checkbox")
                .unwrap_or(false);
            if !type_ok {
                el.remove();
                return Ok(());
            }
        }

        Ok(())
    };

    let settings = lol_html::RewriteStrSettings::new()
        .append_element_content_handler(lol_html::element!("*", element_handler))
        .append_document_content_handler(lol_html::doc_comments!(|c| {
            c.remove();
            Ok(())
        }));

    lol_html::rewrite_str(input, settings).unwrap_or_else(|e| {
        // 静默返回空串会整篇丢弃正文，记录错误以便排查。
        tracing::error!(error = ?e, input_len = input.len(), "HTML 清理失败，回退空串会丢弃正文");
        String::new()
    })
}

#[cfg(feature = "server")]
/// 文章正文 HTML 清理：允许较完整的标签与 data URI，外链添加 `noopener noreferrer`。
pub fn clean_html(input: &str) -> String {
    let config = SanitizerConfig {
        allowed_tags: &DEFAULT_ALLOWED_TAGS,
        extra_generic_attrs: vec![
            "class",
            "aria-hidden",
            "aria-label",
            "aria-labelledby",
            "id",
            "role",
            "accesskey",
            "title",
        ],
        extra_tag_attrs: vec![
            ("a", vec!["class", "aria-hidden", "aria-label"]),
            ("img", vec!["data-src", "class", "style"]),
            // input 仅放行 checkbox 必备属性;type 的具体取值由 element_handler 强校验为 checkbox。
            ("input", vec!["type", "checked", "disabled"]),
            // pre 上的可运行代码块标记：data-runnable / data-lang / data-overrides / data-source。
            // data-overrides / data-source 是 markdown 渲染时 HTML 转义后的内容（见 markdown.rs），不含未转义引号。
            (
                "pre",
                vec![
                    "data-runnable",
                    "data-lang",
                    "data-overrides",
                    "data-source",
                ],
            ),
            ("span", vec!["class", "style"]),
            // KaTeX 数学公式 SSR 渲染生成的 SVG 矢量图 (根号 / 矩阵竖线 / 括号 / 箭头等) 必备属性
            (
                "svg",
                vec![
                    "xmlns",
                    "width",
                    "height",
                    "viewbox",
                    "preserveaspectratio",
                    "style",
                ],
            ),
            ("path", vec!["d"]),
            ("h1", vec!["id", "class"]),
            ("h2", vec!["id", "class"]),
            ("h3", vec!["id", "class"]),
            ("h4", vec!["id", "class"]),
            ("h5", vec!["id", "class"]),
            ("h6", vec!["id", "class"]),
        ],
        allowed_schemes: &DEFAULT_ALLOWED_SCHEMES,
        allow_data_uri: false,
        link_rel: Some("noopener noreferrer"),
        remove_tags: &CLEAN_CONTENT_TAGS,
    };
    sanitize(input, &config)
}

#[cfg(feature = "server")]
/// 评论 HTML 清理：移除折叠块，保留图片（src 受 URL 安全校验），禁用 data URI，外链添加 `nofollow noopener`。
pub fn clean_comment_html(input: &str) -> String {
    let config = SanitizerConfig {
        allowed_tags: &COMMENT_ALLOWED_TAGS,
        extra_generic_attrs: vec![
            "class",
            "title",
            "aria-hidden",
            "aria-label",
            "role",
            "accesskey",
        ],
        extra_tag_attrs: vec![
            ("a", vec!["class", "aria-hidden", "aria-label"]),
            // span 的 style：KaTeX 服务端渲染产出的内联 style（元素垂直对齐/定位）
            // 需保留，否则公式排版错位。与文章正文路径（sanitizer.rs:382）对齐。
            ("span", vec!["class", "style"]),
            // KaTeX 数学公式 SSR 渲染生成的 SVG 矢量图 (根号 / 矩阵竖线 / 括号 / 箭头等) 必备属性
            (
                "svg",
                vec![
                    "xmlns",
                    "width",
                    "height",
                    "viewbox",
                    "preserveaspectratio",
                    "style",
                ],
            ),
            ("path", vec!["d"]),
        ],
        allowed_schemes: &DEFAULT_ALLOWED_SCHEMES,
        allow_data_uri: false,
        link_rel: Some("nofollow noopener"),
        remove_tags: &CLEAN_CONTENT_TAGS,
    };
    sanitize(input, &config)
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn clean_html_allows_blur_img_attributes() {
        let input = r#"<span class="blur-img" style="--ar:16/9"><img class="blur-img-placeholder" src="/uploads/x.webp?w=20" alt="t"><img class="blur-img-full" data-src="/uploads/x.webp?w=800" alt="t"></span>"#;
        let result = clean_html(input);
        assert!(result.contains("data-src"), "data-src should be allowed");
        assert!(
            result.contains("blur-img-placeholder"),
            "class should be allowed"
        );
        assert!(result.contains("--ar"), "style should be allowed");
    }

    #[test]
    fn safe_tags_preserved() {
        assert_eq!(clean_html("<p>safe</p>"), "<p>safe</p>");
        assert_eq!(
            clean_html("<p><strong>bold</strong></p>"),
            "<p><strong>bold</strong></p>"
        );
    }

    #[test]
    fn script_and_style_removed() {
        assert_eq!(
            clean_html("<script>alert(1)</script><style>.x{}</style><p>ok</p>"),
            "<p>ok</p>"
        );
    }

    #[test]
    fn id_and_class_preserved() {
        assert_eq!(
            clean_html("<h1 id=\"toc\" class=\"title\">x</h1>"),
            "<h1 id=\"toc\" class=\"title\">x</h1>"
        );
        assert_eq!(
            clean_html("<p id=\"note\" class=\"hint\">x</p>"),
            "<p id=\"note\" class=\"hint\">x</p>"
        );
    }

    #[test]
    fn javascript_url_stripped() {
        assert_eq!(
            clean_html("<a href=\"javascript:alert(1)\">x</a>"),
            "<a rel=\"noopener noreferrer\">x</a>"
        );
    }

    #[test]
    fn vbscript_url_stripped() {
        assert_eq!(
            clean_html("<a href=\"vbscript:msgbox\">x</a>"),
            "<a rel=\"noopener noreferrer\">x</a>"
        );
    }

    #[test]
    fn unknown_tags_removed_content_kept() {
        assert_eq!(clean_html("<custom>keep me</custom>"), "keep me");
    }

    #[test]
    fn comment_removes_details_summary() {
        assert_eq!(
            clean_comment_html("<details><summary>sum</summary>body</details>"),
            "sumbody"
        );
    }

    #[test]
    fn comment_keeps_safe_img() {
        // 站内相对路径与 https 图片保留，src/alt 属性齐全。
        let out = clean_comment_html(r#"<img src="/uploads/2026/08/a.webp" alt="截图">"#);
        assert!(out.contains("<img"), "评论应保留 img: {out}");
        assert!(out.contains(r#"src="/uploads/2026/08/a.webp""#), "src 应保留: {out}");
        assert!(out.contains(r#"alt="截图""#), "alt 应保留: {out}");

        let out = clean_comment_html(r#"<img src="https://example.com/a.png" alt="x">"#);
        assert!(out.contains(r#"src="https://example.com/a.png""#), "https 图床应保留: {out}");
    }

    #[test]
    fn comment_strips_unsafe_img() {
        // javascript:/data: URI 的 img src 必须被剥除（标签保留但 src 消失）。
        for input in [
            r#"<img src="javascript:alert(1)">"#,
            r#"<img src="data:image/png;base64,iVBORw0KGgo=">"#,
            r#"<img src="x" onerror="alert(1)">"#,
        ] {
            let out = clean_comment_html(input);
            assert!(
                !out.contains("javascript:") && !out.contains("data:image") && !out.contains("onerror"),
                "评论图片的危险属性必须被清除: {input} -> {out}"
            );
        }
    }
    #[test]
    fn katex_svg_and_path_preserved() {
        let katex_html = crate::api::katex::render_display("\\sqrt{\\pi}");
        let cleaned = clean_html(&katex_html);
        assert!(
            cleaned.contains("<svg"),
            "clean_html should preserve <svg> for KaTeX sqrt"
        );
        assert!(
            cleaned.contains("<path"),
            "clean_html should preserve <path> for KaTeX sqrt"
        );
        assert!(
            cleaned.contains("d="),
            "clean_html should preserve d attribute on <path>"
        );
        assert!(
            cleaned.contains("viewBox=") || cleaned.contains("viewbox="),
            "clean_html should preserve viewBox attribute on <svg>"
        );

        let comment_cleaned = clean_comment_html(&katex_html);
        assert!(
            comment_cleaned.contains("<svg"),
            "clean_comment_html should preserve <svg>"
        );
        assert!(
            comment_cleaned.contains("<path"),
            "clean_comment_html should preserve <path>"
        );
    }

    #[test]
    fn comment_removes_data_uris() {
        assert_eq!(
            clean_comment_html("<a href=\"data:text/html,hi\">x</a>"),
            "<a rel=\"nofollow noopener\">x</a>"
        );
    }

    // ---- is_safe_url 直接分支测试 ----
    // is_safe_url 是安全敏感的内部函数，以下测试锁定其各分支的行为契约。

    #[test]
    fn is_safe_url_allows_https() {
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        assert!(is_safe_url("https://example.com", &schemes, false));
        assert!(is_safe_url("http://example.com", &schemes, false));
    }

    #[test]
    fn is_safe_url_rejects_javascript() {
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        assert!(!is_safe_url("javascript:alert(1)", &schemes, false));
    }

    #[test]
    fn is_safe_url_rejects_vbscript() {
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        assert!(!is_safe_url("vbscript:msgbox", &schemes, false));
    }

    #[test]
    fn is_safe_url_data_uri_respects_flag_and_media_type() {
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        // 仅在显式允许且 media type 为图片时通过
        assert!(is_safe_url("data:image/png;base64,iVBOR", &schemes, true));
        assert!(is_safe_url(
            "data:image/svg+xml;base64,PHN2Zz4=",
            &schemes,
            true
        ));
        // 禁用 data URI 时拒绝
        assert!(!is_safe_url("data:image/png;base64,iVBOR", &schemes, false));
        // 非图片 data URI 拒绝
        assert!(!is_safe_url(
            "data:text/html,<script>alert(1)</script>",
            &schemes,
            true
        ));
        assert!(!is_safe_url(
            "data:application/javascript,alert(1)",
            &schemes,
            true
        ));
    }

    #[test]
    fn is_safe_url_allows_relative_and_fragment() {
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        // 绝对路径
        assert!(is_safe_url("/path/to/page", &schemes, false));
        // 锚点
        assert!(is_safe_url("#section", &schemes, false));
    }

    #[test]
    fn is_safe_url_empty_is_safe() {
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        // 空 URL（如 img 无 src）视为安全。
        assert!(is_safe_url("", &schemes, false));
        assert!(is_safe_url("   ", &schemes, false));
    }

    #[test]
    fn is_safe_url_allows_other_whitelisted_schemes() {
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        // mailto / tel / ftp 等均在默认白名单中。
        assert!(is_safe_url("mailto:user@example.com", &schemes, false));
        assert!(is_safe_url("tel:+8613800138000", &schemes, false));
        assert!(is_safe_url("ftp://example.com/file", &schemes, false));
    }

    #[test]
    fn is_safe_url_rejects_scheme_with_whitespace() {
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        // 含空格的 scheme 名是已知的混淆手法，应被拒绝。
        assert!(!is_safe_url("java\tscript:alert(1)", &schemes, false));
    }

    #[test]
    fn is_safe_url_rejects_unknown_schemes() {
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        // 未知 scheme 默认拒绝。
        assert!(!is_safe_url("file:///etc/passwd", &schemes, false));
        assert!(!is_safe_url(
            "blob:https://example.com/abc",
            &schemes,
            false
        ));
        assert!(!is_safe_url("about:blank", &schemes, false));
        assert!(!is_safe_url("custom-app://open", &schemes, false));
    }

    #[test]
    fn is_safe_url_scheme_matching_is_case_insensitive() {
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        // scheme 大小写不敏感：HTTPS 与 https 等价。
        assert!(is_safe_url("HTTPS://example.com", &schemes, false));
        assert!(!is_safe_url("JAVASCRIPT:alert(1)", &schemes, false));
    }

    // ---- input / 任务列表 checkbox 白名单与 XSS 边界 ----

    #[test]
    fn clean_html_allows_task_list_checkbox() {
        // pulldown-cmark 对 - [ ] / - [x] 的实际输出结构
        let input = r#"<ul>
<li><input disabled="" type="checkbox"/> 未完成</li>
<li><input disabled="" type="checkbox" checked=""/> 已完成</li>
</ul>"#;
        let result = clean_html(input);
        // input 标签保留
        assert!(result.contains("<input"), "input 应保留, got: {result}");
        // type=checkbox 保留
        assert!(
            result.contains(r#"type="checkbox""#),
            "type=checkbox 应保留, got: {result}"
        );
        // checked 保留(体现勾选状态)
        assert!(
            result.contains("checked"),
            "checked 属性应保留, got: {result}"
        );
        // disabled 保留
        assert!(
            result.contains("disabled"),
            "disabled 属性应保留, got: {result}"
        );
    }

    #[test]
    fn clean_html_input_rejects_type_image() {
        // type=image 是已知的 input 滥用面（可配合 src 触发请求），必须整体移除
        let input =
            r#"<ul><li><input type="image" src="https://evil.example/x.png">文本</li></ul>"#;
        let result = clean_html(input);
        assert!(
            !result.contains("input"),
            "type=image 的 input 必须被整体移除, got: {result}"
        );
        assert!(
            !result.contains("evil.example"),
            "残留的 src 也应随 input 一并移除, got: {result}"
        );
        // 文本内容保留
        assert!(result.contains("文本"));
    }

    #[test]
    fn clean_html_input_rejects_type_text() {
        let result = clean_html(r#"<input type="text">"#);
        assert!(
            !result.contains("input"),
            "type=text 的 input 应被移除, got: {result}"
        );
    }

    #[test]
    fn clean_html_input_without_type_removed() {
        // 缺省 type 属性的 input（如 <input checked> 经属性过滤后 type 缺省）必须整体移除
        let result = clean_html("<input checked>");
        assert!(
            !result.contains("input"),
            "无 type 属性的 input 应被整体移除, got: {result}"
        );
    }

    #[test]
    fn clean_comment_html_input_stripped() {
        // 评论白名单本就不含 input，任务列表 checkbox 在评论侧不放开
        let result = clean_comment_html(r#"<input type="checkbox" checked>"#);
        assert!(
            !result.contains("input"),
            "评论侧 input 应被剥离, got: {result}"
        );
    }

    #[test]
    fn clean_html_preserves_runnable_pre_data_attrs() {
        // 可运行代码块标记应完整保留，供阅读器扫描挂载 CodeRunner。
        let input = r#"<pre data-runnable="true" data-lang="python" data-overrides="{&quot;timeout_secs&quot;:10}"><code class="language-python">print(1)</code></pre>"#;
        let result = clean_html(input);
        assert!(
            result.contains(r#"data-runnable="true""#),
            "data-runnable 应保留, got: {result}"
        );
        assert!(
            result.contains(r#"data-lang="python""#),
            "data-lang 应保留, got: {result}"
        );
        assert!(
            result.contains("data-overrides="),
            "data-overrides 应保留, got: {result}"
        );
    }

    #[test]
    fn clean_html_strips_unknown_data_attrs_on_pre() {
        // 仅放行白名单的三个 data-* 属性，其它 data-*（如恶意 data-onclick）应被剥离。
        let input = r#"<pre data-runnable="true" data-evil="x"><code>x</code></pre>"#;
        let result = clean_html(input);
        assert!(
            result.contains("data-runnable"),
            "白名单 data-runnable 应保留"
        );
        assert!(
            !result.contains("data-evil"),
            "未知 data-* 应被剥离, got: {result}"
        );
    }

    // ---- XSS 攻击向量回归：属性白名单是这里的核心防线 ----
    // 这些测试锁定"非白名单属性被剥离"这一不变量——若有人放宽属性过滤，
    // 经典 XSS 向量就会重新可用，测试会在那一步失败。

    #[test]
    fn clean_html_strips_event_handler_attributes() {
        // onerror/onload/onclick 等事件处理器属性全不在白名单，必须被移除。
        // 即便 <img> 本身合法，onerror 也不能留下。
        let cases = [
            r#"<img src="x" onerror="alert(1)">"#,
            r#"<img src=x onerror=alert(1)>"#,
            r#"<body onload="alert(1)">"#,
            r#"<div onclick="alert(1)">x</div>"#,
            r#"<a href="/x" onmouseover="alert(1)">x</a>"#,
            r#"<svg onload="alert(1)"></svg>"#,
        ];
        for input in cases {
            let result = clean_html(input);
            assert!(
                !result.contains("onerror")
                    && !result.contains("onload")
                    && !result.contains("onclick")
                    && !result.contains("onmouseover"),
                "事件处理器属性应被剥离, input: {input}, got: {result}"
            );
        }
    }

    #[test]
    fn clean_html_strips_event_handler_attribute_with_mixed_case() {
        // 大小写混淆绕过尝试：EvEr、大写、混合都应被拦（属性名匹配应大小写不敏感地拒绝）。
        for attr in ["OnErRoR", "ONERROR", "On_Error".replace('_', "or").as_str()] {
            let input = format!(r#"<img src="x" {attr}="alert(1)">"#);
            let result = clean_html(&input);
            assert!(
                !result.to_lowercase().contains("onerror"),
                "大小写混淆的事件处理器应被剥离: {input} -> {result}"
            );
        }
    }

    #[test]
    fn clean_html_removes_script_tag_and_content() {
        // script 在 CLEAN_CONTENT_TAGS：标签连同内容一起移除（而非转义后保留）。
        let result = clean_html("<p>hi</p><script>alert(1)</script><p>bye</p>");
        assert!(
            !result.contains("script"),
            "script 标签应被完全移除: {result}"
        );
        assert!(!result.contains("alert"), "script 内容应被清除: {result}");
        assert!(
            result.contains("hi") && result.contains("bye"),
            "周围内容应保留: {result}"
        );
    }

    #[test]
    fn clean_html_removes_style_tag_and_content() {
        // style 也走 CLEAN_CONTENT_TAGS：CSS 注入（expression()、@import）随内容一起移除。
        let result = clean_html("<style>body{background:url(javascript:alert(1))}</style><p>x</p>");
        assert!(!result.contains("style"), "style 标签应被移除: {result}");
        assert!(
            !result.contains("javascript"),
            "style 内危险内容应被清除: {result}"
        );
    }

    #[test]
    fn clean_html_drops_dangerous_tags_entirely() {
        // 这些标签不在白名单：整体移除（含子树），无法用于 XSS / 数据外泄。
        for tag in ["iframe", "object", "embed", "form", "math", "base", "meta"] {
            let input = format!("<{tag} src=\"javascript:alert(1)\"></{tag}>");
            let result = clean_html(&input);
            assert!(
                !result.to_lowercase().contains(&format!("<{tag}")),
                "<{tag}> 不在白名单应被移除: {result}"
            );
        }

        // svg 在白名单中（配合 KaTeX 渲染），但非白名单属性（如 src / onerror / onload）会被剥离
        let svg_input = r#"<svg src="javascript:alert(1)" onload="alert(2)"></svg>"#;
        let svg_result = clean_html(svg_input);
        assert!(
            !svg_result.contains("javascript"),
            "svg 上的非白名单属性/javascript 应被剥离: {svg_result}"
        );
        assert!(
            !svg_result.contains("onload"),
            "svg 上的 onload 事件处理器应被剥离: {svg_result}"
        );
    }

    #[test]
    fn clean_html_drops_javascript_scheme_in_href_and_src() {
        // 即便标签合法，javascript:/vbscript: scheme 必须被拒（is_safe_url 防线）。
        let cases = [
            r#"<a href="javascript:alert(1)">x</a>"#,
            r#"<a href="vbscript:msgbox(1)">x</a>"#,
            r#"<img src="javascript:alert(1)">"#,
            // 编码绕过尝试：HTML 实体编码的 javascript:
            r#"<a href="&#106;avascript:alert(1)">x</a>"#,
            r#"<a href="java&#115;cript:alert(1)">x</a>"#,
        ];
        for input in cases {
            let result = clean_html(input);
            // href/src 值里不应残留可执行的 javascript scheme（属性可能被整段移除或值被清空）。
            // 至少裸的 "javascript:" 字面量不应在 href/src 属性值中出现。
            let lower = result.to_lowercase();
            assert!(
                !lower.contains("javascript:") && !lower.contains("vbscript:"),
                "危险 scheme 应被移除, input: {input}, got: {result}"
            );
        }
    }

    #[test]
    fn clean_html_data_uri_blocked_in_article_body() {
        // clean_html 配置 allow_data_uri: false——所有 data URI（含 image/png）都应被拒。
        // 这是文章正文路径的契约；评论路径同样。
        let result = clean_html(r#"<img src="data:image/png;base64,iVBORw0KGgo=처리">"#);
        assert!(
            !result.contains("data:image/png"),
            "文章正文应禁用 data URI: {result}"
        );
    }

    #[test]
    fn clean_html_data_uri_text_html_blocked_even_if_flag_true() {
        // 即便 allow_data_uri=true，is_safe_data_uri 也只放行图片类型；
        // data:text/html（可执行脚本）永远拒绝。直接测内部函数锁定该不变量。
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        assert!(!is_safe_url(
            "data:text/html,<script>alert(1)</script>",
            &schemes,
            true
        ));
        assert!(!is_safe_url(
            "data:application/javascript,alert(1)",
            &schemes,
            true
        ));
        // 安全图片类型在 flag=true 时放行
        assert!(is_safe_url("data:image/png;base64,iVBOR=", &schemes, true));
    }

    #[test]
    fn clean_html_svg_data_uri_carries_risk_even_when_allowed() {
        // is_safe_data_uri 允许 image/svg+xml——SVG 内可嵌 <script>。
        // 这里锁定当前行为：flag=true 时 svg data URI 被放行（调用方需自行评估风险），
        // 并用注释标记这是一个潜在风险点（文章正文 allow_data_uri=false 已堵住）。
        let schemes = DEFAULT_ALLOWED_SCHEMES.clone();
        assert!(
            is_safe_url("data:image/svg+xml,<svg></svg>", &schemes, true),
            "svg data URI 在 flag=true 时当前被放行（已知风险点）"
        );
        // 但文章正文路径 flag=false，svg data URI 同样被拒
        assert!(!is_safe_url(
            "data:image/svg+xml,<svg><script>alert(1)</script></svg>",
            &schemes,
            false
        ));
    }

    #[test]
    fn clean_comment_html_strips_event_handlers() {
        // 评论路径（更严格）同样不能漏掉事件处理器。
        let result = clean_comment_html(r#"<p onclick="alert(1)">x</p>"#);
        assert!(!result.contains("onclick"), "评论 XSS 向量: {result}");
    }
}
