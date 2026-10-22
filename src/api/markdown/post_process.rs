//! 渲染后处理：/uploads/ 图片 blur-up 包裹与表格横向滚动容器。

/// 把 HTML 里的 /uploads/ 图片转成 blur-up 双层结构。
///
/// 仅处理 src 以 /uploads/ 开头的 img；外链图保持原样。
/// 对每个匹配的 img：
/// 1. 提取 src，解析出 rel_path（去 /uploads/ 前缀和 query）
/// 2. 查 get_image_dimensions 拿真实宽高，算 --ar（如 "16:9"）
/// 3. 生成 `<span class="blur-img" style="--ar:..">` 包裹两层 img
pub(super) fn wrap_images_with_blur(html: &str) -> String {
    wrap_images_with_blur_with(html, crate::api::image::get_image_dimensions)
}

/// `wrap_images_with_blur` 的纯函数核心，接受 dimensions 查询闭包以便单测。
///
/// `dims_fn(rel_path) -> Option<(u32, u32)>` 注入真实或测试用的 dimensions 来源，
/// 使本函数不依赖文件系统——测试可注入已知宽高，生产注入 get_image_dimensions。
fn wrap_images_with_blur_with<F>(html: &str, dims_fn: F) -> String
where
    F: Fn(&str) -> Option<(u32, u32)>,
{
    use regex::Regex;
    use std::sync::LazyLock;

    // 匹配 pulldown-cmark 产出的 <img src="..." alt="..." /> 或 <img src="..." alt="...">
    // pulldown-cmark 格式可控：src 在前，alt 在后，属性用双引号
    static IMG_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"<img\s+src="(/uploads/[^"]+)"(?:\s+alt="([^"]*)")?\s*/?>"#)
            .expect("IMG_RE 正则模式应在编译期通过校验")
    });

    IMG_RE
        .replace_all(html, |caps: &regex::Captures| {
            let src = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let alt = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            // 从 src 解析 rel_path：去 /uploads/ 前缀 + 去 query
            let rel_path = src
                .strip_prefix("/uploads/")
                .unwrap_or(src)
                .split('?')
                .next()
                .unwrap_or("");

            // 查 dimensions，算 aspect-ratio
            // 注意：CSS aspect-ratio 用斜杠分隔（width / height），不是冒号
            let ar_style = dims_fn(rel_path)
                .map(|(w, h)| format!(" style=\"--ar:{} / {};\"", w, h))
                .unwrap_or_default();

            // alt 转义（src/alt 来自 markdown，pulldown-cmark 已转义过，这里直接用）
            let alt_attr = if alt.is_empty() {
                String::new()
            } else {
                format!(" alt=\"{}\"", alt)
            };

            format!(
                "<span class=\"blur-img\"{ar}><img class=\"blur-img-placeholder\" src=\"{src}?w=20\"{alt_attr}><img class=\"blur-img-full\" data-src=\"{src}?w=800\"{alt_attr}></span>",
                ar = ar_style,
                src = src,
                alt_attr = alt_attr,
            )
        })
        .into_owned()
}

