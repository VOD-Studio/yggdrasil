//! 已存储目录 HTML（`toc_html`）的一级小节解析。
//!
//! 首页「最新一篇」的目录预览与笔记列表的「N 节」共用：只读 Markdown 渲染管线
//! 产出的嵌套 `<ul>`，取最外层列表项的链接文字，不重新渲染 Markdown。

/// 目录的一级小节：总数与前若干个标题（已解码为纯文本）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TocSections {
    pub total: usize,
    pub headings: Vec<String>,
}

/// 解析目录最外层列表的小节，最多保留 `limit` 个标题，`total` 计全部。
///
/// 目录由第一个标题的层级起算，所以「只有三级标题」的文章，其三级标题就是一级小节。
/// 层级先深后浅时渲染器会提前闭合根列表，深度不足 1 的链接也按一级处理。
pub fn toc_sections(toc_html: &str, limit: usize) -> TocSections {
    let mut sections = TocSections::default();
    let mut depth = 0usize;
    let mut rest = toc_html;
    while let Some(start) = rest.find('<') {
        let Some(len) = rest[start..].find('>') else {
            break;
        };
        let tag = &rest[start + 1..start + len];
        rest = &rest[start + len + 1..];
        match tag.split_ascii_whitespace().next().unwrap_or_default() {
            "ul" => depth += 1,
            "/ul" => depth = depth.saturating_sub(1),
            "a" => {
                let end = rest.find("</a>").unwrap_or(rest.len());
                if depth <= 1 {
                    sections.total += 1;
                    if sections.headings.len() < limit {
                        sections.headings.push(decode_text(&rest[..end]));
                    }
                }
                rest = &rest[end..];
            }
            _ => {}
        }
    }
    sections
}

/// 链接文字是转义后的纯文本：去掉残留标签，解码实体并合并空白。
fn decode_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(pos) = rest.find(['<', '&']) {
        text.push_str(&rest[..pos]);
        rest = &rest[pos..];
        if rest.starts_with('<') {
            rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
            continue;
        }
        match rest.find(';').filter(|&end| end <= 10) {
            Some(end) => match decode_entity(&rest[1..end]) {
                Some(c) => {
                    text.push(c);
                    rest = &rest[end + 1..];
                }
                None => {
                    text.push('&');
                    rest = &rest[1..];
                }
            },
            None => {
                text.push('&');
                rest = &rest[1..];
            }
        }
    }
    text.push_str(rest);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let code = name.strip_prefix('#')?;
            let value = match code.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => code.parse().ok()?,
            };
            char::from_u32(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn li(id: &str, text: &str) -> String {
        format!("<li><a href=\"#{id}\" aria-label=\"{text}\">{text}</a>")
    }

    #[test]
    fn empty_toc_has_no_sections() {
        assert_eq!(toc_sections("", 6), TocSections::default());
    }

    #[test]
    fn nested_items_are_not_sections() {
        let html = format!(
            "<ul>{}<ul>{}</li>{}</li></ul></li>{}</li></ul>",
            li("a", "背景"),
            li("a-1", "细节"),
            li("a-2", "补充"),
            li("b", "结论")
        );
        let sections = toc_sections(&html, 6);
        assert_eq!(sections.total, 2);
        assert_eq!(sections.headings, ["背景", "结论"]);
    }

    #[test]
    fn lower_level_only_toc_counts_its_top_level() {
        // 只有三级标题时目录从三级起算，它们就是一级小节。
        let html = format!("<ul>{}</li>{}</li></ul>", li("x", "甲"), li("y", "乙"));
        assert_eq!(toc_sections(&html, 6).headings, ["甲", "乙"]);
    }

    #[test]
    fn early_closed_root_still_counts_shallower_items() {
        // 先三级后二级：渲染器提前闭合根列表，后面的二级标题落在列表外。
        let html = format!(
            "<ul>{}</li></ul></li>{}</li></ul>",
            li("x", "深"),
            li("y", "浅")
        );
        let sections = toc_sections(&html, 6);
        assert_eq!(sections.total, 2);
        assert_eq!(sections.headings, ["深", "浅"]);
    }

    #[test]
    fn keeps_first_headings_and_counts_all() {
        let items: String = (1..=9)
            .map(|i| li(&format!("s{i}"), &format!("第 {i} 节")))
            .collect();
        let sections = toc_sections(&format!("<ul>{items}</li></ul>"), 6);
        assert_eq!(sections.total, 9);
        assert_eq!(sections.headings.len(), 6);
        assert_eq!(sections.headings[5], "第 6 节");
    }

    #[cfg(feature = "server")]
    #[test]
    fn reads_rendered_markdown_toc() {
        let md = "## 起步\n\n### 安装\n\n## Tom & Jerry `<T>`\n\n#### 很深\n\n## 收尾\n";
        let toc = crate::api::markdown::render_markdown_enhanced(md).toc_html;
        let sections = toc_sections(&toc, 6);
        assert_eq!(sections.total, 3);
        assert_eq!(sections.headings, ["起步", "Tom & Jerry <T>", "收尾"]);
    }

    #[test]
    fn decodes_entities_in_headings() {
        let text = crate::utils::html::escape_html("A & B <C> \"D\" 'E'");
        let html = format!(
            "<ul>{}</li>{}</li></ul>",
            li("a", &text),
            li("b", "&#169; &#xA9; &unknown; &")
        );
        let sections = toc_sections(&html, 6);
        assert_eq!(
            sections.headings,
            ["A & B <C> \"D\" 'E'", "© © &unknown; &"]
        );
    }
}