/// 把每个 `<table>...</table>` 包进可横向滚动的 `<div class="table-wrap">`。
///
/// 移动端窄屏下宽表格无法横向滚动：pulldown-cmark 产出裸 table，外层 `<main>` 又是
/// `overflow-hidden` 会把超宽内容直接裁掉。这里给 table 套一层 `table-wrap`（CSS 配
/// `overflow-x: auto`），让表格在容器内滚动而非撑破页面。
///
/// 正则匹配 pulldown-cmark 输出的 `<table ...>...</table>`（非贪婪、跨行），包裹整个
/// table 标签。对已包裹的 HTML 幂等（不会二次嵌套）——`table-wrap` div 内的 table
/// 仍会被正则命中并再次包裹，故调用方仅在渲染管线调用一次。
pub(super) fn wrap_tables(html: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    static TABLE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)<table(\s[^>]*)?>.*?</table>")
            .expect("TABLE_RE 正则模式应在编译期通过校验")
    });

    TABLE_RE
        .replace_all(html, |caps: &regex::Captures| {
            format!("<div class=\"table-wrap\">{}</div>", &caps[0])
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::markdown::clean_html;

    #[test]
    fn wrap_images_with_blur_wraps_uploads_image() {
        // 注入返回 None 的 dims_fn,验证 --ar 缺省时的结构正确性。
        // 不依赖 uploads/ 文件系统。
        let html = r#"<p><img src="/uploads/nonexistent/test.webp" alt="test"></p>"#;
        let result = wrap_images_with_blur_with(html, |_| None);
        assert!(
            result.contains("blur-img-placeholder"),
            "should have placeholder"
        );
        assert!(result.contains("blur-img-full"), "should have full layer");
        assert!(result.contains("?w=20"), "placeholder should use ?w=20");
        assert!(result.contains("?w=800"), "full should use ?w=800");
        assert!(result.contains("data-src"), "full should use data-src");
    }

    #[test]
    fn wrap_images_with_blur_skips_external_image() {
        // 外链图不进入 dims_fn,保持原样。
        let html = r#"<img src="https://example.com/img.png" alt="ext">"#;
        let result = wrap_images_with_blur_with(html, |_| None);
        assert!(
            !result.contains("blur-img"),
            "external image should not be wrapped"
        );
    }

    #[test]
    fn wrap_images_with_blur_uses_slash_in_aspect_ratio() {
        // 注入已知 dimensions,验证 --ar 用斜杠分隔(如 "--ar:800 / 600;")。
        // 此前依赖 uploads/ 真实文件,现已解耦。
        let html = r#"<img src="/uploads/2026/06/18/abc.webp" alt="t">"#;
        let result = wrap_images_with_blur_with(html, |_| Some((800, 600)));
        assert!(result.contains("--ar:"), "should have --ar");
        assert!(
            result.contains(" / "),
            "aspect-ratio must use slash separator, got: {}",
            result
        );
        assert!(result.contains("--ar:800 / 600;"), "应含精确宽高");
    }

    #[test]
    fn wrap_images_with_blur_omits_ar_when_no_dimensions() {
        // dims_fn 返回 None 时不输出 --ar,避免空 style 属性。
        let html = r#"<img src="/uploads/x.webp" alt="t">"#;
        let result = wrap_images_with_blur_with(html, |_| None);
        assert!(!result.contains("--ar"), "无 dimensions 不应有 --ar");
        // 但包裹结构仍应生成
        assert!(result.contains("blur-img"));
    }

    #[test]
    fn wrap_images_with_blur_strips_query_from_rel_path() {
        // rel_path 提取应去 query 后缀,dims_fn 收到的是去 query 的路径。
        // dims_fn 是 Fn(replace_all 要求),用 RefCell 捕获传入值。
        use std::cell::RefCell;
        let html = r#"<img src="/uploads/2026/x.webp?w=100" alt="t">"#;
        let received = RefCell::new(String::new());
        let _ = wrap_images_with_blur_with(html, |p| {
            *received.borrow_mut() = p.to_string();
            Some((100, 100))
        });
        assert_eq!(
            received.borrow().as_str(),
            "2026/x.webp",
            "rel_path 应去 query"
        );
    }

    #[test]
    fn wrap_images_with_blur_preserves_alt() {
        let html = r#"<img src="/uploads/x.webp" alt="描述">"#;
        let result = wrap_images_with_blur_with(html, |_| Some((10, 10)));
        assert!(result.contains(r#"alt="描述""#), "placeholder 应保留 alt");
        assert!(
            result.matches(r#"alt="描述""#).count() == 2,
            "两层 img 都应带 alt"
        );
    }

    #[test]
    fn wrap_images_with_blur_omits_alt_attr_when_empty() {
        let html = r#"<img src="/uploads/x.webp">"#;
        let result = wrap_images_with_blur_with(html, |_| None);
        assert!(!result.contains("alt=\""), "无 alt 时不应生成空 alt 属性");
    }

    #[test]
    fn full_pipeline_wrap_then_clean_preserves_slash() {
        // 模拟完整渲染管线:wrap → clean_html,验证 sanitizer 不破坏斜杠。
        // 注入确定 dimensions,脱离文件系统依赖。
        let html = r#"<img src="/uploads/2026/06/18/abc.webp" alt="t">"#;
        let wrapped = wrap_images_with_blur_with(html, |_| Some((800, 600)));
        let cleaned = clean_html(&wrapped);
        assert!(
            cleaned.contains(" / "),
            "clean_html must preserve slash in --ar, got: {}",
            cleaned
        );
    }

    // ---- wrap_tables 单元测试 ----

    #[test]
    fn wrap_tables_wraps_bare_table() {
        let html =
            "<table><thead><tr><th>A</th></tr></thead><tbody><tr><td>1</td></tr></tbody></table>";
        let result = wrap_tables(html);
        assert!(
            result.starts_with("<div class=\"table-wrap\"><table>")
                && result.ends_with("</table></div>"),
            "应整体包裹一层 table-wrap, got: {}",
            result
        );
    }

    #[test]
    fn wrap_tables_wraps_table_with_attributes() {
        // pulldown-cmark 不会给 table 加属性,但正则需兼容带属性/自闭合起始的 table。
        let html = r#"<table class="x"><tr><td>1</td></tr></table>"#;
        let result = wrap_tables(html);
        assert!(
            result.contains("<div class=\"table-wrap\"><table"),
            "应从 table 起始标签整体包裹, got: {}",
            result
        );
        assert!(result.ends_with("</table></div>"));
    }

    #[test]
    fn wrap_tables_wraps_multiple_tables() {
        let html =
            "<table><tr><td>1</td></tr></table>\n<p>间隔</p>\n<table><tr><td>2</td></tr></table>";
        let result = wrap_tables(html);
        let wrap_count = result.matches("<div class=\"table-wrap\">").count();
        assert_eq!(wrap_count, 2, "两个 table 应各自包裹, got: {}", result);
        // 中间段落不被误包
        assert!(result.contains("\n<p>间隔</p>\n"));
    }

    #[test]
    fn wrap_tables_handles_multiline_table() {
        // pulldown-cmark 产出的 table HTML 是单行无换行的,但正则用 (?s) 跨行兼容手写 HTML。
        let html = "<table>\n  <tr>\n    <td>1</td>\n  </tr>\n</table>";
        let result = wrap_tables(html);
        assert!(
            result.starts_with("<div class=\"table-wrap\"><table>")
                && result.ends_with("</table></div>"),
            "跨行 table 应整体包裹, got: {}",
            result
        );
    }

    #[test]
    fn wrap_tables_does_not_touch_non_table_html() {
        let html = "<p>段落</p><ul><li>项</li></ul>";
        let result = wrap_tables(html);
        assert_eq!(result, html, "无 table 时应原样返回");
    }

    #[test]
    fn wrap_tables_then_clean_preserves_div_and_table() {
        // 端到端:wrap → clean_html,确认 sanitizer 放行 div.table-wrap 与内部 table 结构。
        let html =
            "<table><thead><tr><th>H</th></tr></thead><tbody><tr><td>v</td></tr></tbody></table>";
        let wrapped = wrap_tables(html);
        let cleaned = clean_html(&wrapped);
        assert!(
            cleaned.contains(r#"<div class="table-wrap">"#),
            "clean_html 应保留 table-wrap div, got: {}",
            cleaned
        );
        assert!(
            cleaned.contains("<table>") && cleaned.contains("</table>"),
            "clean_html 应保留 table 标签, got: {}",
            cleaned
        );
    }
}
